//! `anime-notif` binary entry point: resolves the config file, opens the
//! database, parses CLI arguments, and dispatches to `anime-notif-cli`.
//! `serve` (the background daemon) is not wired up yet — it lands with
//! `anime-notif-daemon` in a later milestone.

use std::path::{Path, PathBuf};

use anime_notif_core::{Config, ConfigError};
use anime_notif_store::Store;

const USAGE: &str = "\
Usage:
  anime-notif serve
  anime-notif list
  anime-notif <id|alias|name> set category|alias <value>
  anime-notif <id|alias|name> show
  anime-notif <id|alias|name> rm
  anime-notif categories list
  anime-notif categories add <name> [--notify] [--auto-download]
  anime-notif categories rm <name>
  anime-notif source list
  anime-notif source add <path-or-url>
  anime-notif source test <path-or-url>

Config file: $ANIME_NOTIF_CONFIG, or the platform default config directory.\
";

/// Resolves the config file path: `$ANIME_NOTIF_CONFIG` if set, otherwise
/// the platform-default location (see `anime_notif_core::paths`).
fn config_path() -> PathBuf {
    std::env::var_os("ANIME_NOTIF_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(anime_notif_core::paths::default_config_path)
}

/// Loads the config file, falling back to defaults when it doesn't exist
/// yet (a bare, non-Nix install has no config until the user creates one or
/// runs a command that writes it, e.g. `categories add`/`source add`).
/// A config file that exists but fails to parse/validate is a hard error.
fn load_config(path: &Path) -> Config {
    match Config::load(path) {
        Ok(config) => config,
        Err(ConfigError::Read { .. }) => Config::default(),
        Err(err) => {
            eprintln!("error: invalid config at {}: {err}", path.display());
            std::process::exit(1);
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let command = match anime_notif_cli::parse(&args) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("{err}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    let path = config_path();
    let config = load_config(&path);

    let store = match Store::open(&config.general.db).await {
        Ok(store) => store,
        Err(err) => {
            eprintln!("error: failed to open database: {err}");
            std::process::exit(1);
        }
    };

    match anime_notif_cli::dispatch(command, &config, &path, &store).await {
        Ok(output) => print!("{output}"),
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}
