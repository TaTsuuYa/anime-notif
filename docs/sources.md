# Source plugins

A source plugin is a single, shareable TOML file describing how to poll a
JSON API for anime releases and where, in its response, to find each field.
No code — anyone can write and share one. This page is the format
reference; `skills/create-source-plugin/` (added once it lands) generates
one for you from a pasted API request and sample response.

## Minimal example: SubsPlease

[`sources/subsplease.toml`](../sources/subsplease.toml) is the real,
working example this project develops and tests against
(`https://subsplease.org/api/?f=latest&tz=Africa/Casablanca`):

```toml
id = "subsplease"
endpoint = "https://subsplease.org/api/"
method = "GET"
interval = "10m"
query = { f = "latest", tz = "Africa/Casablanca" }

items = ".[]"
variants = ".downloads[]"

[fields]
series  = { path = ".show" }
episode = { path = ".episode", default = "?" }
cover   = { path = ".image_url", prefix = "https://subsplease.org" }
id      = { path = ".page" }

[fields.variant]
resolution = { path = ".res", regex = "(\\d+)", default = "480" }
method     = { default = "magnet" }
link       = { path = ".magnet" }
```

Run `anime-notif source test sources/subsplease.toml` (once the CLI lands)
to fetch the live endpoint and print the normalized releases — the fastest
way to iterate on a plugin.

## Top-level fields

| Key | Required | Description |
|---|---|---|
| `id` | yes | Unique source id. Used as `Release::source_id`, in download-path overrides (`docs/config.md`), and in the database. |
| `endpoint` | yes | The API URL to poll. |
| `method` | no (`GET`) | `GET` or `POST`. |
| `interval` | no | Poll interval override (e.g. `"10m"`), falling back to `general.default_interval`. |
| `resolution_wait` | no | Resolution-wait override (e.g. `"45m"`), falling back to `downloads.resolution_wait`. See `docs/downloads.md`. |
| `headers` | no | Extra HTTP headers. Values may reference `${VAR}` (expanded from the environment when the file loads). |
| `query` | no | Query string parameters. |
| `body` | no | Request body, for `POST`. |
| `items` | yes | jq-style path to the release array in the response (see below). |
| `variants` | no (`"."`) | jq-style path, relative to each item, to its download variants. Leave unset when an item *is* a single variant (one resolution/method/link per item). |
| `fields` | yes | Field extraction rules — see below. |

Unknown keys anywhere in the file are a hard error (not silently ignored) —
this catches the single most common authoring mistake: **a bare `key = value`
placed after a `[table]` header lands inside that table, not at the top
level**, because TOML has no block/indentation scoping. Concretely:

```toml
items = ".[]"

[fields]
series = { path = ".show" }

variants = ".downloads[]"   # WRONG: this is now `fields.variants`, not
                             # the top-level `variants` — and `[fields]`
                             # has no such key, so with the schema's
                             # deny-unknown-fields check it fails to load
                             # instead of silently being dropped.
```

Fix: put every top-level key (`items`, `variants`, `endpoint`, ...) *before*
the `[fields]` table, as in the SubsPlease example above.

## jq-style paths

Paths are evaluated against JSON with a real jq-compatible subset — every
path here means the same thing under actual `jq <path>`, so you can
prototype with the `jq` CLI:

| Path | Meaning |
|---|---|
| `.` | The value itself (identity). |
| `.a` | Field `a` of an object. |
| `.a.b` | Nested field access. |
| `.a[]` | Iterate every element of array `a` (or every value of object `a`, for APIs like SubsPlease's that key releases by show name instead of using an array). |
| `.a[2]` | Index 2 of array `a` (negative indices count from the end: `.a[-1]` is the last element). |
| `.a.b[].c[2]` | Chains of the above. |

Not supported: pipes, filters, functions, or field names needing quoting —
only path expressions. If your API needs more than that, open an issue; the
subset covers every source shape encountered so far.

## Field extraction rules

Every field — `fields.series`, `fields.episode`, `fields.season`,
`fields.cover`, `fields.id`, and `fields.variant.{resolution,method,link}` —
is a `{ path?, regex?, default?, prefix? }` table, resolved in this order:

1. **`path`** (optional) — jq path, relative to the item (for series-level
   fields) or the variant (for `fields.variant.*`). Omit `path` entirely for
   a **constant** field, e.g. a source that's magnet-only:
   `method = { default = "magnet" }`.
2. **`regex`** (optional) — applied to the path's result; the first capture
   group is used, or the whole match if the pattern has no groups. A
   non-match is treated the same as a missing value (falls through to
   `default`).
3. **`default`** (optional) — used when `path` is unset, finds nothing, or
   (after `regex`) doesn't match.
4. **`prefix`** (optional) — prepended to the final string value, e.g. to
   turn a relative image/link path into an absolute URL
   (`prefix = "https://subsplease.org"` turns `/img/x.jpg` into
   `https://subsplease.org/img/x.jpg`).

`series`, `resolution`, `method`, and `link` are **required**: an item or
variant missing one (after the above resolution) is skipped and reported as
a warning (surfaced by `source test`) rather than aborting the whole poll.
`episode`, `season`, `cover`, and `id` are optional — give `episode` a
`default` if the API sometimes omits it (used as-is, e.g. `"?"`).

`method`'s final value must resolve to `direct`, `torrent`, or `magnet` (a
few synonyms like `http`/`url`/`torrent_file` are also accepted); anything
else is a warning, not a crash.

### Resolution label convention

Sources disagree on whether resolution is `"1080p"` or `"1080"`. By
convention, plugins normalize to **bare digit strings** with a `(\d+)`
regex on the raw value (as the SubsPlease example does), and
`config.toml`'s `downloads.default_resolution`/`resolution_fallback`
(`docs/config.md`) are specified the same way.

## Remote and Nix-provided sources

`config.toml`'s `sources` list accepts local paths, `http(s)://` URLs to a
shared plugin file (fetched and cached locally, with an optional pinned
SHA-256 checksum), or, in Nix, a store path from another flake's package
output (see `docs/nix.md`, added once the Nix outputs land).
