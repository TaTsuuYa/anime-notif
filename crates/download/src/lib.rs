//! Hands a release off to be downloaded, by method: `direct` is fetched
//! natively over HTTP; `torrent` is either dropped as a `.torrent` file
//! into a watch-folder or handed to a configured command; `magnet` is
//! handed to a configured command (or a platform-default opener). Command
//! hand-off never goes through a shell — tokens are substituted after
//! splitting, not before, so a link's characters can never be interpreted
//! as shell syntax.
//!
//! This crate only executes an already-resolved [`DownloadRequest`]; the
//! `base_dir`/per-source/per-method override precedence that produces
//! `dir`/`command` lives in `anime_notif_core::config::Downloads` and is
//! the caller's (the daemon's) responsibility.

#![warn(missing_docs)]

mod command;
mod error;
mod filename;

pub use error::DownloadError;

use std::path::{Path, PathBuf};

use anime_notif_core::DownloadMethod;

/// Opens `url` via `command_override` (a template like `"firefox {url}"`,
/// tokenized the same safe way as download hand-off commands — see the
/// module docs), or the platform default opener when unset. Used for a
/// notification's click-to-open-show-page action; deliberately independent
/// of `downloads.methods.magnet.command`, since a user who points that at
/// a torrent client would not want show-page clicks routed there too.
pub fn open_url(url: &str, command_override: Option<&str>) -> Result<(), DownloadError> {
    let tokens = command::resolve_command_tokens(command_override)?;
    command::spawn(&command::substitute(&tokens, url))
}

/// A fully resolved request to download one release variant.
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    /// How this release is delivered.
    pub method: DownloadMethod,
    /// The URL (direct/torrent) or magnet URI (magnet).
    pub link: String,
    /// Destination directory (for `direct`, or `torrent` without a
    /// configured command), already resolved via
    /// `Downloads::resolve_dir`.
    pub dir: PathBuf,
    /// Hand-off command template, already resolved via
    /// `Downloads::resolve_command` (`None` means "use the platform
    /// default" for `magnet`, or "drop a file in `dir`" for `torrent`).
    pub command: Option<String>,
    /// Suggested filename, without extension (sanitized internally).
    pub file_stem: String,
}

/// Executes a [`DownloadRequest`].
#[async_trait::async_trait]
pub trait Downloader: Send + Sync {
    /// Downloads/hands off `request`, returning the resulting local path
    /// for methods that write a file (`direct`, and `torrent` without a
    /// command override), or `request.dir` for methods that hand off to an
    /// external process.
    async fn download(&self, request: &DownloadRequest) -> Result<PathBuf, DownloadError>;
}

/// The real [`Downloader`]: async HTTP for `direct`/torrent-file fetches,
/// direct process spawning (no shell) for command hand-off.
#[derive(Debug, Clone, Default)]
pub struct StdDownloader {
    client: reqwest::Client,
}

impl StdDownloader {
    /// Creates a downloader using a default `reqwest` client.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a downloader using a caller-supplied `reqwest` client (e.g.
    /// to share one client, and its connection pool, across the daemon).
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    async fn fetch_to_file(
        &self,
        request: &DownloadRequest,
        extension: &str,
    ) -> Result<PathBuf, DownloadError> {
        std::fs::create_dir_all(&request.dir).map_err(|source| DownloadError::Io {
            path: request.dir.clone(),
            source,
        })?;

        let response = self
            .client
            .get(&request.link)
            .send()
            .await?
            .error_for_status()?;
        let ext = extension_or(&request.link, extension);
        let path = request
            .dir
            .join(format!("{}.{ext}", filename::sanitize(&request.file_stem)));

        let bytes = response.bytes().await?;
        std::fs::write(&path, &bytes).map_err(|source| DownloadError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(path)
    }
}

#[async_trait::async_trait]
impl Downloader for StdDownloader {
    async fn download(&self, request: &DownloadRequest) -> Result<PathBuf, DownloadError> {
        match request.method {
            DownloadMethod::Direct => self.fetch_to_file(request, "bin").await,
            DownloadMethod::Torrent => match &request.command {
                Some(template) => {
                    let tokens = command::resolve_command_tokens(Some(template))?;
                    command::spawn(&command::substitute(&tokens, &request.link))?;
                    Ok(request.dir.clone())
                }
                None => self.fetch_to_file(request, "torrent").await,
            },
            DownloadMethod::Magnet => {
                let tokens = command::resolve_command_tokens(request.command.as_deref())?;
                command::spawn(&command::substitute(&tokens, &request.link))?;
                Ok(request.dir.clone())
            }
        }
    }
}

/// Picks a file extension from the link's URL path, falling back to
/// `default` when the URL has none (or doesn't parse as a URL at all,
/// which shouldn't happen for a well-formed release but is handled rather
/// than panicking).
fn extension_or(link: &str, default: &str) -> String {
    url::Url::parse(link)
        .ok()
        .and_then(|u| {
            Path::new(u.path())
                .extension()
                .map(|e| e.to_string_lossy().to_string())
        })
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn extension_or_uses_url_path_extension() {
        assert_eq!(extension_or("https://x.test/file.mkv?a=b", "bin"), "mkv");
        assert_eq!(extension_or("https://x.test/file", "bin"), "bin");
        assert_eq!(extension_or("not a url", "bin"), "bin");
    }

    #[tokio::test]
    async fn direct_download_writes_file() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/ep1.mkv"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"video-bytes".to_vec()))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let downloader = StdDownloader::new();
        let request = DownloadRequest {
            method: DownloadMethod::Direct,
            link: format!("{}/ep1.mkv", server.uri()),
            dir: dir.path().to_path_buf(),
            command: None,
            file_stem: "One Piece - 1121".into(),
        };

        let path = downloader.download(&request).await.unwrap();
        assert_eq!(path.extension().unwrap(), "mkv");
        assert_eq!(std::fs::read(&path).unwrap(), b"video-bytes");
    }

    #[tokio::test]
    async fn torrent_without_command_downloads_file_to_watch_dir() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/ep1.torrent"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"torrent-bytes".to_vec()))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let downloader = StdDownloader::new();
        let request = DownloadRequest {
            method: DownloadMethod::Torrent,
            link: format!("{}/ep1.torrent", server.uri()),
            dir: dir.path().to_path_buf(),
            command: None,
            file_stem: "One Piece - 1121".into(),
        };

        let path = downloader.download(&request).await.unwrap();
        assert_eq!(path.extension().unwrap(), "torrent");
        assert_eq!(std::fs::read(&path).unwrap(), b"torrent-bytes");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn magnet_hands_off_to_configured_command() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("marker");

        let downloader = StdDownloader::new();
        let request = DownloadRequest {
            method: DownloadMethod::Magnet,
            link: "magnet:?xt=urn:btih:AAA".into(),
            dir: dir.path().to_path_buf(),
            command: Some(format!("touch {}", marker.display())),
            file_stem: "irrelevant".into(),
        };

        downloader.download(&request).await.unwrap();
        // The command was spawned fire-and-forget; give it a moment.
        for _ in 0..50 {
            if marker.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(
            marker.exists(),
            "expected {} to be created",
            marker.display()
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn open_url_uses_configured_command_override() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("opened");

        open_url(
            "https://subsplease.org/shows/example/",
            Some(&format!("touch {}", marker.display())),
        )
        .unwrap();

        for _ in 0..50 {
            if marker.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(
            marker.exists(),
            "expected {} to be created",
            marker.display()
        );
    }
}
