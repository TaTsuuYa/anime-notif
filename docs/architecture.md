# Architecture

anime-notif is a Cargo workspace producing a single binary (`anime-notif`)
that dispatches to either the background daemon (`serve`) or a CLI
subcommand. Crates are added incrementally as milestones land; this page is
updated alongside them (see `.claude/skills/sync-docs/`).

## Crate layout

```
crates/
  core/    anime-notif-core   — Release/DownloadMethod model, Config schema, source-plugin
                                 schema + jq/regex/default/prefix extraction engine, XDG paths
  store/   anime-notif-store  — libSQL-backed persistence (series, seen, pending, interactions)
  fetch/   anime-notif-fetch  — HTTP client: resolves source plugins (local/remote), polls endpoints
  notify/  anime-notif-notify — Notifier trait + Linux (D-Bus) backend; Windows/macOS pending
  download/anime-notif-download — direct/torrent/magnet handoff, never through a shell
  daemon/  anime-notif-daemon — scheduler, resolution-wait engine, loopback control server
  cli/     anime-notif-cli    — command parsing/dispatch, selector + category prefix matching
  anime-notif/                — binary entry point: loads config, opens the store, dispatches
```

### `core`

Owns the types every other crate depends on so they're defined once:

- [`model::Release`](../crates/core/src/model.rs) — a normalized
  resolution/method/link variant for one episode, with `dedup_key()` (unique
  per exact variant) and `episode_key()` (groups variants of the same
  episode, used by the resolution-wait workflow).
- [`config::Config`](../crates/core/src/config.rs) — the declarative config
  schema (see `docs/config.md`), including the download-path override
  resolution logic (`Downloads::resolve_dir`).
- [`paths`](../crates/core/src/paths.rs) — platform-default config/data/
  cache/download directories via the `directories` crate.

Config loading expands `${VAR}` environment references (unresolved ones are
left as-is) before TOML parsing, then `~` in path fields, then validates
category/rule/method-key consistency.

- [`jqpath::JqPath`](../crates/core/src/jqpath.rs) — a small, dependency-free
  evaluator for the jq path subset (`.a.b[].c[2]`) plugins use to point at
  JSON fields; every path means the same thing under real `jq`.
- [`source::SourcePlugin`](../crates/core/src/source.rs) — the source-plugin
  TOML schema (see `docs/sources.md`) and its `compile()` step, which
  pre-parses every jq path and regex into a [`source::CompiledSource`] so
  polling doesn't re-parse on every call.
- [`extract::extract`](../crates/core/src/extract.rs) — turns a source's raw
  JSON response into `Vec<Release>` using a `CompiledSource`'s field rules.
  Malformed items/variants are skipped and reported as warnings rather than
  aborting the whole poll.

### `fetch`

Thin HTTP glue on top of `core`'s source engine:
[`resolve_source`](../crates/fetch/src/loader.rs) turns a source location
(local path or `http(s)://` URL) into a `CompiledSource` — remote plugins
are fetched, cached under a cache directory keyed by a hash of the URL, and
fall back to the cached copy on a transient fetch failure; an optional
pinned SHA-256 checksum can be enforced on fresh fetches.
[`poll`](../crates/fetch/src/poll.rs) then fetches a source's endpoint
(method/headers/query/body from the plugin) and runs `core::extract` on the
response.

`sources/subsplease.toml` is the real, working example this project
develops and tests against; `crates/fetch/tests/subsplease.rs` validates it
both against a captured fixture and (opt-in, `--ignored`) the live API.

### `cli` and the `anime-notif` binary

`cli`'s grammar puts the selector first (`<id|alias|name> set category
liked`), which doesn't map onto `clap`'s subcommand model — so argument
parsing ([`parse`](../crates/cli/src/lib.rs)) is a small hand-rolled parser
rather than derived. [`dispatch`](../crates/cli/src/lib.rs) executes a
parsed `Command` against a `Config` + `Store`, returning the text to print
(kept separate from I/O so it's directly testable). Selector and category
prefix resolution live in [`selector.rs`](../crates/cli/src/selector.rs);
table rendering in [`table.rs`](../crates/cli/src/table.rs); the
config-file-rewriting commands (`categories add/rm`, `source add`) in
[`config_write.rs`](../crates/cli/src/config_write.rs). See `docs/cli.md`
for the full command reference.

The `anime-notif` binary (`crates/anime-notif`) is a thin entry point:
resolve the config path → load it (defaults if it doesn't exist yet) →
open the store → parse argv → dispatch. For every command except `serve`,
`dispatch` goes to `cli`; for `serve`, it initializes tracing and calls
`anime_notif_daemon::run(config)` directly instead.

### `notify` and `download`

`notify` defines the `Notifier` trait — `notify(&Notification)`, fire-and-
forget — and a Linux backend (`LinuxNotifier`, via `notify-rust`/D-Bus)
with real action buttons. Every `NotificationAction` carries a full URL
(the daemon's control server) rather than an opaque id, so both a native
button click and the (not-yet-built) plain-link fallback converge on the
same action-handling code path. `NullNotifier` records notifications
instead of showing them, used in tests and as the fallback on platforms
without a native backend yet (`docs/notifications.md`).

`download` defines the `Downloader` trait and `StdDownloader`: async HTTP
for `direct`/torrent-file fetches, and direct process spawning (never a
shell — command templates are tokenized, then only whole-token
`{magnet}`/`{link}` placeholders are substituted, then executed via
`Command::new(program).args(args)`) for command hand-off. See
`docs/downloads.md`.

### `daemon`

Ties everything together:

- [`resolution`](../crates/daemon/src/resolution.rs) — pure decision logic
  (no I/O): ranking resolutions against the fallback preference list,
  picking the best available variant (preferring the favourite method),
  and matching a title against `[[rules]]` to seed a category. Exhaustively
  unit-tested.
- [`engine::Engine`](../crates/daemon/src/engine.rs) — the I/O
  orchestration: `process_poll` (dedup → group by episode → resolve each
  group now or record as pending), `sweep_pending` (resolution-wait
  timeout fallback), and `handle_action` (download/whitelist/blacklist,
  shared by the control server and native action callbacks). Owns its
  dependencies via `Arc` (store/config/notifier/downloader) so an
  `Arc<Engine>` can be shared, `'static`, across scheduler tasks and the
  control server. Integration-tested against an in-memory store with fake
  notifier/downloader, covering the full resolution-wait state machine.
