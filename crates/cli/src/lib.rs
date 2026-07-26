//! The `anime-notif` CLI: argument parsing and command execution for
//! everything except `serve` (the daemon itself lives in
//! `anime-notif-daemon` and is wired in by the `anime-notif` binary).
//!
//! The grammar is deliberately not a conventional "verb first" CLI for
//! per-show operations: `anime-notif <selector> set category liked` puts
//! the selector (numeric id, alias, or title) first, which doesn't map onto
//! `clap`'s subcommand model without fighting it, so argument parsing here
//! is hand-rolled (see [`parse`]) rather than derived.

#![warn(missing_docs)]

mod config_write;
mod error;
pub mod logs;
mod selector;
mod table;

pub use error::CliError;

use std::path::Path;

use anime_notif_core::Config;
use anime_notif_store::Store;

/// What to do to a single resolved show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShowAction {
    /// `<selector> set <field> <value>`.
    Set {
        /// `"category"` or `"alias"`.
        field: String,
        /// The new value.
        value: String,
    },
    /// `<selector> show`.
    Show,
    /// `<selector> rm`.
    Rm,
}

/// A fully parsed top-level command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `serve` — handled by the `anime-notif` binary, not this crate.
    Serve,
    /// `list`.
    List,
    /// `categories list`.
    CategoriesList,
    /// `categories add <name> [--notify] [--auto-download]`.
    CategoriesAdd {
        /// Category name.
        name: String,
        /// Whether new episodes for this category notify.
        notify: bool,
        /// Whether new episodes for this category auto-download.
        auto_download: bool,
    },
    /// `categories rm <name>`.
    CategoriesRm {
        /// Category name.
        name: String,
    },
    /// `source list`.
    SourceList,
    /// `source add <path-or-url>`.
    SourceAdd {
        /// Local path or URL to a source plugin file.
        location: String,
    },
    /// `source test <path-or-url>`.
    SourceTest {
        /// Local path or URL to a source plugin file.
        location: String,
    },
    /// `<selector> <action>`.
    Show {
        /// Numeric id, alias, or title.
        selector: String,
        /// What to do to the resolved show.
        action: ShowAction,
    },
    /// `logs [--follow|-f] [--lines|-n N] [--path]`.
    Logs {
        /// Keep printing new lines as they're appended, like `tail -f`,
        /// after first printing the last `lines` lines. Handled directly
        /// by the `anime-notif` binary rather than [`dispatch`], since it
        /// never returns.
        follow: bool,
        /// How many recent lines to print (ignored if `path_only`).
        lines: usize,
        /// Print the log file's path instead of its content.
        path_only: bool,
    },
}

/// Parses CLI arguments (excluding the program name) into a [`Command`].
///
/// The first argument decides the shape: `serve`/`list` take no further
/// arguments; `categories`/`source` take a known subcommand; anything else
/// is treated as a show selector followed by `set <field> <value>` /
/// `show` / `rm`.
pub fn parse(args: &[String]) -> Result<Command, CliError> {
    let mut it = args.iter();
    let first = it
        .next()
        .ok_or_else(|| CliError::Usage("no command given".into()))?;

    match first.as_str() {
        "serve" => Ok(Command::Serve),
        "list" => Ok(Command::List),
        "categories" => parse_categories(it),
        "source" => parse_source(it),
        "logs" => parse_logs(it),
        selector => parse_show(selector, it),
    }
}

fn parse_logs<'a>(mut it: impl Iterator<Item = &'a String>) -> Result<Command, CliError> {
    let mut follow = false;
    let mut lines = 200usize;
    let mut path_only = false;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--follow" | "-f" => follow = true,
            "--path" => path_only = true,
            "--lines" | "-n" => {
                let value = it
                    .next()
                    .ok_or_else(|| CliError::Usage("logs --lines: expected a number".into()))?;
                lines = value.parse().map_err(|_| {
                    CliError::Usage(format!("logs --lines: invalid number {value:?}"))
                })?;
            }
            other => return Err(CliError::Usage(format!("logs: unknown flag {other:?}"))),
        }
    }
    Ok(Command::Logs {
        follow,
        lines,
        path_only,
    })
}

fn parse_categories<'a>(mut it: impl Iterator<Item = &'a String>) -> Result<Command, CliError> {
    let sub = it
        .next()
        .ok_or_else(|| CliError::Usage("categories: expected list|add|rm".into()))?;
    match sub.as_str() {
        "list" => Ok(Command::CategoriesList),
        "add" => {
            let name = it
                .next()
                .ok_or_else(|| CliError::Usage("categories add: expected <name>".into()))?
                .clone();
            let mut notify = false;
            let mut auto_download = false;
            for flag in it {
                match flag.as_str() {
                    "--notify" => notify = true,
                    "--auto-download" => auto_download = true,
                    other => {
                        return Err(CliError::Usage(format!(
                            "categories add: unknown flag {other:?}"
                        )))
                    }
                }
            }
            Ok(Command::CategoriesAdd {
                name,
                notify,
                auto_download,
            })
        }
        "rm" => {
            let name = it
                .next()
                .ok_or_else(|| CliError::Usage("categories rm: expected <name>".into()))?
                .clone();
            Ok(Command::CategoriesRm { name })
        }
        other => Err(CliError::Usage(format!(
            "categories: unknown subcommand {other:?}"
        ))),
    }
}

