//! Offline TTL-cache tests against a temporary directory.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hongkong_pollution_skill::cache;

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "hkpollution-cache-test-{}-{tag}",
        std::process::id()
    ))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after the epoch")
        .as_secs() as i64
}

fn seed(dir: &Path, url: &str, stored_at: i64, body: &str) {
    std::fs::create_dir_all(dir).expect("create cache dir");
    let path = dir.join(format!("{:016x}.json", cache::hash(url)));
    let envelope = serde_json::json!({
        "url": url,
        "stored_at": stored_at,
        "body": body,
    });
    std::fs::write(path, envelope.to_string()).expect("write envelope");
}

#[test]
fn entries_round_trip_expire_and_report_stats() {
    let dir = temp_dir("ttl");
    let _ = std::fs::remove_dir_all(&dir);
    // SAFETY: this test binary only touches the variable in this one test.
    unsafe { std::env::set_var("HKPOLLUTION_CACHE_DIR", &dir) };

    let url = "https://example.test/cache/ttl";
    let other = "https://example.test/cache/other";

    assert!(cache::read(url, 60).is_none(), "an empty cache misses");
    seed(&dir, url, now() - 10, "{\"fresh\":true}");
    assert_eq!(
        cache::read(url, 60).as_deref(),
        Some("{\"fresh\":true}"),
        "a young entry is served"
    );
    assert!(
        cache::read(url, 5).is_none(),
        "an entry older than the TTL must miss"
    );
    assert!(cache::read(url, 0).is_none(), "a zero TTL never hits");

    seed(&dir, url, now() - 100, "{\"stale\":true}");
    assert!(
        cache::read(url, 60).is_none(),
        "a stale entry must miss even when it is the only one"
    );

    seed(&dir, other, now(), "{\"other\":true}");
    assert!(
        cache::read(other, 60).is_some(),
        "each URL has its own file"
    );
    assert_eq!(
        cache::read(url, 60),
        None,
        "one URL never reads another URL's body"
    );

    let (entries, bytes) = cache::stats();
    assert_eq!(entries, 2);
    assert!(bytes > 0);

    let path = cache::path_for(url).expect("a cache path");
    assert!(path.starts_with(&dir));
    assert!(path.extension().and_then(|value| value.to_str()) == Some("json"));

    let _ = std::fs::remove_dir_all(&dir);
}
