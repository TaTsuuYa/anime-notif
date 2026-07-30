# anime-notif source plugin schema

Authoritative copy: `docs/sources.md` in the anime-notif repo
(https://github.com/TaTsuuYa/anime-notif). This file is a self-contained
copy for use by this skill when the repo itself isn't available; keep it in
sync if you're updating the schema.

## Top-level fields

| Key | Required | Description |
|---|---|---|
| `id` | yes | Unique source id (lowercase, hyphens). Used as `Release::source_id`, in download-path overrides, and in the database. |
| `endpoint` | yes | The API URL to poll. |
| `method` | no (`GET`) | `GET` or `POST`. |
| `interval` | no | Poll interval override (e.g. `"10m"`), else the global default. |
| `resolution_wait` | no | Resolution-wait override (e.g. `"45m"`), else the global default. |
| `headers` | no | Extra HTTP headers. Values may reference `${VAR}` (expanded from the environment at load time — for API keys, etc). |
| `query` | no | Query string parameters. |
| `body` | no | Request body, for `POST`. |
| `icon` | no | A fixed URL to the source's own icon (e.g. its favicon) — shown as the small app/source badge on every notification from this source. A single constant for the whole source (unlike `fields.cover`, which is per-release). |
| `items` | yes | jq-style path to the release array in the response. |
| `variants` | no (`"."`) | jq-style path, *relative to each item*, to its download variants. Omit when an item has exactly one resolution/method/link. |
| `fields` | yes | Field extraction rules — see below. |
| `batch` | no | Batch-release detection — see below. Omit if the source never bundles multiple episodes into one release. |
| `version` | no | Versioned-release detection — see below. Omit if the source never re-releases an episode with a version suffix. |

### ⚠️ Field ordering matters (a real TOML gotcha)

TOML has no block/indentation scoping — a bare `key = value` after a
`[table]` header belongs to *that table* until the next header, however it
looks indented in your editor. **Every top-level key (`items`, `variants`,
`endpoint`, `interval`, ...) must appear *before* the `[fields]` table**:

```toml
# CORRECT
items = ".[]"
variants = ".downloads[]"

[fields]
series = { path = ".show" }
```

```toml
# WRONG — variants silently becomes `fields.variants`, which the schema
# doesn't define, and (because the schema rejects unknown fields) the file
# fails to load with a clear error rather than being silently ignored.
items = ".[]"

[fields]
series = { path = ".show" }
variants = ".downloads[]"
```

## jq-style paths

A real jq-compatible subset (test with the actual `jq` CLI if you like):

| Path | Meaning |
|---|---|
| `.` | The value itself. |
| `.a` | Field `a` of an object. |
| `.a.b` | Nested field access. |
| `.a[]` | Iterate array `a` (or every value of object `a`). |
| `.a[2]` | Index 2 of array `a` (`.a[-1]` = last element). |
| `.a.b[].c[2]` | Chains of the above. |

Not supported: pipes, filters, functions, quoted/special field names.

## Field extraction rules

Every field (`fields.series`, `fields.episode`, `fields.season`,
`fields.cover`, `fields.show_url`, `fields.id`, `fields.variant.resolution`,
`fields.variant.method`, `fields.variant.link`) is a table:

```toml
{ path = ".some.path", regex = "(\\d+)", default = "480", prefix = "https://example.com" }
```

Resolved in this order:

1. **`path`** (optional) — jq path, relative to the item (series-level
   fields) or the variant (`fields.variant.*`). Omit for a **constant**:
   `method = { default = "magnet" }`.
2. **`regex`** (optional) — applied to the path's result; first capture
   group, or the whole match if the pattern has no groups. No match =
   treated as missing (falls through to `default`).
3. **`default`** (optional) — used when `path` is unset, finds nothing, or
   the regex doesn't match.
4. **`prefix`**/**`suffix`** (optional) — prepended/appended to the final
   value. Use both to build a page URL from a bare slug:
   `show_url = { path = ".page", prefix = "https://example.com/shows/", suffix = "/" }`.

`series`, `resolution`, `method`, and `link` are **required** — an item/
variant missing one (after the above) is skipped with a warning rather than
crashing the whole poll. Everything else, including `show_url` (used as a
notification's click-to-open-show-page target), is optional.

`method`'s final value must be `direct`, `torrent`, or `magnet` (a few
synonyms like `http`/`url`/`torrent_file` are also accepted).

### Resolution convention

Sources disagree on `"1080p"` vs `"1080"`. Normalize to **bare digits**
with `regex = "(\\d+)"` on the resolution field — this matches the
convention `config.toml`'s `downloads.default_resolution`/
`resolution_fallback` are specified in.

## Batch releases

Some sources occasionally bundle several episodes into one release (e.g.
once a season finishes airing) instead of one release per episode. If the
sample data has anything like this — look for an episode value that's a
*range* (`"01-22"`) rather than a single number, or a title containing the
word "Batch" — add:

```toml
[batch]
regex = "^\\d+\\s*-\\s*\\d+$"   # matches against the extracted `episode` value by default
# path = ".title"                # set this instead to match a different field
# ignore = false                 # default true (skip batches); false includes them normally
```

Only whether `regex` *matches* matters (capture groups are ignored). If you
don't see any batch-shaped entries in the sample data, skip this table
entirely — it's optional, and most sources never need it.

## Versioned releases

Some sources occasionally re-release an episode with a fix, signalled by a
version suffix on the episode value — SubsPlease does this with `"08"` →
`"08v2"` → `"08v3"`, etc. If the sample data has anything like this, add:

```toml
[version]
regex = "^(?P<episode>\\d+)v(?P<version>\\d+)$"   # matches against the extracted `episode` value by default
# path = ".episode"                                 # set this instead to match a different field
```

`regex` **must** have two named capture groups: `episode` (the base episode
number, version suffix stripped) and `version` (the version number, digits
only) — a regex missing either name fails to load. A value that doesn't
match is unversioned (version 1, episode unchanged). If you don't see any
versioned entries in the sample data, skip this table entirely.

Unlike `[batch]`, this table only describes *how to detect* a version — it
has no `ignore` knob of its own. What to do about a detected version
(ignore every one, only act on a new version of the current latest episode,
or always act on it) is a `config.toml`-level policy (the `[versions]`
table, `docs/config.md`), not part of the plugin file.

## Using the finished plugin

Add the file's path (local) or a URL (if you're sharing it) to
`config.toml`'s `sources` list:

```toml
sources = [
  "sources/your-source.toml",
  "https://raw.githubusercontent.com/you/repo/main/your-source.toml",
]
```

On NixOS/home-manager, add it to `services.anime-notif.settings.sources`
instead (see `docs/nix.md`) — a plugin published as a flake package output
can be referenced directly as a store path there.
