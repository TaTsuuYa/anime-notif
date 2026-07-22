# Downloads

How a release gets from "notified" to "on disk" (or handed to an external
client), and the resolution-wait behavior that decides *when* that happens.

## Path resolution

See `docs/config.md`'s `[downloads]` section for the full override
precedence (`base_dir` → per-source → per-method → per-source+method). That
logic (`Downloads::resolve_dir`/`resolve_command` in `anime-notif-core`)
produces a `dir` and an optional `command` per `(source, method)`, which
the daemon passes to `anime-notif-download` as an already-resolved
`DownloadRequest` — the download crate itself has no knowledge of the
override rules, only "here's where to put it / what to run."

## By method

| Method | Behavior |
|---|---|
| `direct` | Async HTTP GET, streamed to `<dir>/<sanitized-filename>.<ext>`. Extension is taken from the URL path if present, else `.bin`. |
| `torrent` | If a `command` is configured for this `(source, method)`: the `.torrent` URL is handed to it (see below). Otherwise: the `.torrent` file itself is downloaded and dropped into `dir` — typically a torrent client's watch-folder. |
| `magnet` | Always handed to a command (there's nothing to download — a magnet URI *is* the payload): the configured `command`, or a platform default (`xdg-open` on Linux, `open` on macOS, `cmd /C start` on Windows). |

## Command hand-off is never a shell

A configured `command` (e.g. `"transmission-remote --add {magnet}"`) is
tokenized *before* the link is substituted in — the template is split into
words first (respecting quotes, via shell-word splitting), then any token
that is exactly `{magnet}` or `{link}` is replaced with the real value, and
the result is executed directly via `Command::new(program).args(args)`.
There is no shell in between. This means a magnet URI's `&`, `%`, and other
characters are passed through as one literal argument — they can never be
interpreted as shell syntax, so this is safe even though the link comes
from an untrusted third-party API.

Only a token that is *exactly* `{magnet}`/`{link}` is substituted — a
token like `prefix-{magnet}-suffix` is left untouched (documented, not
supported), which keeps substitution unambiguous.

Hand-off commands are launched fire-and-forget (`spawn`, not `wait`): a
torrent client or browser is expected to outlive the command that launched
it.

## Resolution-wait workflow

When an episode drops, its available resolutions might not include the one
you want yet (e.g. 480p appears minutes before 1080p). anime-notif waits
rather than notifying at whatever quality happens to exist first:

1. **Desired resolution present** (`downloads.default_resolution`): notify
   (if the category's `notify` is set) and/or auto-download (if
   `auto_download` is set) immediately, at the favourite method
   (`downloads.default_method`) if available at that resolution, else
   whatever method is available there.
2. **Desired resolution absent**: the episode is recorded as **pending**,
   tracking the best-ranked variant seen so far (per
   `downloads.resolution_fallback`'s preference order) without notifying.
   Every subsequent poll re-checks: if the desired resolution has since
   appeared, it resolves immediately as in (1); otherwise the pending
   entry's best-so-far is upgraded if a better-ranked variant showed up.
3. **Timeout**: once `downloads.resolution_wait` (default 30 minutes,
   overridable per source via the plugin's own `resolution_wait`) has
   elapsed since the episode was first seen, the best-so-far variant is
   used regardless of whether the desired resolution ever appeared.

A resolution not listed in `resolution_fallback` at all is still eligible
as a last-resort fallback — it just ranks after every listed one.

This logic lives in `anime-notif-daemon`'s `resolution` (pure ranking/
matching, exhaustively unit-tested) and `engine` (the I/O orchestration:
database updates, notifying, downloading) modules.
