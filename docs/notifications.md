# Notifications

## When a notification is sent

A show's category controls this (see `docs/config.md`'s `[[categories]]`):
`notify = true` sends a notification when an episode resolves (see
`docs/downloads.md`'s resolution-wait workflow — a notification only fires
once resolution-wait has resolved the episode, not on every raw sighting of
a variant).

## Anatomy of a notification

A desktop notification has more parts than "title and body," and two of
them are easy to mix up — this tripped us up too (cover art used to show
where the app icon should be). What anime-notif sets:

| Part | What it shows | Where it comes from |
|---|---|---|
| Title | The series name | — |
| Body | Episode/resolution/method (e.g. `Episode 1121 [1080] via magnet`) | — |
| **Small badge icon** | A small icon in the corner identifying *who* sent the notification — like an app's taskbar icon | The source's own icon (`icon` in the source plugin, e.g. SubsPlease's favicon), falling back to anime-notif's bundled icon if the source has none configured or it fails to fetch |
| **Big content image** | A larger image shown in the notification body — this is where the show's cover art belongs | The release's cover art (`fields.cover` in the source plugin), if the source provides one; no image at all if not (nothing sensible to substitute for "a picture of this specific show") |
| Sound | Plays when the notification appears | `notifications.sound_file`/`sound_name`, globally or per source — see below |
| Actions | Buttons (or the click-the-body action) | See "Actions" below |

Concretely, in the underlying D-Bus notification: the badge icon is the
`app_icon` parameter, the content image is the `image-path` **hint**, which
most notification daemons render as the main picture and demote `app_icon`
to a small corner overlay once both are present. Setting only one field
for both purposes (what earlier versions of anime-notif did) makes
whichever was set fill the main image slot — which is why cover art was
appearing in the app-icon's place: it was the *only* image being set at
all.

### Sound

```toml
[notifications]
sound_file = "/usr/share/sounds/freedesktop/stereo/message-new-instant.oga"
# sound_name = "message-new-instant"   # alternative: a freedesktop sound-theme name instead of a file

[notifications.sources.subsplease]
sound_file = "/home/you/sounds/subsplease-ding.oga"
```

- `notifications.sound_file`/`sound_name` set the **default** sound for
  every notification. `sound_file` (a specific audio file) wins if both
  are set.
- `notifications.sources.<source-id>.sound_file`/`sound_name` overrides
  the default for one source's notifications specifically. Setting
  *either* field for a source uses that source's table exclusively — it
  doesn't merge with the global default (e.g. a source with only
  `sound_name` set does **not** fall back to a global `sound_file`).
- Leave everything unset (the default) to just get your notification
  daemon's own default sound behavior.

Confirmation notifications (Download/Whitelist/Blacklist results, below)
are always silent, regardless of this setting — they're a rapid-fire
response to your own click, not a new-episode alert.

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
token required on every request — persisted at `<state dir>/control_token`
and reused across restarts, specifically so a notification shown before a
restart keeps working: a token regenerated on every restart would silently
invalidate every button on every notification still on screen, which
looked exactly like "clicking it does nothing") — this is true whether the
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

The click path is logged end to end. Run `anime-notif logs --follow` (see
`docs/cli.md`) — with `RUST_LOG=debug anime-notif serve` for maximum
detail (`services.anime-notif.logLevel = "debug"` on Nix) — and click the
button again. In order, you should see:

1. `showing notification` (`anime-notif-notify`, when the notification is
   first displayed).
2. `notification action clicked` (`anime-notif-notify`, once you click).
3. `action request delivered` (`anime-notif-notify`, once the HTTP request
   to the control server succeeds).
4. `control server received an action request` (`anime-notif-daemon::control`,
   server-side receipt).
5. `handling notification action` / `action handled` (`anime-notif-daemon::engine`,
   the actual work).

Where it stops tells you what's wrong:

- **No line 2 at all**: the click never reached our code — a
  notification-daemon/desktop-environment issue, outside our control (some
  desktops don't render/deliver actions for banner popups the way they do
  for notifications opened from the message tray/calendar, for example).
- **Line 2 but not 3**: the HTTP request to the control server failed —
  check the warning logged alongside it for why (e.g. the daemon isn't
  running, or something's blocking loopback connections).
- **Line 3 but not 4**: the request never reached the server, which is
  unusual for loopback traffic and worth reporting as a bug.
- **Line 4, then `action rejected: token mismatch`**: the persisted token
  file (`<state dir>/control_token`) is missing, unreadable, or was
  deleted/regenerated since the notification was shown. Confirm it exists
  and is readable by whichever user runs `serve`.
- **Line 5 with an error/unexpected message**: the action itself failed
  for a normal, visible reason (e.g. `handle_download_action` reporting "no
  known download" because the episode was never actually seen) — the
  message explains what happened.

## Cover art and source icon caching

Both the release's cover art (`fields.cover`) and the source's badge icon
(`icon`) are fetched once and cached locally, keyed by URL hash — an image
is only ever downloaded once, regardless of how many notifications reuse
it (see `anime-notif-daemon`'s `cover` module). The bundled default icon
(`assets/icon.svg`) is embedded in the binary itself and written to the
cache directory once at startup, so it's available even with no source
icon configured and no network access.

## Security note

The control server binds to `127.0.0.1` only (never a public interface)
and every request requires the token persisted at `<state dir>/control_token`
(written with `0600` permissions on Unix) — so triggering an action
requires either the daemon's own notification callback or read access to
that file, not just network access to the machine.
