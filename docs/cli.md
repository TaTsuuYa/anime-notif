# CLI reference

`anime-notif` is a single binary: `anime-notif <command>`. Every command
except `serve` (the background daemon — lands with the daemon milestone)
reads the config file and opens the database, then exits.

The config file is `$ANIME_NOTIF_CONFIG` if set, otherwise the platform
default (see `docs/config.md`). If it doesn't exist yet, built-in defaults
are used — nothing is required before you can run `anime-notif list`.

## Show commands: `<selector> <verb>`

Unlike a conventional "verb first" CLI, per-show commands put the
**selector first**: `anime-notif <id|alias|name> <verb> ...`.

| Command | Effect |
|---|---|
| `anime-notif list` | Table of every tracked show: ID, Name, Category, Last Episode, Alias, Last Interaction. |
| `anime-notif <selector> show` | The show's row plus its full interaction history (created/category/alias/download events, newest first). |
| `anime-notif <selector> set category <value>` | Change category. `<value>` may be the full name or the shortest prefix that's still unambiguous (see below). |
| `anime-notif <selector> set alias <value>` | Change alias. Rejected if another show already has it. |
| `anime-notif <selector> rm` | Delete the show and all its recorded state (seen episodes, pending entries, interaction history). |

### Selector resolution

`<selector>` is tried, in order, as:

1. **Numeric id** — if it parses as an integer and a show has that id.
2. **Alias** — exact match.
3. **Title** — exact match. If more than one show has that exact title
   (e.g. the same series tracked from two sources), the command is
   rejected with an error naming how many shows matched — **nothing is
   changed**. Use the numeric id or an alias instead.

```
$ anime-notif list
ID  Name       Category  Last Episode  Alias  Last Interaction
--  ---------  --------  ------------  -----  -----------------------------------
1   One Piece  normal    -             -      -

$ anime-notif "One Piece" set category l
ID  Name       Category  Last Episode  Alias  Last Interaction
--  ---------  --------  ------------  -----  -----------------------------------
1   One Piece  liked     -             -      2026-07-22T22:11:20.593426270+00:00

$ anime-notif "One Piece" set alias op
$ anime-notif op show
...
History:
  2026-07-22T22:11:20.934455549+00:00  alias -> op
  2026-07-22T22:11:20.593548925+00:00  category -> liked

$ anime-notif op rm
Removed "One Piece" (id 1)
```

### Category value matching

`set category <value>` accepts a category's full name, or the shortest
prefix that matches exactly one defined category — so with the default
`liked`/`normal`/`uninterested`, a single letter (`l`, `n`, `u`) is always
enough. If you've defined categories whose names share a longer common
prefix (e.g. `normal` and `notified` both start with `n` *and* `no`), the
command reports every candidate that matched and you type more of the name
to disambiguate — there's no hardcoded "always 2 letters" rule, just
"however many letters it actually takes."

## Category commands

| Command | Effect |
|---|---|
| `anime-notif categories list` | Table of defined categories: Name, Notify, Auto-download. |
| `anime-notif categories add <name> [--notify] [--auto-download]` | Add a category. Flags default to off. |
| `anime-notif categories rm <name>` | Remove a category. Refused for an unknown name or the last remaining category. |

## Source commands

| Command | Effect |
|---|---|
| `anime-notif source list` | Lists configured source locations (paths/URLs from `config.toml`'s `sources`). |
| `anime-notif source add <path-or-url>` | Adds a source location. |
| `anime-notif source test <path-or-url>` | Loads/compiles the plugin, polls its live endpoint, and prints every normalized release plus any extraction warnings — the fast way to debug a plugin's jq paths without waiting for the daemon's poll cycle. |

```
$ anime-notif source test sources/subsplease.toml
60 release(s) extracted
- All Works Maid ep 05 [480] magnet -> magnet:?xt=urn:btih:...
- All Works Maid ep 05 [720] magnet -> magnet:?xt=urn:btih:...
...
```

## `categories add/rm` and `source add` rewrite `config.toml`

These three commands are the only ones that mutate the config file (every
other show/category-assignment mutation goes to the database — see
`docs/architecture.md`'s config-vs-database split). They work by
deserializing the whole config, changing it, and re-serializing with
`toml::to_string_pretty` — **this does not preserve comments or your
original formatting**. If your config file is managed by Nix/home-manager
(commonly a read-only symlink into the Nix store), these commands fail with
a clear I/O error; edit your Nix configuration instead.

## `serve`

Runs the background daemon in the foreground (service managers wrap it —
see `docs/nix.md` once the NixOS/home-manager modules land). Polls every
configured source on its own schedule, runs the resolution-wait workflow
(`docs/downloads.md`), and serves the loopback control server that handles
notification actions (`docs/notifications.md`).

Logs via `tracing`; control the verbosity with `RUST_LOG` (e.g.
`RUST_LOG=info anime-notif serve`, `RUST_LOG=debug` for more detail).

```
$ RUST_LOG=info anime-notif serve
INFO anime_notif_store::migrations: applied migration version=1
INFO anime_notif_daemon: control server listening control_addr=127.0.0.1:41131
INFO anime_notif_daemon: starting poller source=subsplease
```
