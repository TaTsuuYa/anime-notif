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

### Release packaging
- `.github/workflows/release.yml`: builds and publishes release binaries
  (Linux/Windows/macOS x86_64, macOS aarch64) with SHA-256 checksums to a
  GitHub release, triggered on `v*` tag pushes. Not yet exercised by an
  actual tag push.

### Fixed
- Real deployment (a user's own NixOS flake) caught two gaps the local
  `nixosTest` didn't: (1) the NixOS module never put `anime-notif` on
  `PATH` — `services.anime-notif` now adds the package to
  `environment.systemPackages`, with a VM-test regression check
  (`which anime-notif`); (2) notifications silently never arrive when
  `serve` runs via the **NixOS** module, because desktop notifications
  need the login session's D-Bus bus and a `systemd --system` service
  under a dedicated system user has no such bus. This isn't fixable in the
  system-service model — `docs/nix.md` now leads with a callout
  explaining it and pointing at the **home-manager** module (a
  `systemd --user` service, which does have the session bus) as the path
  that actually delivers notifications.

### Added
- Source plugins can now declare an optional `[batch]` table describing
  how to recognize a **batch release** (one release bundling multiple
  episodes, e.g. SubsPlease's `episode = "01-22"` — confirmed against a
  real batch item in the captured fixture) via a jq path + regex, and
  whether to skip them (`ignore`, defaulting to `true`). Skipped batches
  are excluded from extraction entirely and reported as an extraction
  warning rather than silently dropped. Omitting `[batch]` (existing
  plugins) never flags anything as a batch. `sources/subsplease.toml` now
  uses this for its real, previously-unhandled `Dr. Stone S3 (01-22)`
  batch entry; `docs/sources.md` and `skills/create-source-plugin/`'s
  schema reference document the feature.

### Added
- **Click-to-open-show-page.** Source plugins can declare `fields.show_url`
  (a `FieldExtractor`, same as `cover`; `FieldExtractor` also gained a
  `suffix` option alongside `prefix`, needed to build a page URL like
  `https://subsplease.org/shows/<slug>/` from a bare slug). A new
  `[notifications]` config section controls it: `open_show_page` (default
  `true`) and `open_command` (default: platform opener, independent of
  `downloads.methods.magnet.command`). When a release has a `show_url` and
  the setting is enabled, its notification gets a `"default"` action — the
  freedesktop convention for "click the notification body itself" — that
  opens the page. `sources/subsplease.toml` now sets `show_url` for real,
  confirmed against the live API. `anime-notif-download` gained a small
  `open_url` helper (and `command::substitute` a `{url}` placeholder
  alongside `{magnet}`/`{link}`) to run it, never through a shell, same as
  every other command hand-off.
- **Confirmation notifications.** Clicking Download/Whitelist/Blacklist now
  pops a second, action-less notification with the result — those clicks
  are a headless background HTTP request with no other way to show
  feedback, so without this a working click and a silently-failed one
  looked identical.

### Fixed
- **Whitelist now actually whitelists.** Clicking Whitelist changed a
  show's category but never downloaded anything — reported as "the
  default/preferred action should be triggered" when whitelisting.
  `Engine::handle_reclassify_action` now downloads the episode you were
  just notified about immediately when the target category auto-downloads
  (reusing the same favourite-resolution/method selection as the manual
  Download action, reconstructed from the `seen` log via a new shared
  `pick_best_available` helper). Blacklist is unaffected (its target
  category never auto-downloads).
- **"Clicking Download does nothing"** — no single root cause could be
  confirmed remotely (most likely a notification-daemon/desktop-environment
  quirk around action delivery for banner popups, which is outside our
  control), so this shipped two real, verifiable improvements instead of a
  guess: (1) `anime-notif-notify`'s Linux backend had a latent bug where
  the reserved `"default"` action id was unconditionally ignored — harmless
  before, but would have silently broken the new click-to-open-show-page
  feature; (2) notifications are now shown resident and non-expiring
  (`Hint::Resident`, `Timeout::Never`) rather than using the notification
  daemon's default, since a notification torn down or auto-dismissed before
  it's clicked is indistinguishable from "the click didn't work"; (3) the
  full click path is now logged at `info`/`debug` (`showing notification` →
  `notification action clicked` → `action request delivered` →
  `handling notification action` → `action handled`/`action failed`), so
  the next time this happens `journalctl` shows exactly where it stopped
  rather than nothing at all — see `docs/notifications.md`'s new
  troubleshooting section.

### Fixed (confirmed root cause of "Download does nothing")
- Found the actual bug behind the previous release's logging additions:
  the control-server token was regenerated on **every** daemon restart.
  Any notification already on screen (or in the notification history) from
  before a restart carries the *old* token in its button URLs, so clicking
  it silently gets rejected as `Forbidden` — and this wasn't logged at all
  before now, so it looked exactly like nothing happening. The token is
  now persisted at `<state dir>/control_token` (`0600` on Unix) and reused
  across restarts; `load_or_generate_token` only generates a new one if
  that file is missing or empty. Verified end-to-end: started `serve`,
  killed it, started it again, and confirmed the pre-restart token still
  authenticates against the new run.
- The control server now logs every incoming action request (accepted or
  rejected, with kind/series_id/episode) — a token mismatch used to return
  `Forbidden` with zero logging anywhere.

### Added
- `anime-notif logs [--follow|-f] [--lines|-n N] [--path]`: reads the log
  file `serve` now also writes (daily-rotating, under
  `anime_notif_core::paths::default_log_dir()`, via `tracing-appender`) —
  works identically regardless of init system, including Windows/macOS
  where `journalctl` doesn't exist. `--follow` tails it live, for watching
  the action-click path in real time while debugging.
- `serve` now logs to that file in addition to stdout/stderr (unchanged,
  so systemd/journald capture is unaffected).

### Added
- **Notification sound.** New `notifications.sound_file`/`sound_name`
  config keys set a default sound for every notification (`sound_file`, a
  specific audio file, wins if both are set), and a
  `[notifications.sources.<id>]` table (`sound_file`/`sound_name`)
  overrides it per source — setting either field there uses that source's
  table exclusively rather than merging with the global default.
  Confirmation notifications (Download/Whitelist/Blacklist results) are
  always silent. `anime-notif-notify` gained a `Sound` enum
  (`File(PathBuf)`/`Name(String)`, mapped to the `sound-file`/`sound-name`
  D-Bus hints) and `Engine::resolve_sound` implements the precedence.

### Fixed
- **Cover art was showing where the app/source icon should be.** Both
  images were being crammed into the single D-Bus `app_icon` parameter via
  one `icon_path` field, so whichever was set filled the main image slot.
  `Notification` now has two separate fields — `icon_path` (small
  app/source badge, the `app_icon` parameter) and `image_path` (big content
  image, the `image-path` hint) — and source plugins can declare a new
  top-level `icon` field (a fixed URL, e.g. a favicon) providing the
  former; `fields.cover` continues to provide the latter, per release.
  `sources/subsplease.toml` now sets `icon` to SubsPlease's favicon.
  `docs/notifications.md` gained an "Anatomy of a notification" section
  explaining the D-Bus `app_icon`-vs-`image-path` distinction end to end.

### Fixed
- **Download/Whitelist/Blacklist buttons silently not working after a
  machine restart.** Root cause: right after a restart, anime-notif's
  systemd `--user` unit can start polling before the desktop's
  notification service is registered on the session bus, so the first
  notification attempt fails
  (`org.freedesktop.DBus.Error.ServiceUnknown: The name is not
  activatable`). That failure was propagated with `?` out of
  `process_episode_group`, aborting `process_poll`'s loop over the *rest*
  of that poll's episode groups too — so any other episodes fetched in the
  same batch silently never got notified or downloaded, recoverable only
  on the next poll cycle. `Engine::send_notification` now treats a
  notifier failure as non-fatal (logs a warning and continues), matching
  the pattern `send_confirmation` already used — the specific episode
  whose notification failed is still lost (it's already marked seen by
  that point, so nothing to click for it), but every other episode in the
  batch is now processed normally regardless of ordering.
  `nix/modules/home-manager.nix`'s systemd unit is now ordered
  `After`/`PartOf`/`WantedBy` `graphical-session.target` instead of
  `default.target` (reached much earlier in the session, typically before
  the notification daemon exists) to narrow the race in the first place.
  `docs/notifications.md` and `docs/nix.md` document both the failure mode
  and the mitigation.

### Added
- **Versioned-release handling** (e.g. SubsPlease re-releasing episode
  `"08"` as `"08v2"`, `"08v3"`, ... to fix an earlier upload). Source
  plugins can now declare an optional `[version]` table (`path`?, `regex`
  with required named capture groups `episode`/`version`) that splits a
  matching episode value into its base episode and version number —
  `Release` gained a `version: u32` field (`1` for unversioned releases, or
  when the source declares no `[version]` table at all — fully
  backward-compatible). A new top-level `[versions]` config table (`mode`:
  `ignore` | `latest_only` (default) | `all`, plus `[versions.sources.<id>]`
  per-source overrides, mirroring `[notifications.sources.<id>]`'s idiom)
  controls what happens once a version is detected: never
  notify/download a version bump, only one that's a new version of the
  series' *current* latest episode, or always. In every mode, a show with
  no prior episode on record at all always processes its first-ever
  release normally even if it's already versioned, so a show can't fail to
  surface just because its earliest available release wasn't the original.
  `sources/subsplease.toml` now sets `[version]` for real (verified against
  actual SubsPlease torrent-title conventions), with a synthetic versioned
  entry added to the captured fixture to exercise it end to end.
  `docs/sources.md`, `docs/config.md`, and `skills/create-source-plugin/`
  document the new tables.

### Fixed
- `nix flake check`/`nixos-rebuild switch` printed two evaluation
  warnings on every build: `lib.getExe` guessing at the binary name
  because the package had no `meta.mainProgram`, and `pkgs.system`/
  `final.system` (deprecated in favor of `pkgs.stdenv.hostPlatform.system`)
  in both Nix modules and the overlay. Both are now set correctly; `nix
  flake check` output is warning-free.
