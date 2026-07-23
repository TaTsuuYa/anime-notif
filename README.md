# anime-notif

A cross-platform background service that watches configurable web APIs for
new anime releases, notifies you (with your favourite resolution/download
method), and auto-downloads the shows you've marked as liked. No GUI —
everything is driven by a config file, the CLI, and Nix/home-manager.

> **Status:** the core service works end-to-end — polling, the
> resolution-wait workflow, notifications (Linux), downloads, the CLI, and
> Nix packaging are all in place and tested against the live SubsPlease
> API. Windows/macOS native notifications aren't implemented yet (the
> daemon still runs fine there, it just won't pop a toast). See
> `docs/architecture.md` for what exists and `CHANGELOG.md` for recent
> changes.

## Why

Anime release trackers are usually a single opinionated source with a GUI.
anime-notif instead treats each source as a **plugin**: a small TOML file
describing an API endpoint and where in its JSON response to find the show
title, episode, resolution, and download link (jq-style paths, with optional
regex extraction and defaults). Anyone can write and share one — see
`skills/create-source-plugin/` to generate one with an AI coding agent from
a sample API response.

Key ideas:

- **Sources are plugins.** A shared TOML file, no code, describes how to
  read any JSON API — see `docs/sources.md`. `sources/subsplease.toml` is
  a real, working example.
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
- **Downloads never touch a shell.** Hand-off commands (magnet links,
  custom torrent-client integration) are tokenized and substituted before
  being executed directly — a release's link can never be interpreted as
  shell syntax.

## Install

### Nix (recommended)

```sh
nix run github:TaTsuuYa/anime-notif -- list
```

For NixOS/home-manager modules (`services.anime-notif`), see
[`docs/nix.md`](docs/nix.md).

### From source

```sh
nix develop        # or: direnv allow (.envrc provided)
cargo build --release --workspace
./target/release/anime-notif list
```

## Quickstart

```sh
# Point at a config file (defaults to the platform config dir if unset).
export ANIME_NOTIF_CONFIG=~/.config/anime-notif/config.toml

# See what a source would extract, without running the daemon:
anime-notif source test sources/subsplease.toml

# Add it to your config, then run the daemon:
anime-notif source add sources/subsplease.toml
anime-notif serve

# Manage shows once the daemon has seen some:
anime-notif list
anime-notif "One Piece" set category liked
anime-notif "One Piece" show
```

See [`docs/cli.md`](docs/cli.md) for the full command reference and
[`docs/config.md`](docs/config.md) for `config.toml`.

## Documentation

- [`docs/architecture.md`](docs/architecture.md) — crate layout, what each
  piece owns.
- [`docs/config.md`](docs/config.md) — full `config.toml` reference.
- [`docs/sources.md`](docs/sources.md) — the source-plugin format.
- [`docs/cli.md`](docs/cli.md) — CLI command reference.
- [`docs/downloads.md`](docs/downloads.md) — download handoff and the
  resolution-wait workflow.
- [`docs/notifications.md`](docs/notifications.md) — notification actions
  and how they reach the daemon.
- [`docs/nix.md`](docs/nix.md) — flake outputs, NixOS/home-manager modules.

## Development

This repo's development environment is a Nix flake:

```sh
nix develop        # or: direnv allow, if you use direnv (.envrc is provided)
cargo build --workspace
cargo test --workspace
```

See `.claude/skills/check/` for the full pre-commit gate (fmt, clippy,
test, doc, `nix flake check` — which builds the package, runs the offline
test suite inside the Nix sandbox, and boots a NixOS VM to confirm the
systemd service actually starts).

## License

MIT.
