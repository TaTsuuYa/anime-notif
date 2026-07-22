//! Download error types.

use std::path::PathBuf;

/// Errors that can occur handing a release off for download.
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    /// The direct/torrent-file HTTP request failed.
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// Writing the downloaded file to disk failed.
    #[error("failed to write {path}: {source}")]
    Io {
        /// Path that failed to write.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A configured hand-off command couldn't be tokenized (unbalanced
    /// quotes, etc.).
    #[error("invalid command template {0:?}: {1}")]
    BadCommand(String, String),

    /// A hand-off command template had no tokens at all.
    #[error("command template is empty")]
    EmptyCommand,

    /// The hand-off command's program couldn't be launched (not found,
    /// not executable, ...).
    #[error("failed to launch {0:?}: {1}")]
    Spawn(String, #[source] std::io::Error),
}
