---
name: add-command
description: Wire up a new anime-notif CLI subcommand end-to-end (clap definition, handler, DB query, test, docs). Use when adding or extending a CLI command.
---

# add-command

Adds a new CLI subcommand (or extends an existing one) in `crates/cli`, keeping the pattern
consistent with existing commands (`list`, `set`, `show`, `rm`, `categories`, `source`).

## Steps

1. Define the subcommand/args in `crates/cli/src/args.rs` using `clap` derive, matching the
   selector conventions already in place (id | alias | name, with duplicate-name handling and
   category prefix-matching reused from `crates/cli/src/selector.rs`).
2. Implement the handler in `crates/cli/src/commands/<name>.rs`, using `crates/store` queries only
   — never touch the DB ad hoc from the handler.
3. If the command mutates state the daemon cares about (category, alias, deletion), ping the
   daemon's loopback control server to refresh if it's running; if unreachable, the DB write alone
   is still authoritative (don't hard-fail).
4. Add a unit/integration test in `crates/cli/tests/` covering the happy path and the documented
   edge cases (duplicate name, ambiguous category prefix, unknown selector).
5. Update `docs/cli.md` with the new command's syntax, arguments, and example output.
6. Run the `check` skill before committing. Commit as `feat(cli): add <name> command`.
