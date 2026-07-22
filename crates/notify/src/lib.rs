//! Cross-platform notification sending: a small [`Notifier`] trait plus a
//! native Linux (D-Bus) backend. Windows/macOS native backends land in a
//! later milestone (`docs/notifications.md`); until then,
//! [`default_notifier`] falls back to [`NullNotifier`] there, so the
//! daemon still runs — it just doesn't pop a toast.
//!
//! Every notification's actions carry a full URL (the daemon's loopback
//! control server) rather than an opaque id: both the native D-Bus action
//! buttons and the plain-link fallback (used where native actions aren't
//! available) hit the same URL, so action handling lives in exactly one
//! place — the control server — regardless of how the click happened.

#![warn(missing_docs)]

mod error;
#[cfg(target_os = "linux")]
mod linux;

pub use error::NotifyError;

use std::path::PathBuf;
use std::sync::Mutex;

/// One clickable action on a notification: a stable id, a display label,
/// and the URL to hit when it's chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationAction {
    /// Stable identifier (e.g. `"download"`, `"whitelist"`, `"blacklist"`).
    pub id: String,
    /// Text shown on the button/link.
    pub label: String,
    /// URL to hit (the daemon's loopback control server) when this action
    /// is chosen.
    pub url: String,
}

/// A notification to show the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// Notification title (typically the series name).
    pub title: String,
    /// Notification body (typically episode/resolution/method details).
    pub body: String,
    /// Path to a locally cached icon (cover art, or the bundled app icon
    /// fallback), if any.
    pub icon_path: Option<PathBuf>,
    /// Available actions, if any.
    pub actions: Vec<NotificationAction>,
}

/// A platform notification backend.
pub trait Notifier: Send + Sync {
    /// Shows a notification. Returns once the notification has been
    /// *shown* — action clicks are delivered asynchronously (later, or
    /// never) by hitting the relevant [`NotificationAction::url`], not
    /// through this call's return value.
    fn notify(&self, notification: &Notification) -> Result<(), NotifyError>;
}

/// Records every notification it's asked to show instead of displaying
/// anything. Used in tests, and as the fallback on platforms without a
/// native backend yet.
#[derive(Debug, Default)]
pub struct NullNotifier {
    /// Every notification passed to [`Notifier::notify`], in order.
    pub sent: Mutex<Vec<Notification>>,
}

impl NullNotifier {
    /// Creates an empty recorder.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Notifier for NullNotifier {
    fn notify(&self, notification: &Notification) -> Result<(), NotifyError> {
        self.sent.lock().unwrap().push(notification.clone());
        Ok(())
    }
}

/// Picks the best available notifier for the current platform: native
/// D-Bus notifications on Linux, otherwise [`NullNotifier`] (Windows/macOS
/// native backends are not implemented yet — see `docs/notifications.md`).
pub fn default_notifier() -> Box<dyn Notifier> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxNotifier)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Box::new(NullNotifier::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_notifier_records_notifications() {
        let notifier = NullNotifier::new();
        let notification = Notification {
            title: "One Piece".into(),
            body: "Episode 1121 [1080p]".into(),
            icon_path: None,
            actions: vec![NotificationAction {
                id: "download".into(),
                label: "Download".into(),
                url: "http://127.0.0.1:9999/action?kind=download".into(),
            }],
        };
        notifier.notify(&notification).unwrap();
        let sent = notifier.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].title, "One Piece");
    }
}