fn parse_source<'a>(mut it: impl Iterator<Item = &'a String>) -> Result<Command, CliError> {
    let sub = it
        .next()
        .ok_or_else(|| CliError::Usage("source: expected list|add|test".into()))?;
    match sub.as_str() {
        "list" => Ok(Command::SourceList),
        "add" => {
            let location = it
                .next()
                .ok_or_else(|| CliError::Usage("source add: expected <path-or-url>".into()))?
                .clone();
            Ok(Command::SourceAdd { location })
        }
        "test" => {
            let location = it
                .next()
                .ok_or_else(|| CliError::Usage("source test: expected <path-or-url>".into()))?
                .clone();
            Ok(Command::SourceTest { location })
        }
        other => Err(CliError::Usage(format!(
            "source: unknown subcommand {other:?}"
        ))),
    }
}

fn parse_show<'a>(
    selector: &str,
    mut it: impl Iterator<Item = &'a String>,
) -> Result<Command, CliError> {
    let verb = it
        .next()
        .ok_or_else(|| CliError::Usage(format!("{selector}: expected set|show|rm")))?;
    let action = match verb.as_str() {
        "show" => ShowAction::Show,
        "rm" => ShowAction::Rm,
        "set" => {
            let field = it
                .next()
                .ok_or_else(|| CliError::Usage("set: expected <field>".into()))?
                .clone();
            let value = it
                .next()
                .ok_or_else(|| CliError::Usage("set: expected <value>".into()))?
                .clone();
            ShowAction::Set { field, value }
        }
        other => {
            return Err(CliError::Usage(format!(
                "{selector}: unknown verb {other:?}"
            )))
        }
    };
    Ok(Command::Show {
        selector: selector.to_string(),
        action,
    })
}

/// Executes a parsed command, returning the text to print to stdout.
///
/// `config_path` is the file `categories add/rm` and `source add` rewrite;
/// it need not exist yet (it's created on first write).
pub async fn dispatch(
    command: Command,
    config: &Config,
    config_path: &Path,
    store: &Store,
) -> Result<String, CliError> {
    match command {
        Command::Serve => {
            Ok("the daemon isn't wired up yet — `serve` lands in a later milestone".to_string())
        }
        Command::List => {
            let rows = store.list_all().await?;
            Ok(table::format_series_table(&rows))
        }
        Command::CategoriesList => Ok(table::format_categories(&config.categories)),
        Command::CategoriesAdd {
            name,
            notify,
            auto_download,
        } => {
            config_write::add_category(config_path, config, &name, notify, auto_download)?;
            Ok(format!(
                "Added category {name:?} (notify={notify}, auto_download={auto_download})\n"
            ))
        }
        Command::CategoriesRm { name } => {
            config_write::remove_category(config_path, config, &name)?;
            Ok(format!("Removed category {name:?}\n"))
        }
        Command::SourceList => Ok(table::format_sources(&config.sources)),
        Command::SourceAdd { location } => {
            config_write::add_source(config_path, config, &location)?;
            Ok(format!("Added source {location:?}\n"))
        }
        Command::SourceTest { location } => {
            let client = anime_notif_fetch::client();
            let cache_dir = anime_notif_core::paths::default_cache_dir();
            let compiled =
                anime_notif_fetch::resolve_source(&client, &location, &cache_dir, None).await?;
            let result = anime_notif_fetch::poll(&client, &compiled).await?;
            Ok(table::format_extraction_result(&result))
        }
        Command::Show { selector, action } => dispatch_show(selector, action, config, store).await,
        Command::Logs {
            follow: _,
            lines,
            path_only,
        } => {
            let dir = anime_notif_core::paths::default_log_dir();
            if path_only {
                return Ok(format!("{}\n", dir.display()));
            }
            match logs::find_current_log_file(&dir) {
                Some(path) => logs::read_tail(&path, lines),
                None => Ok(format!(
                    "No log file found yet at {} — has `anime-notif serve` been run?\n",
                    dir.display()
                )),
            }
        }
    }
}

