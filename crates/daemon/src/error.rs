//! Daemon error types.

/// Errors that can occur while processing a poll or an action.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// A database operation failed.
    #[error(transparent)]
    Store(#[from] anime_notif_store::StoreError),

    /// Showing a notification failed.
    #[error(transparent)]
    Notify(#[from] anime_notif_notify::NotifyError),

    /// Downloading/handing off a release failed.
    #[error(transparent)]
    Download(#[from] anime_notif_download::DownloadError),

    /// The `pending` table's stored best-variant JSON was corrupt.
    #[error("corrupt pending entry for series {series_id} episode {episode:?}: {source}")]
    CorruptPending {
        /// The series id.
        series_id: i64,
        /// The episode identifier.
        episode: String,
        /// Underlying JSON error.
        #[source]
        source: serde_json::Error,
    },
}

/// Errors that can occur starting the daemon.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    /// Opening the database failed.
    #[error(transparent)]
    Store(#[from] anime_notif_store::StoreError),

    /// Binding the loopback control server's socket failed.
    #[error("failed to bind control server: {0}")]
    Bind(#[from] std::io::Error),
}
