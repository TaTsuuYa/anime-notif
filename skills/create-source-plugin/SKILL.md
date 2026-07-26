---
name: create-source-plugin
description: Generate an anime-notif source plugin (a TOML file) from a sample API request and response, so anime-notif can poll a new anime-release API. Use when the user wants to add support for a new anime source/tracker/API to anime-notif.
---

# create-source-plugin

anime-notif (https://github.com/TaTsuuYa/anime-notif) watches JSON APIs for
new anime releases. A **source plugin** is a small TOML file that tells it
how to poll one API and where, in its response, to find each field. This
skill generates one from a sample API request and response — no code, just
this one file.

This skill is agent-agnostic: any capable coding assistant can follow it.
It is versioned against the plugin schema in
`reference/plugin-schema.md` — if you're the anime-notif maintainer and the
schema changes, update that file (and this one if the workflow changes) in
the same commit as the code change.

## What you need from the user

1. **The API request**: endpoint URL, HTTP method, and any required
   headers/query parameters/body (e.g. an API key). If they don't know,
   ask them to open their browser's network inspector on the source's
   website and find the request that returns the release list as JSON.
2. **A sample response**: ask them to paste the actual JSON (or fetch it
   yourself if the endpoint needs no auth — a plain `curl <url>` is enough).
   A real, current sample beats a hypothetical one; field names and
   nesting must match exactly.
3. **The source's id**: a short, unique, lowercase-with-hyphens name (e.g.
   `subsplease`, `nyaa-si`). Used in file paths and config, not shown to
   the user elsewhere.

## Steps

1. **Read `reference/plugin-schema.md`** for the full field reference
   (jq-style paths, `regex`/`default`/`prefix`, the `variants` nesting, and
   — important — the TOML field-ordering gotcha it documents).

2. **Find the release list.** Look at the sample JSON's top-level shape:
   - An array → `items = ".[]"`.
   - An object keyed by something per-release (e.g. per show, per episode)
     → still `items = ".[]"` (iterating an object's values is supported).
   - A nested array, e.g. `{"data": {"releases": [...]}}` → `items = ".data.releases[]"`.

3. **Set the source's icon**, if you can find one (e.g. the site's
   `/favicon.ico`) — a fixed URL, not a jq path, set once at the top level:
   `icon = "https://example.com/favicon.ico"`. This is the small app/source
   badge shown on every notification from this source (distinct from
   `fields.cover` below, which is the big per-show content image). Optional
   — skip it if there's no obvious icon.

4. **Map the required series-level fields**, each as a jq path *relative to
   one item*:
   - `fields.series` (required) — the show's title.
   - `fields.episode` — give it `default = "?"` if the API can omit it.
   - `fields.season`, `fields.cover`, `fields.show_url`, `fields.id` —
     optional; map them if the data exists, otherwise leave them out
     entirely.
   - If `fields.cover`'s URL is relative (starts with `/`), set `prefix`
     to the site's origin (`https://example.com`) to absolutize it.
   - If the API gives you a slug/id rather than a full show-page URL, build
     one with `prefix`/`suffix` on `fields.show_url`, e.g.
     `{ path = ".slug", prefix = "https://example.com/shows/", suffix = "/" }`.
     This becomes the notification's click-to-open-show-page target.

5. **Find the download variants.** Does one item carry one
   resolution/method/link, or several (e.g. a `downloads`/`files`/`links`
   array with one entry per quality)?
   - Several, nested under a key → `variants = "<key>[]"` (a path
     *relative to the item*, e.g. `.downloads[]`).
   - Just one per item → omit `variants` (it defaults to `"."`, meaning
     "the item itself is the one variant").

6. **Map the variant fields** under `[fields.variant]`:
   - `resolution` — often needs `regex = "(\\d+)"` to normalize `"1080p"`
     or `"1080"` down to bare digits (anime-notif's convention — see the
     reference doc). Give it a sane `default` (e.g. `"480"`) in case a
     variant omits it.
   - `method` — must resolve to `direct`/`torrent`/`magnet`. If every
     release from this source is the same method (e.g. magnet-only,
     common for torrent trackers), skip `path` entirely and just set
     `default = "magnet"` (a constant field).
   - `link` — the URL or magnet URI.

7. **Check for batch releases.** Scan the sample data for any entry whose
   episode value is a *range* (e.g. `"01-22"`) rather than a single number,
   or a title/filename containing "Batch" — sources sometimes bundle a
   whole cour into one release once it's finished airing. If you find one,
   add a `[batch]` table (see `reference/plugin-schema.md`) so it's skipped
   by default rather than notifying like a normal episode. If you don't see
   any, skip this — most sources never need it.

8. **Write the file** to `sources/<id>.toml` (matching
   `reference/plugin-schema.md`'s field ordering — top-level keys like
   `items`/`variants` *before* the `[fields]` table).

9. **Validate it.** If the user has anime-notif installed:
   `anime-notif source test sources/<id>.toml` fetches the live endpoint
   and prints every normalized release plus any extraction warnings — fix
   the plugin until warnings are gone and the output looks right (if you
   added a `[batch]` table, confirm the batch entries are actually the ones
   being skipped, not something else). If they don't have it installed, at
   minimum re-check every path by hand against the sample JSON, and mention
   they should run `source test` once they do.

10. **Tell the user how to use it**: add the path (or, once published, a
   URL to it) to their `config.toml`'s `sources` list — see
   `reference/plugin-schema.md`'s bottom section — or, on NixOS/home-manager,
   `services.anime-notif.settings.sources`.

## Worked example

`examples/subsplease.toml` is a complete, real, working plugin (SubsPlease's
`?f=latest` API) — use it as a concrete reference for the shape of a
finished file, including the constant-field (`method`),
relative-URL-with-`prefix` (`cover`), and batch-detection (`[batch]`,
matching SubsPlease's real `"01-22"`-style episode ranges) patterns.
