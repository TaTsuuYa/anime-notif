//! Native D-Bus notifications on Linux, via `notify-rust`, with real action
//! buttons. A clicked action is delivered by hitting that action's URL
//! (the daemon's loopback control server) from a background thread —
//! [`notify_rust::NotificationHandle::wait_for_action`] blocks the calling
//! thread until the user interacts, so it's run off the main task.

use notify_rust::{Hint, Timeout};

use crate::{Notification, Notifier, NotifyError};

/// Notifier backend for Linux desktops with a running notification
/// service (D-Bus).
pub struct LinuxNotifier;

impl Notifier for LinuxNotifier {
    fn notify(&self, notification: &Notification) -> Result<(), NotifyError> {
        let mut n = notify_rust::Notification::new();
        n.summary(&notification.title);
        n.body(&notification.body);
        if let Some(icon) = &notification.icon_path {
            n.icon(&icon.to_string_lossy());
        }
        for action in &notification.actions {
            n.action(&action.id, &action.label);
        }
        // Some daemons remove a notification from history (or stop
        // delivering its ActionInvoked signal) as soon as it would
        // otherwise auto-expire; keep it around and don't time it out on
        // our own so the user has time to click an action.
        n.hint(Hint::Resident(true));
        n.timeout(Timeout::Never);

        tracing::debug!(
            title = %notification.title,
            actions = ?notification.actions.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            "showing notification"
        );

        let handle = n.show().map_err(|e| NotifyError::Backend(e.to_string()))?;

        let actions = notification.actions.clone();
        let title = notification.title.clone();
        std::thread::spawn(move || {
            handle.wait_for_action(|action_id| {
                if action_id == "__closed" {
                    tracing::debug!(%title, "notification closed without an action");
                    return;
                }
                let Some(action) = actions.iter().find(|a| a.id == action_id) else {
                    tracing::warn!(%title, %action_id, "notification action has no matching URL");
                    return;
                };
                tracing::info!(%title, %action_id, url = %action.url, "notification action clicked");
                match ureq::get(&action.url).call() {
                    Ok(_) => tracing::debug!(%title, %action_id, "action request delivered"),
                    Err(err) => {
                        tracing::warn!(%title, %action_id, %err, "failed to deliver action request to control server")
                    }
                }
            });
        });

        Ok(())
    }
}
