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
  (preferring one literally named `liked`, else the first such category),
  **and immediately downloads the episode you were just notified about** —
  whitelisting is meant to act on the episode in front of you, not only
  change behavior for future ones.
- **Blacklist** sets the category to one with `notify = false` and
  `auto_download = false` (preferring one literally named `uninterested`,
  else the first such category). Does not download anything.

Clicking **Download**, **Whitelist**, or **Blacklist** also pops a second,
plain (action-less) confirmation notification with the result (e.g.
`"One Piece" set to category "liked"; downloading episode 1121 [1080p]`,
or an error) — the click itself is a background HTTP request with no
browser tab to show a result in otherwise, so without this you'd have no
way to tell whether anything happened.

### Clicking the notification itself: opening the show page

If the source provides `fields.show_url` (`docs/sources.md`) and
`notifications.open_show_page` is enabled (the default — `docs/config.md`),
the notification also gets a `"default"` action — the freedesktop
convention most Linux notification daemons invoke when you click the
notification **body** itself, not one of the named buttons above. Clicking
it opens the show's page via `notifications.open_command` (or the platform
default opener) — no confirmation notification for this one, since the
browser tab opening is itself the visible confirmation.

If `open_show_page` is disabled, or the source has no `show_url`, no
`"default"` action is registered at all. We don't control what a given
notification daemon does when you click a notification with no action
registered for the body (most either do nothing or just dismiss it) — "the
notification shouldn't disappear" isn't something we can guarantee in that
case, only that *we* never wire up anything that would make it do so.

## How an action reaches the daemon

Every [`NotificationAction`](../crates/notify/src/lib.rs) carries a full
URL pointing at the daemon's **loopback control server**
(`http://127.0.0.1:<port>/action?...`, bound to `127.0.0.1` only, with a
random per-run token required on every request) — this is true whether the
action is triggered by a native button or a plain link, so action handling
(`Engine::handle_action` in `anime-notif-daemon`, one of
`download`/`whitelist`/`blacklist`/`open_show`) lives in exactly one place
regardless of how the click happened:

- **Linux**: real D-Bus notification action buttons
  (`anime-notif-notify`'s `LinuxNotifier`, via `notify-rust`). Clicking one
  runs a background thread that hits the action's URL. Notifications are
  shown resident and non-expiring (`Hint::Resident`, `Timeout::Never`), so
  they don't get torn down by the notification daemon (or time out) before
  you get to click something.
- **Windows/macOS**: not implemented yet — [`default_notifier`](../crates/notify/src/lib.rs)
  falls back to `NullNotifier` there for now (the daemon still runs and
  processes episodes; it just doesn't pop a toast). Native toast + the
  link-fallback path (for cases where actions aren't reliable) land in a
  later milestone.

### If clicking an action seems to do nothing

The click path is logged end to end at `info`/`debug` level — with
`RUST_LOG=debug` (see `docs/cli.md`'s `serve` section, or
`services.anime-notif.logLevel` on Nix), `journalctl` for the service shows,
in order: `showing notification` → `notification action clicked` (from
`anime-notif-notify`, once you click) → `action request delivered` →
`handling notification action` / `action handled` (from
`anime-notif-daemon`, once the control server receives it). If you don't
see the "clicked" line at all, the click never reached our code (a
notification-daemon/desktop-environment issue, outside our control — some
desktops don't render/deliver actions for banner popups the way they do for
notifications opened from the message tray/calendar, for example); if you
see "clicked" and "delivered" but nothing after that, check the service is
still the same one that showed the notification (a restarted daemon
generates a new per-run token, invalidating any notification still on
screen from before the restart).

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
