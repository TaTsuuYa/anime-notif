//! Resolves a source plugin location — a local file path or a remote URL to
//! a shared plugin file — into a [`CompiledSource`].

use std::path::Path;

use anime_notif_core::{CompiledSource, SourcePlugin};
use sha2::{Digest, Sha256};

use crate::error::FetchError;

/// Hex-encoded SHA-256 of `data`, used both for the remote-plugin cache
/// filename and for the optional pinned-checksum check.
pub fn sha256_hex(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn is_url(location: &str) -> bool {
    location.starts_with("http://") || location.starts_with("https://")
}

/// Resolves a source plugin location into a compiled source.
///
/// - A local path is read and compiled directly.
/// - A `http(s)://` URL is fetched and compiled; the fetched text is cached
///   under `cache_dir` (keyed by a hash of the URL) so that a transient
///   network failure on a later poll falls back to the last-known-good copy
///   instead of dropping the source entirely. Each successful fetch
///   overwrites the cache, so remote plugins are effectively hot-reloaded on
///   every call — pass the same `cache_dir` across calls to get the
///   fallback behavior.
/// - If `expected_sha256` is set, a freshly fetched remote plugin's content
///   must hash to it or the fetch is rejected (the cached copy, if any, is
///   left untouched and *not* used as a fallback in this case — a checksum
///   mismatch means the remote content changed unexpectedly, which the
///   fallback path is not meant to paper over).
pub async fn resolve_source(
    client: &reqwest::Client,
    location: &str,
    cache_dir: &Path,
    expected_sha256: Option<&str>,
) -> Result<CompiledSource, FetchError> {
    if !is_url(location) {
        return Ok(SourcePlugin::load(Path::new(location))?);
    }

    let cache_path = cache_dir.join(format!("{}.toml", sha256_hex(location)));

    let raw = match fetch_text(client, location).await {
        Ok(raw) => {
            if let Some(expected) = expected_sha256 {
                let actual = sha256_hex(&raw);
                if actual != expected {
                    return Err(FetchError::ChecksumMismatch {
                        location: location.to_string(),
                        expected: expected.to_string(),
                        actual,
                    });
                }
            }
            write_cache(&cache_path, &raw)?;
            raw
        }
        Err(e) => {
            if cache_path.exists() {
                tracing::warn!(
                    location,
                    error = %e,
                    "failed to fetch remote source plugin; using cached copy"
                );
                std::fs::read_to_string(&cache_path).map_err(|source| FetchError::CacheWrite {
                    path: cache_path.clone(),
                    source,
                })?
            } else {
                return Err(e);
            }
        }
    };

    Ok(SourcePlugin::parse(&raw, &cache_path)?)
}

fn write_cache(path: &Path, contents: &str) -> Result<(), FetchError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| FetchError::CacheWrite {
            path: path.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(path, contents).map_err(|source| FetchError::CacheWrite {
        path: path.to_path_buf(),
        source,
    })
}

async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<String, FetchError> {
    let resp = client.get(url).send().await?.error_for_status()?;
    Ok(resp.text().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const MINIMAL_PLUGIN: &str = r#"
id = "test"
endpoint = "https://example.com/api"
items = ".[]"
[fields]
series = { path = ".title" }
[fields.variant]
resolution = { default = "1080p" }
method = { default = "direct" }
link = { path = ".url" }
"#;

    #[test]
    fn loads_local_file() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(MINIMAL_PLUGIN.as_bytes()).unwrap();
        let path = file.path().to_str().unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = reqwest::Client::new();
        let cache_dir = tempfile::tempdir().unwrap();
        let compiled = rt
            .block_on(resolve_source(&client, path, cache_dir.path(), None))
            .unwrap();
        assert_eq!(compiled.id, "test");
    }

    #[tokio::test]
    async fn fetches_and_caches_remote_plugin() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/plugin.toml"))
            .respond_with(ResponseTemplate::new(200).set_body_string(MINIMAL_PLUGIN))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let cache_dir = tempfile::tempdir().unwrap();
        let url = format!("{}/plugin.toml", server.uri());

        let compiled = resolve_source(&client, &url, cache_dir.path(), None)
            .await
            .unwrap();
        assert_eq!(compiled.id, "test");

        let cache_path = cache_dir.path().join(format!("{}.toml", sha256_hex(&url)));
        assert!(cache_path.exists());
    }

    #[tokio::test]
    async fn checksum_mismatch_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/plugin.toml"))
            .respond_with(ResponseTemplate::new(200).set_body_string(MINIMAL_PLUGIN))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let cache_dir = tempfile::tempdir().unwrap();
        let url = format!("{}/plugin.toml", server.uri());

        let err = resolve_source(&client, &url, cache_dir.path(), Some("deadbeef"))
            .await
            .unwrap_err();
        assert!(matches!(err, FetchError::ChecksumMismatch { .. }));
    }

    #[tokio::test]
    async fn falls_back_to_cache_on_fetch_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/plugin.toml"))
            .respond_with(ResponseTemplate::new(200).set_body_string(MINIMAL_PLUGIN))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let cache_dir = tempfile::tempdir().unwrap();
        let url = format!("{}/plugin.toml", server.uri());

        // Prime the cache with a successful fetch.
        resolve_source(&client, &url, cache_dir.path(), None)
            .await
            .unwrap();

        // Drop the mock so the next fetch fails, then confirm we still get
        // a compiled source from the cache instead of an error.
        drop(server);
        let compiled = resolve_source(&client, &url, cache_dir.path(), None)
            .await
            .unwrap();
        assert_eq!(compiled.id, "test");
    }
}
