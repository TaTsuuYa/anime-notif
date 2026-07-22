---
name: sync-docs
description: Update affected docs/*.md pages and CHANGELOG.md after a code change, so documentation never drifts from behavior. Use before committing any feature/behavior change in this repo.
---

# sync-docs

anime-notif's requirement is that documentation is updated after *every* change. This skill is
the mechanical check for that before a commit lands.

## Steps

1. From the diff (`git diff --staged`), identify what changed: config schema, source-plugin
   schema, CLI surface, notification behavior, download behavior, Nix module options, or
   architecture.
2. Update the corresponding doc(s):
   - Config keys → `docs/config.md`
   - Source/plugin schema → `docs/sources.md` **and** `skills/create-source-plugin/reference/plugin-schema.md`
     (the user-facing plugin-authoring skill is version-pinned to this schema — if one changes, so
     does the other, in the same commit)
   - CLI commands/flags → `docs/cli.md`
   - Notification behavior → `docs/notifications.md`
   - Download behavior/path resolution → `docs/downloads.md`
   - Nix options → `docs/nix.md`
   - Cross-cutting/structural change → `docs/architecture.md`
3. Add a bullet to `CHANGELOG.md` under `[Unreleased]` describing the user-visible change.
4. Re-read the updated doc section once to confirm it matches the actual code (not the intent) —
   check field names, defaults, and examples against the source.
5. Include the doc updates in the same commit as the code change — never a separate follow-up.