async fn dispatch_show(
    selector: String,
    action: ShowAction,
    config: &Config,
    store: &Store,
) -> Result<String, CliError> {
    let row = selector::resolve_selector(store, &selector).await?;
    match action {
        ShowAction::Show => {
            let history = store.list_interactions(row.id).await?;
            Ok(table::format_series_detail(&row, &history))
        }
        ShowAction::Rm => {
            store.delete_series(row.id).await?;
            Ok(format!("Removed {:?} (id {})\n", row.title, row.id))
        }
        ShowAction::Set { field, value } => {
            match field.as_str() {
                "category" => {
                    let resolved = selector::resolve_category(&value, &config.categories)?;
                    store.set_category(row.id, resolved).await?;
                }
                "alias" => {
                    store.set_alias(row.id, &value).await?;
                }
                other => return Err(CliError::UnknownField(other.to_string())),
            }
            let updated = store
                .get_by_id(row.id)
                .await?
                .ok_or_else(|| CliError::NotFound(selector.clone()))?;
            Ok(table::format_series_table(std::slice::from_ref(&updated)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_list_and_serve() {
        assert_eq!(parse(&args(&["list"])).unwrap(), Command::List);
        assert_eq!(parse(&args(&["serve"])).unwrap(), Command::Serve);
    }

    #[test]
    fn parses_selector_set_show_rm() {
        assert_eq!(
            parse(&args(&["one-piece", "set", "category", "liked"])).unwrap(),
            Command::Show {
                selector: "one-piece".into(),
                action: ShowAction::Set {
                    field: "category".into(),
                    value: "liked".into(),
                },
            }
        );
        assert_eq!(
            parse(&args(&["5", "show"])).unwrap(),
            Command::Show {
                selector: "5".into(),
                action: ShowAction::Show,
            }
        );
        assert_eq!(
            parse(&args(&["op", "rm"])).unwrap(),
            Command::Show {
                selector: "op".into(),
                action: ShowAction::Rm,
            }
        );
    }

    #[test]
    fn parses_categories_and_source_subcommands() {
        assert_eq!(
            parse(&args(&["categories", "list"])).unwrap(),
            Command::CategoriesList
        );
        assert_eq!(
            parse(&args(&["categories", "add", "watching", "--notify"])).unwrap(),
            Command::CategoriesAdd {
                name: "watching".into(),
                notify: true,
                auto_download: false,
            }
        );
        assert_eq!(
            parse(&args(&["source", "test", "sources/subsplease.toml"])).unwrap(),
            Command::SourceTest {
                location: "sources/subsplease.toml".into(),
            }
        );
    }

    #[test]
    fn parses_logs_command_and_flags() {
        assert_eq!(
            parse(&args(&["logs"])).unwrap(),
            Command::Logs {
                follow: false,
                lines: 200,
                path_only: false,
            }
        );
        assert_eq!(
            parse(&args(&["logs", "--follow", "--lines", "50"])).unwrap(),
            Command::Logs {
                follow: true,
                lines: 50,
                path_only: false,
            }
        );
        assert_eq!(
            parse(&args(&["logs", "-f", "-n", "10"])).unwrap(),
            Command::Logs {
                follow: true,
                lines: 10,
                path_only: false,
            }
        );
        assert_eq!(
            parse(&args(&["logs", "--path"])).unwrap(),
            Command::Logs {
                follow: false,
                lines: 200,
                path_only: true,
            }
        );
    }

    #[test]
    fn logs_rejects_bad_lines_value() {
        assert!(matches!(
            parse(&args(&["logs", "--lines", "not-a-number"])),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn missing_arguments_are_usage_errors() {
        assert!(matches!(parse(&args(&[])), Err(CliError::Usage(_))));
        assert!(matches!(
            parse(&args(&["one-piece", "set", "category"])),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            parse(&args(&["one-piece"])),
            Err(CliError::Usage(_))
        ));
    }

    #[tokio::test]
    async fn dispatch_list_set_show_rm_end_to_end() {
        let store = Store::open_in_memory().await.unwrap();
        store
            .upsert_series("subsplease", "One Piece", "normal", None)
            .await
            .unwrap();
        let config = Config::default();
        let config_path = Path::new("/nonexistent/config.toml");

        let out = dispatch(Command::List, &config, config_path, &store)
            .await
            .unwrap();
        assert!(out.contains("One Piece"));

        let out = dispatch(
            parse(&args(&["One Piece", "set", "category", "liked"])).unwrap(),
            &config,
            config_path,
            &store,
        )
        .await
        .unwrap();
        assert!(out.contains("liked"));

        let out = dispatch(
            parse(&args(&["One Piece", "show"])).unwrap(),
            &config,
            config_path,
            &store,
        )
        .await
        .unwrap();
        assert!(out.contains("History:"));
        assert!(out.contains("category"));

        let out = dispatch(
            parse(&args(&["One Piece", "rm"])).unwrap(),
            &config,
            config_path,
            &store,
        )
        .await
        .unwrap();
        assert!(out.contains("Removed"));
        assert!(store.list_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn dispatch_reports_unknown_field() {
        let store = Store::open_in_memory().await.unwrap();
        store
            .upsert_series("subsplease", "One Piece", "normal", None)
            .await
            .unwrap();
        let config = Config::default();
        let err = dispatch(
            parse(&args(&["One Piece", "set", "bogus", "x"])).unwrap(),
            &config,
            Path::new("/nonexistent/config.toml"),
            &store,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CliError::UnknownField(_)));
    }
}
