# Notifications

## When a notification is sent

A show's category controls this (see `docs/config.md`'s `[[categories]]`):
`notify = true` sends a notification when an episode resolves (see
`docs/downloads.md`'s resolution-wait workflow — a notification only fires
once resolution-wait has resolved the episode, not on every raw sighting of
a variant).

## Actions

Every notification includes a **Download** action (manually (re-)trigger
the download at the favourite method/resolution, independent of whether
the category auto-downloads). Categories that don't auto-download — the
"undecided" ones — additionally get **Whitelist** and **Blacklist**
actions, to classify the show:

- **Whitelist** sets the category to one with `auto_download = true`
  (preferring one literally named `liked`, else the first such category).
- **Blacklist** sets the category to one with `notify = false` and
  `auto_download = false` (preferring one literally named `uninterested`,
  else the first such category).

## How an action reaches the daemon

Every [`NotificationAction`](../crates/notify/src/lib.rs) carries a full
URL pointing at the daemon's **loopback control server**
(`http://127.0.0.1:<port>/action?...`, bound to `127.0.0.1` only, with a
random per-run token required on every request) — this is true whether the
action is triggered by a native button or a plain link, so action handling
(`Engine::handle_action` in `anime-notif-daemon`) lives in exactly one
place regardless of how the click happened:

- **Linux**: real D-Bus notification action buttons
  (`anime-notif-notify`'s `LinuxNotifier`, via `notify-rust`). Clicking one
  runs a background thread that hits the action's URL.
- **Windows/macOS**: not implemented yet — [`default_notifier`](../crates/notify/src/lib.rs)
  falls back to `NullNotifier` there for now (the daemon still runs and
  processes episodes; it just doesn't pop a toast). Native toast + the
  link-fallback path (for cases where actions aren't reliable) land in a
  later milestone.

## Cover art

If a release has a `cover` URL (see `docs/sources.md`), the daemon fetches
it and caches it locally (keyed by URL hash — an image is only ever
downloaded once) and sets it as the notification icon. If the source
doesn't provide one, or the fetch fails, it falls back to the bundled
default icon (`assets/icon.svg`, embedded in the binary and written to the
cache directory once at startup — see `anime-notif-daemon`'s `cover`
module).

## Security note

The control server binds to `127.0.0.1` only (never a public interface)
and every request requires the per-run token generated at daemon startup —
so triggering an action requires either the daemon's own notification
callback or knowledge of that token, not just network access to the
machine.
