---
name: check
description: Run the full local gate (cargo fmt, clippy, test, nix flake check) before committing anime-notif changes. Use before every commit in this repo.
---

# check

The pre-commit gate for this repo. Run all of these; fix failures rather than skipping them.

## Steps

1. `nix develop -c cargo fmt --all -- --check`
2. `nix develop -c cargo clippy --all-targets --all-features -- -D warnings`
3. `nix develop -c cargo test --workspace --all-features`
4. `nix develop -c cargo doc --workspace --no-deps` (must build clean; `#![warn(missing_docs)]`
   is `-D`'d in CI, so undocumented public items fail this)
5. `nix flake check` (only once `flake.nix` exists — milestone 7+)
6. `git status` — confirm no stray build artifacts, `result` symlinks, or DB/cache files are staged.
7. Only commit once all of the above are clean.
