# anime-notif

A cross-platform background service that watches configurable web APIs for
new anime releases, notifies you (with your favourite resolution/download
method), and auto-downloads the shows you've marked as liked. No GUI —
everything is driven by a config file, the CLI, and Nix/home-manager.

> **Status:** early development. Core config/model/storage layer is in
> place; the fetch/CLI/daemon/download/notification/Nix-packaging layers are
> being built incrementally. See `docs/architecture.md` for what exists so
> far and the project's plan for what's next.

## Why

Anime release trackers are usually a single opinionated source with a GUI.
anime-notif instead treats each source as a **plugin**: a small TOML file
describing an API endpoint and where in its JSON response to find the show
title, episode, resolution, and download link (jq-style paths, with optional
regex extraction and defaults). Anyone can write and share one.

Key ideas:

- **Sources are plugins.** A shared TOML file, no code, describes how to
  read any JSON API (see `docs/sources.md`, added once the source engine
  lands).
- **Categories are behavior, not just labels.** `liked` (notify +
  auto-download), `normal` (notify only), `uninterested` (neither — this
  is the blacklist) are just the seeded defaults; you can redefine them.
- **Config vs. state are separate.** `config.toml` is declarative and
  Nix-generatable; per-show state (category, alias, history) lives in a
  local SQLite file or an optional remote libSQL/Turso database — a config
  switch, not a second code path.
- **Resolution-aware notifications.** If your desired resolution hasn't
  dropped yet, anime-notif waits (default 30 minutes, configurable) before
  falling back to a lower one, instead of notifying immediately at whatever
  quality happens to exist first.

## Documentation

- [`docs/architecture.md`](docs/architecture.md) — crate layout, what each
  piece owns.
- [`docs/config.md`](docs/config.md) — full `config.toml` reference.
- More pages (`cli.md`, `sources.md`, `downloads.md`, `notifications.md`,
  `nix.md`) are added as those parts of the service land.

## Development

This repo's development environment is a Nix flake:

```sh
nix develop        # or: direnv allow, if you use direnv (.envrc is provided)
cargo build --workspace
cargo test --workspace
```

See `.claude/skills/check/` for the full pre-commit gate (fmt, clippy, test,
doc, `nix flake check`).

## License

MIT.
