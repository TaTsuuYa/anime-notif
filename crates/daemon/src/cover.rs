//! Cover art fetch/cache for notification icons: a release's cover image
//! is downloaded once and reused from the cache on subsequent
//! notifications for the same URL. A source that doesn't provide a cover
//! (or a fetch that fails) falls back to the bundled default icon —
//! nothing here ever blocks a notification.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const DEFAULT_ICON_BYTES: &[u8] = include_bytes!("../../../assets/icon.svg");

/// Writes the bundled default icon into `cache_dir` (once — subsequent
/// calls reuse the existing file) and returns its path.
pub fn ensure_default_icon(cache_dir: &Path) -> std::io::Result<PathBuf> {
    let path = cache_dir.join("default-icon.svg");
    if !path.exists() {
        std::fs::create_dir_all(cache_dir)?;
        std::fs::write(&path, DEFAULT_ICON_BYTES)?;
    }
    Ok(path)
}

fn cache_path_for(cache_dir: &Path, url: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let ext = url
        .rsplit('.')
        .next()
        .filter(|e| e.len() <= 5 && !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("img");
    cache_dir.join(format!("cover-{hash}.{ext}"))
}

/// Fetches and caches `url`'s image, returning the local path. An existing
/// cached copy is reused without re-fetching. Returns `None` (never an
/// error — callers should fall back to the default icon) on any failure.
pub async fn fetch_cover_cached(
    client: &reqwest::Client,
    cache_dir: &Path,
    url: &str,
) -> Option<PathBuf> {
    let path = cache_path_for(cache_dir, url);
    if path.exists() {
        return Some(path);
    }
    let response = client.get(url).send().await.ok()?.error_for_status().ok()?;
    let bytes = response.bytes().await.ok()?;
    std::fs::create_dir_all(cache_dir).ok()?;
    std::fs::write(&path, &bytes).ok()?;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_default_icon_writes_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path1 = ensure_default_icon(dir.path()).unwrap();
        assert!(path1.exists());
        let contents = std::fs::read(&path1).unwrap();
        assert_eq!(contents, DEFAULT_ICON_BYTES);

        // Second call reuses the same file (doesn't error, same path).
        let path2 = ensure_default_icon(dir.path()).unwrap();
        assert_eq!(path1, path2);
    }

    #[test]
    fn cache_path_is_stable_and_keeps_extension() {
        let dir = PathBuf::from("/cache");
        let a = cache_path_for(&dir, "https://x.test/cover.jpg");
        let b = cache_path_for(&dir, "https://x.test/cover.jpg");
        assert_eq!(a, b);
        assert_eq!(a.extension().unwrap(), "jpg");
    }

    #[test]
    fn cache_path_falls_back_to_generic_extension() {
        let dir = PathBuf::from("/cache");
        let path = cache_path_for(&dir, "https://x.test/cover?size=large");
        assert_eq!(path.extension().unwrap(), "img");
    }

    #[tokio::test]
    async fn fetch_cover_cached_reuses_existing_file_without_refetching() {
        let dir = tempfile::tempdir().unwrap();
        let url = "https://x.test/cover.jpg";
        let path = cache_path_for(dir.path(), url);
        std::fs::write(&path, b"already-cached").unwrap();

        // No mock server configured; if this tried to actually fetch it
        // would fail (connection refused) and return None. Getting the
        // cached path back proves the fetch was skipped.
        let client = reqwest::Client::new();
        let result = fetch_cover_cached(&client, dir.path(), url).await;
        assert_eq!(result, Some(path));
    }

    #[tokio::test]
    async fn fetch_cover_cached_returns_none_on_failure_not_error() {
        let dir = tempfile::tempdir().unwrap();
        let client = reqwest::Client::new();
        // Nothing listening on this port; the fetch fails, and we should
        // get None back rather than a panic/error.
        let result = fetch_cover_cached(&client, dir.path(), "http://127.0.0.1:1/nope.jpg").await;
        assert_eq!(result, None);
    }
}
