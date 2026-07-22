---
name: add-source
description: Scaffold a new anime-notif source plugin (TOML) with a matching source-test fixture and docs entry. Use when adding a new built-in/example source to this repo.
---

# add-source

Scaffolds a new source plugin under `sources/` in this repo, consistent with the schema in
`crates/core/src/source.rs` and documented in `docs/sources.md`.

## Steps

1. Ask for (or infer from a pasted sample API request + response): endpoint, method, auth
   (headers/query/body), the `items` jq path, per-field paths (`series`, `episode`, `season`,
   `cover`, `id`), and the `variants` path with `resolution`/`method`/`link`.
2. Write `sources/<id>.toml` following the schema documented in `docs/sources.md` — every field
   extractor is `{ path?, regex?, default?, prefix? }`.
3. Add a captured JSON fixture at `crates/core/tests/fixtures/<id>.json` (a real or representative
   sample response).
4. Add a `crates/core/tests/source_<id>.rs` (or a case in the existing extraction test suite) that
   loads the fixture and asserts the normalized `Release`s match expectations.
5. Run `anime-notif source test sources/<id>.toml` (or the equivalent cargo test) to confirm
   extraction works end-to-end.
6. Add one row for the source to `docs/sources.md`'s examples table.
7. Run the `check` skill before committing. Commit as `feat(sources): add <id> source plugin`.
