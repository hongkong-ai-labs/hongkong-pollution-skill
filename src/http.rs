//! Minimal HTTP access with timeouts, gzip, a descriptive User-Agent, bounded
//! retries and the optional on-disk TTL cache.
//!
//! Transport errors, `5xx` and `429` are retried; every other status fails on
//! the first attempt. All callers go through [`get_text`] or [`get_cached`], so
//! they share the same timeouts and retry policy.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::sleep;
use std::time::Duration;

use crate::cache;
use crate::error::{Error, Result};

/// User-Agent sent with every request, as requested by the data providers.
pub const USER_AGENT: &str = concat!(
    "hongkong-pollution-skill/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/hongkong-ai-labs/hongkong-pollution-skill)"
);

/// Connect timeout applied to every request.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Read timeout applied to every request.
const READ_TIMEOUT: Duration = Duration::from_secs(20);
/// Total attempts when `HKPOLLUTION_RETRIES` is unset.
const DEFAULT_ATTEMPTS: u32 = 3;
/// Upper bound accepted from `HKPOLLUTION_RETRIES`.
const MAX_ATTEMPTS: u32 = 5;
/// Waits before the second and third attempt; later attempts reuse the last.
const BACKOFF: [Duration; 2] = [Duration::from_millis(300), Duration::from_millis(900)];
/// Only a small `Retry-After` delta is worth honouring.
const RETRY_AFTER_RANGE: std::ops::RangeInclusive<u64> = 1..=5;

/// Sentinels for "no `--max-age` override" in the process-wide cache policy.
const NO_OVERRIDE: u64 = u64::MAX;

static CACHE_ENABLED: AtomicBool = AtomicBool::new(true);
static MAX_AGE: AtomicU64 = AtomicU64::new(NO_OVERRIDE);

/// Apply `--no-cache`, `--max-age` and `--refresh` to every later request in
/// this process. `max_age` replaces the per-source TTL; `Some(0)` refetches and
/// rewrites the entry.
pub fn configure_cache(no_cache: bool, max_age: Option<u64>) {
    CACHE_ENABLED.store(!no_cache, Ordering::Relaxed);
    MAX_AGE.store(max_age.unwrap_or(NO_OVERRIDE), Ordering::Relaxed);
}

fn agent() -> ureq::Agent {
    let read = Duration::from_secs(timeout_secs());
    let connect = read.min(CONNECT_TIMEOUT);
    ureq::AgentBuilder::new()
        .timeout_connect(connect)
        .timeout_read(read)
        .timeout_write(read)
        .user_agent(USER_AGENT)
        .build()
}

/// Read timeout in seconds, from `HKPOLLUTION_TIMEOUT_SECS` when set.
fn timeout_secs() -> u64 {
    std::env::var("HKPOLLUTION_TIMEOUT_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(READ_TIMEOUT.as_secs())
}

/// Total attempts, from `HKPOLLUTION_RETRIES` when set and clamped to 1..=5.
fn attempts() -> u32 {
    std::env::var("HKPOLLUTION_RETRIES")
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .map_or(DEFAULT_ATTEMPTS, |value| value.clamp(1, MAX_ATTEMPTS))
}

/// GET `url` and return the response body as text.
///
/// Gzip responses are transparently decoded. Transport problems, `5xx` and
/// `429` are retried up to [`attempts`] total attempts with the fixed backoff,
/// or with the delay carried by a `Retry-After: <seconds>` header when that
/// value is 1..=5. Every other non-2xx status fails immediately. Any terminal
/// problem becomes [`Error::Http`].
pub fn get_text(url: &str) -> Result<String> {
    let agent = agent();
    let total = attempts();
    let mut attempt = 1;
    loop {
        match request_once(&agent, url) {
            Ok(body) => return Ok(body),
            Err(failure) => {
                if attempt >= total || !failure.retryable {
                    return Err(Error::http(url, failure.message));
                }
                let wait = failure.retry_after.unwrap_or_else(|| backoff(attempt));
                sleep(wait);
                attempt += 1;
            }
        }
    }
}

/// GET `url` through the TTL cache, fetching and storing it on a miss.
///
/// `ttl_secs` is the source's fresh window; `--max-age` replaces it for this
/// run and `--no-cache` skips both the read and the write. Cache I/O errors are
/// ignored by [`crate::cache`], so a broken cache never fails a command.
pub fn get_cached(url: &str, ttl_secs: u64) -> Result<String> {
    if !CACHE_ENABLED.load(Ordering::Relaxed) {
        return get_text(url);
    }
    let ttl = match MAX_AGE.load(Ordering::Relaxed) {
        NO_OVERRIDE => ttl_secs,
        override_ttl => override_ttl,
    };
    if let Some(body) = cache::read(url, ttl) {
        return Ok(body);
    }
    let body = get_text(url)?;
    cache::write(url, &body);
    Ok(body)
}

fn backoff(attempt: u32) -> Duration {
    let index = (attempt.saturating_sub(1) as usize).min(BACKOFF.len() - 1);
    BACKOFF[index]
}

/// One attempt, preserving whether a failure is worth retrying.
struct Failure {
    message: String,
    retryable: bool,
    retry_after: Option<Duration>,
}

fn request_once(agent: &ureq::Agent, url: &str) -> std::result::Result<String, Failure> {
    match agent.get(url).call() {
        Ok(response) => response.into_string().map_err(|error| Failure {
            message: error.to_string(),
            retryable: true,
            retry_after: None,
        }),
        Err(ureq::Error::Status(code, response)) => Err(Failure {
            message: format!("status code {code}"),
            retryable: code == 429 || (500..=599).contains(&code),
            retry_after: response.header("retry-after").and_then(parse_retry_after),
        }),
        Err(ureq::Error::Transport(error)) => Err(Failure {
            message: error.to_string(),
            retryable: true,
            retry_after: None,
        }),
    }
}

/// Parse a `Retry-After` delta, accepting only whole seconds in 1..=5.
fn parse_retry_after(value: &str) -> Option<Duration> {
    let seconds: u64 = value.trim().parse().ok()?;
    RETRY_AFTER_RANGE
        .contains(&seconds)
        .then(|| Duration::from_secs(seconds))
}

/// Percent-encode a query-string value.
///
/// Everything outside the RFC 3986 unreserved set (`A-Z a-z 0-9 - _ . ~`) is
/// escaped as uppercase `%XX`, which keeps `+`, `:`, `,` and non-ASCII safe in
/// a URL. `ureq` offers no public helper, and this keeps the crate free of an
/// extra dependency.
#[must_use]
pub fn encode_query_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::parse_retry_after;
    use std::time::Duration;

    #[test]
    fn retry_after_accepts_only_short_whole_seconds() {
        assert_eq!(parse_retry_after("1"), Some(Duration::from_secs(1)));
        assert_eq!(parse_retry_after(" 5 "), Some(Duration::from_secs(5)));
        assert_eq!(parse_retry_after("0"), None);
        assert_eq!(parse_retry_after("6"), None);
        assert_eq!(parse_retry_after("-1"), None);
        assert_eq!(
            parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"),
            None,
            "HTTP-date values fall back to the fixed backoff"
        );
    }
}
