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
