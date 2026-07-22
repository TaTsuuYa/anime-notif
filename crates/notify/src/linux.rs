//! Native D-Bus notifications on Linux, via `notify-rust`, with real action
//! buttons. A clicked action is delivered by hitting that action's URL
//! (the daemon's loopback control server) from a background thread —
//! [`notify_rust::NotificationHandle::wait_for_action`] blocks the calling
//! thread until the user interacts, so it's run off the main task.

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

        let handle = n.show().map_err(|e| NotifyError::Backend(e.to_string()))?;

        let actions = notification.actions.clone();
        std::thread::spawn(move || {
            handle.wait_for_action(|action_id| {
                if action_id == "__closed" || action_id == "default" {
                    return;
                }
                if let Some(action) = actions.iter().find(|a| a.id == action_id) {
                    // Fire-and-forget: the control server performs the
                    // actual state change, this is just the trigger.
                    let _ = ureq::get(&action.url).call();
                }
            });
        });

        Ok(())
    }
}
