//! Errors loading source plugins and polling their endpoints.

use std::path::PathBuf;

/// Errors that can occur resolving a source plugin (local file or remote
/// URL) or polling its endpoint.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// The source plugin itself failed to load or compile.
    #[error(transparent)]
    Source(#[from] anime_notif_core::SourceError),

    /// An HTTP request (fetching a remote plugin file, or polling a
    /// source's endpoint) failed.
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// Writing the local cache of a remote plugin file failed.
    #[error("failed to write plugin cache {path}: {source}")]
    CacheWrite {
        /// Path that failed to write.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A remote plugin's content didn't match its pinned checksum.
    #[error("checksum mismatch for {location}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// The URL the plugin was fetched from.
        location: String,
        /// The checksum pinned in config.
        expected: String,
        /// The checksum actually computed.
        actual: String,
    },

    /// A source's endpoint responded with a body that wasn't valid JSON.
    #[error("source {source_id} returned invalid JSON: {error}")]
    InvalidJson {
        /// The source's id.
        source_id: String,
        /// The underlying JSON parse error.
        error: serde_json::Error,
    },
}
