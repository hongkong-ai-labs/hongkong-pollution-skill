//! Best-effort on-disk cache for upstream GET bodies.
//!
//! The cache exists so an agent polling the MCP tools does not re-fetch the
//! same EPD or PRAISE payload on every call. It is deliberately simple: one
//! file per URL, a small JSON envelope, and a TTL decided by the caller. Every
//! I/O failure is swallowed — a broken cache must never break a command.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Envelope stored on disk.
#[derive(Serialize, Deserialize)]
struct Envelope {
    url: String,
    stored_at: i64,
    body: String,
}

/// Directory holding the cache, from `$HKPOLLUTION_CACHE_DIR`, then
/// `$XDG_CACHE_HOME/hkpollution`, then `~/.cache/hkpollution`.
#[must_use]
pub fn dir() -> Option<PathBuf> {
    if let Some(dir) = non_empty_var("HKPOLLUTION_CACHE_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Some(dir) = non_empty_var("XDG_CACHE_HOME") {
        return Some(PathBuf::from(dir).join("hkpollution"));
    }
    non_empty_var("HOME").map(|home| PathBuf::from(home).join(".cache").join("hkpollution"))
}

fn non_empty_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// FNV-1a 64, used only to name cache files; it is not a security hash.
#[must_use]
pub fn hash(url: &str) -> u64 {
    let mut value: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in url.as_bytes() {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    value
}

/// Cache file for `url`, if the platform gives us a directory to use.
#[must_use]
pub fn path_for(url: &str) -> Option<PathBuf> {
    dir().map(|dir| dir.join(format!("{:016x}.json", hash(url))))
}

/// Read a cached body when it is younger than `ttl_secs`.
///
/// A `ttl_secs` of 0 never hits, which is how `--max-age 0` and `--refresh`
/// are expressed. Returns `None` for a miss, an expired entry, an unreadable
/// file or a malformed envelope.
#[must_use]
pub fn read(url: &str, ttl_secs: u64) -> Option<String> {
    if ttl_secs == 0 {
        return None;
    }
    let path = path_for(url)?;
    let raw = std::fs::read_to_string(path).ok()?;
    let envelope: Envelope = serde_json::from_str(&raw).ok()?;
    if envelope.url != url {
        return None;
    }
    let age = now_unix().saturating_sub(envelope.stored_at);
    if age < 0 || age as u64 > ttl_secs {
        return None;
    }
    Some(envelope.body)
}

/// Store `body` for `url`, ignoring every failure.
pub fn write(url: &str, body: &str) {
    let Some(dir) = dir() else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let envelope = Envelope {
        url: url.to_string(),
        stored_at: now_unix(),
        body: body.to_string(),
    };
    let Ok(encoded) = serde_json::to_string(&envelope) else {
        return;
    };
    let Some(path) = path_for(url) else {
        return;
    };
    let temporary = path.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&temporary, encoded).is_ok() {
        let _ = std::fs::rename(&temporary, &path);
    } else {
        let _ = std::fs::remove_file(&temporary);
    }
}

/// Number of cached entries and their total size in bytes.
#[must_use]
pub fn stats() -> (usize, u64) {
    let Some(dir) = dir() else {
        return (0, 0);
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    let mut count = 0;
    let mut bytes = 0;
    for entry in entries.flatten() {
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        if let Ok(metadata) = entry.metadata() {
            count += 1;
            bytes += metadata.len();
        }
    }
    (count, bytes)
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{hash, read, write};

    #[test]
    fn hash_is_stable_and_distinct() {
        assert_eq!(
            hash("https://example.test/a"),
            hash("https://example.test/a")
        );
        assert_ne!(
            hash("https://example.test/a"),
            hash("https://example.test/b")
        );
    }

    #[test]
    fn round_trips_and_expires() {
        let dir = std::env::temp_dir().join(format!("hkpollution-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // SAFETY: tests in this file use the variable only through this helper.
        unsafe { std::env::set_var("HKPOLLUTION_CACHE_DIR", &dir) };

        let url = "https://example.test/round-trip";
        assert!(read(url, 60).is_none());
        write(url, "{\"ok\":true}");
        assert_eq!(read(url, 60).as_deref(), Some("{\"ok\":true}"));
        assert!(read(url, 0).is_none(), "a zero TTL must never hit");
        assert!(super::stats().0 >= 1);

        let (count, bytes) = super::stats();
        assert!(count >= 1 && bytes > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
