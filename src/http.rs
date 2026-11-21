//! Minimal HTTP access with timeouts, gzip and a descriptive User-Agent.

use std::time::Duration;

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

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT)
        .timeout_write(READ_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
}

/// GET `url` and return the response body as text.
///
/// Gzip responses are transparently decoded. Any transport problem, non-2xx
/// status or body error becomes [`Error::Http`].
pub fn get_text(url: &str) -> Result<String> {
    let response = agent()
        .get(url)
        .call()
        .map_err(|error| Error::http(url, error))?;
    response
        .into_string()
        .map_err(|error| Error::http(url, error))
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