- [`scheduler`](../crates/daemon/src/scheduler.rs) — spawns one polling
  task per source on its own interval.
- [`control`](../crates/daemon/src/control.rs) — the loopback HTTP control
  server (axum, `127.0.0.1` only, random per-run token): binding is split
  from serving (`bind_listener` then `serve`) because the bound port is
  needed to build `Engine.control_base_url`, but serving needs the
  already-built `Engine` as request state.
- [`cover`](../crates/daemon/src/cover.rs) — cover art fetch/cache for
  notification icons. The default icon (`assets/icon.svg`) is embedded in
  the binary via `include_bytes!` and written out to the cache directory
  once at startup, rather than resolved as an installed data file at
  runtime — one less thing the Nix package (or any other packaging) has to
  get right.

See `docs/downloads.md` and `docs/notifications.md` for the user-facing
behavior this implements.

## Nix packaging

`flake.nix` builds the workspace with [crane](https://github.com/ipetkov/crane)
(`packages.default`/`packages.anime-notif`), including
`pkgs.rustPlatform.bindgenHook` in `nativeBuildInputs` since `libsql-ffi`
builds SQLite via bindgen (needs `libclang`, which the sandboxed `nix build`
doesn't get from an ambient environment the way an interactive `nix develop`
might). The package derivation runs the full offline test suite
(`doCheck = true`) — the live-network subsplease test is `#[ignore]`d so it
doesn't run there.

`nix/modules/nixos.nix` and `nix/modules/home-manager.nix` are both plain
functions of `self` (the flake), returning the actual module — this lets
`services.anime-notif.package` default to `self.packages.${pkgs.system}.default`
without requiring the user to also apply an overlay. Both modules expose a
`settings` option typed via `pkgs.formats.toml {}`, i.e. **freeform Nix
matching `config.toml`'s schema 1:1** rather than a hand-written typed
submodule for every nested field (the standard nixpkgs pattern for
"arbitrary app config" — see e.g. `services.prometheus.settings`) — so
anything in `docs/config.md` can be written directly as Nix.

`checks` (run by `nix flake check`) covers `clippy -D warnings`, `cargo
fmt --check`, the package build (which includes tests), a `nixosTest` that
boots a VM with the module enabled and confirms `anime-notif.service`
reaches `active` and logs "control server listening", and an eval-only
check that home-manager's `homeManagerConfiguration` accepts the
home-manager module without error.

### `store`

Wraps a single [`libsql::Connection`](https://docs.rs/libsql), which speaks
both a local SQLite file and a remote libSQL/Turso database through the same
API — `DbConfig::Local`/`DbConfig::Remote` select between them at
`Store::open` without a second code path. Schema migrations are plain SQL
files under `crates/store/migrations/`, tracked in a `_migrations` table and
applied in order.

Tables (see `crates/store/migrations/0001_init.sql`):

- `series` — one row per tracked show: category, alias (unique), last
  episode/interaction. This is the mutable half of a show's state; the
  config file never changes it.
- `seen` — dedup log keyed by `Release::dedup_key()`, so a poll never
  re-announces the same episode/resolution/method/link combination.
- `pending` — episodes seen without their desired resolution yet, keyed by
  `(series_id, episode)`; preserves `first_seen_at` across updates so the
  resolution-wait timeout is measured from first sight, not last update.
- `interactions` — an audit log (created/category/alias/download) per show.

## Config vs. database

This split is deliberate and load-bearing for the Nix story: `config.toml`
is meant to be generated by Nix/home-manager and is treated as read-only by
the running service. Everything the CLI mutates — category assignment,
aliases, interaction history, seen/pending state — lives in the database
instead, so `anime-notif <show> set category liked` never needs to rewrite a
file that might be a symlink into the Nix store.
