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

/// A sound to play alongside a notification: either a specific audio file,
/// or a name from the freedesktop sound theme spec (e.g.
/// `"message-new-instant"`). `File` is what you want for "a custom sound
/// per source"; `Name` is for reusing a sound your desktop theme already
/// ships.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sound {
    /// Path to a specific sound file (`.oga`/`.ogg`/`.wav`, ...).
    File(PathBuf),
    /// A freedesktop sound-theme name.
    Name(String),
}

/// A notification to show the user.
///
/// Two different images can appear on a notification, and it's easy to
/// mix them up: `icon_path` is the small **app/source badge** (like a
/// program's taskbar icon — identifies *who* is notifying you, shown as a
/// small overlay in a corner by most notification daemons); `image_path`
/// is the big **content image** shown in the body (here, the show's cover
/// art). Setting only `icon_path` (as earlier versions did) makes the
/// badge-sized icon get blown up into the main image slot instead, which
/// is why cover art used to appear where the app icon should be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// Notification title (typically the series name).
    pub title: String,
    /// Notification body (typically episode/resolution/method details).
    pub body: String,
    /// Small app/source badge icon (locally cached source icon, or the
    /// bundled default app icon), if any.
    pub icon_path: Option<PathBuf>,
    /// Big content image shown in the notification body (locally cached
    /// cover art), if any. Left unset rather than falling back to
    /// anything — a notification with no cover just doesn't show one.
    pub image_path: Option<PathBuf>,
    /// Sound to play when the notification is shown, if any.
    pub sound: Option<Sound>,
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
            image_path: None,
            sound: None,
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
