//! Notification error types.

/// Errors that can occur showing a notification.
#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    /// The platform notification backend rejected or failed to display the
    /// notification.
    #[error("failed to show notification: {0}")]
    Backend(String),
}
