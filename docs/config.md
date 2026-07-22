# Configuration

anime-notif is configured entirely through `config.toml` — no GUI. The file is
declarative and safe to generate from Nix/home-manager; anything the CLI
mutates (a show's category, alias, interaction history) lives in the database
instead, never in this file.

Default location: `~/.config/anime-notif/config.toml` (XDG on Linux,
`%APPDATA%\anime-notif` on Windows, `~/Library/Application Support/anime-notif`
on macOS).

Values may reference environment variables with `${VAR_NAME}` — these are
expanded when the file is loaded; a reference to an unset variable is left
as-is rather than erroring, so e.g. `auth_token = "${TURSO_TOKEN}"` only
resolves once the daemon actually has that variable in its environment.
`~` is expanded in path fields.

## `[general]`

| Key | Type | Default | Description |
|---|---|---|---|
| `default_interval` | duration string (`"15m"`, `"1h"`) | `15m` | How often to poll a source that doesn't set its own `interval`. |
| `db` | table | local, `~/.local/share/anime-notif/data.db` | Where state is stored — see below. |

```toml
[general]
default_interval = "15m"

# Local SQLite file (default):
[general.db]
kind = "local"
path = "~/.local/share/anime-notif/data.db"

# Or a remote libSQL/Turso database (same schema, one code path):
[general.db]
kind = "remote"
url = "libsql://your-db.turso.io"
auth_token = "${TURSO_TOKEN}"
```

## `[downloads]`

| Key | Type | Default | Description |
|---|---|---|---|
| `base_dir` | path | `~/Downloads/anime-notif` | Base directory when no override applies: `<base_dir>/<source>/<method>`. |
| `default_method` | `direct`\|`torrent`\|`magnet` | `direct` | Favourite method for liked/auto-downloaded shows. |
| `default_resolution` | string | `1080p` | Desired resolution. Releases below it trigger the resolution-wait workflow (see [downloads.md](downloads.md) once written). |
| `resolution_fallback` | list of strings | `["1080p","720p","480p"]` | Preference order used once `resolution_wait` elapses without the desired resolution appearing. |
| `resolution_wait` | duration string | `30m` | How long to wait for the desired resolution before falling back. |

### Path overrides

Overrides are resolved with this precedence, most specific first:

1. `[downloads.sources.<id>.methods.<method>]` — this source, this method
2. `[downloads.methods.<method>]` — this method, every source
3. `[downloads.sources.<id>]` — this source, every method
4. `<base_dir>/<source_id>/<method>` — the default

```toml
[downloads.methods.direct]
dir = "/mnt/media/direct"

[downloads.methods.torrent]
watch_dir = "/mnt/media/torrents/watch"   # torrent client's watch-folder

[downloads.methods.magnet]
command = "xdg-open {magnet}"             # or e.g. transmission-remote --add {magnet}

[downloads.sources.subsplease]
dir = "/mnt/media/subsplease"

[downloads.sources.subsplease.methods.magnet]
command = "transmission-remote --add {magnet}"
```

For `torrent`, a configured `watch_dir` takes precedence over `dir` at the
same specificity level, since a watch-folder is the more specific intent.

## `[[categories]]`

Categories are data with behavior flags, not hardcoded names — `notify`
controls whether new episodes produce a notification, `auto_download`
controls whether they're downloaded automatically. Seeded defaults:

```toml
[[categories]]
name = "liked"
notify = true
auto_download = true

[[categories]]
name = "normal"
notify = true
auto_download = false

[[categories]]
name = "uninterested"   # this is the "blacklist" — no separate list needed
notify = false
auto_download = false
```

Category names must be unique and non-empty. You can rename them or add your
own; the CLI's category-setting command matches by full name or shortest
unambiguous prefix.

## `[[rules]]`

Declarative rules seed or override a show's category by matching its title,
evaluated when a show is first seen (or re-evaluated — exact semantics land
with the daemon in a later milestone):

```toml
[[rules]]
match = "One Piece"        # exact title match
category = "liked"

[[rules]]
match_regex = "^Isekai.*"  # regex match
category = "uninterested"
```

Each rule must set exactly one of `match`/`match_regex`, and `category` must
name a category defined in `[[categories]]`.

## `sources`

A list of source plugin locations — local paths, URLs to shared plugin files,
or (in Nix) store paths from another flake. See `docs/sources.md` (added once
the source engine lands) for the plugin file format itself.

```toml
sources = [
  "sources/subsplease.toml",
  "https://raw.githubusercontent.com/user/repo/main/nyaa.toml",
]
```

## Validation

`Config::load` rejects: empty/duplicate category names, rules referencing an
undefined category or an invalid regex, and unknown download-method keys
under `[downloads.methods]`/`[downloads.sources.*.methods]`. Errors name the
offending key so they're actionable from the CLI or `nix flake check`.
