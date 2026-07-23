# Changelog

All notable changes to this project are documented here. Format loosely
follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added
- Repo skeleton: `.gitignore`, GitHub Actions CI (fmt/clippy/test on
  Linux/Windows/macOS + `nix flake check`), and internal dev-helper skills
  (`add-source`, `add-command`, `sync-docs`, `check`).
- Nix flake `devShell` (Rust stable via rust-overlay, sqlite, pkg-config,
  dbus on Linux) with direnv auto-loading.
- `anime-notif-core`: the `Release`/`DownloadMethod` domain model, the
  declarative `Config` schema (general/downloads/categories/rules/sources)
  with `${VAR}` and `~` expansion, semantic validation, and download-path
  override resolution (`base_dir` → per-source → per-method →
  per-source+method).
- `anime-notif-store`: libSQL-backed persistence for series (category,
  alias, interaction history), the `seen` dedup log, and the `pending`
  resolution-wait table, with versioned SQL migrations.
- Source plugin engine in `anime-notif-core`: a dependency-free jq-path
  subset evaluator (`.a.b[].c[2]`), the shareable TOML plugin schema
  (`{ path?, regex?, default?, prefix? }` field extractors, nested
  `variants`), and extraction that skips malformed items/variants as
  warnings instead of aborting a poll. `deny_unknown_fields` everywhere in
  the config/plugin schemas, after catching a TOML-scoping bug (a bare key
  after a `[table]` header silently lands inside that table) in our own
  example plugin.
- `anime-notif-fetch`: resolves a source plugin (local file or remote URL,
  with caching + optional checksum pin) and polls its endpoint.
- Real, working example source plugin `sources/subsplease.toml`, tested
  against both a captured fixture and the live SubsPlease API.
- `docs/sources.md`: the source-plugin format reference.
- `anime-notif-cli` and the `anime-notif` binary: `list`, `<selector> set
  category|alias <value>`, `<selector> show`, `<selector> rm` (selector =
  numeric id, alias, or title; ambiguous titles change nothing and report
  how many shows matched), `categories list/add/rm`, `source
  list/add/test`. Category values match by full name or shortest
  unambiguous prefix. `categories add/rm` and `source add` rewrite
  `config.toml` (comments/formatting not preserved — documented). `serve`
  is recognized but not wired up yet.
- `docs/cli.md`: the CLI command reference.
- `anime-notif-notify`: `Notifier` trait, Linux D-Bus backend
  (`notify-rust`) with real action buttons, `NullNotifier` fallback.
  Actions carry full control-server URLs so native and (future) link-
  fallback clicks converge on one code path.
- `anime-notif-download`: `Downloader` trait and `StdDownloader` — async
  HTTP for direct/torrent-file fetches, direct process spawning (never a
  shell) for magnet/command hand-off.
- `anime-notif-daemon`: the resolution-wait engine (`resolution` for pure
  ranking/matching logic, `engine::Engine` for the I/O orchestration:
  dedup, grouping by episode, notify/download-now vs. pending, timeout
  fallback, and the download/whitelist/blacklist action handler shared by
  native callbacks and the control server), a per-source polling
  scheduler, and a loopback control server (axum, `127.0.0.1`-only,
  per-run token). `serve` is now wired up in the `anime-notif` binary and
  verified end-to-end against the live SubsPlease API (poll → extract →
  dedup → resolve → notify, with the control server's `/health` and token
  enforcement confirmed).
- Added `resolution_wait` as a per-source override in the source-plugin
  schema, and `Store::list_seen_for_episode` for the manual "Download"
  action to re-derive the favourite available variant.
- `docs/downloads.md`, `docs/notifications.md`.

### Nix packaging
- `flake.nix` now builds `anime-notif` via crane (`packages.default`, with
  `checks.anime-notif` running the full offline test suite inside the Nix
  sandbox — `pkgs.rustPlatform.bindgenHook` was needed for `libsql-ffi`'s
  bindgen-based SQLite build to work under sandboxed `nix build`, since
  that doesn't inherit an ambient libclang the way `nix develop` can).
- `nixosModules.default` (`services.anime-notif`): systemd service,
  dedicated user/group, sandboxed unit
  (`ProtectSystem`/`ProtectHome`/`NoNewPrivileges`/...), `settings` typed
  via `pkgs.formats.toml {}` so the full `config.toml` schema is available
  as Nix. Verified with a `nixosTest` that boots a VM and confirms the
  service reaches `active` and the control server logs as listening.
- `homeManagerModules.default` (`services.anime-notif`): `systemd --user`
  on Linux, a `launchd` agent on macOS. Verified with an eval-only check
  against `home-manager.lib.homeManagerConfiguration`.
- `overlays.default`, `apps.default` (`nix run`).
- Both modules default `RUST_LOG` to `info` (configurable via
  `services.anime-notif.logLevel`) — found via the `nixosTest`, which
  showed a perfectly "active" service logging nothing at all, since
  anime-notif is silent by default without `RUST_LOG` set.
- `skills/create-source-plugin/`: a user-downloadable, agent-agnostic
  skill that generates a source plugin from a pasted API request/response,
  with a self-contained schema reference and the SubsPlease plugin as a
  worked example.
- `docs/nix.md`.

### Cover art
- `anime-notif-daemon::cover`: fetches and caches a release's cover image
  (keyed by URL hash) for the notification icon, falling back to a bundled
  default icon (`assets/icon.svg`, embedded via `include_bytes!` and
  written to the cache directory once at startup rather than resolved as
  an installed data file at runtime).
