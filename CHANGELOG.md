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
