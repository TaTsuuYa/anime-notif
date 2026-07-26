//! `anime-notif` binary entry point: resolves the config file, opens the
//! database, parses CLI arguments, and dispatches to `anime-notif-cli`.
//! `serve` runs the background daemon directly (`anime-notif-daemon`)
//! instead of going through `dispatch`, since nothing else needs the
//! store/config wiring it does; `logs --follow` is likewise handled here
//! directly since it never returns a single string to print.

use std::path::{Path, PathBuf};

use anime_notif_core::{Config, ConfigError};
use anime_notif_store::Store;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

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
  anime-notif logs [--follow|-f] [--lines|-n N] [--path]

Config file: $ANIME_NOTIF_CONFIG, or the platform default config directory.
Logs: RUST_LOG controls verbosity (e.g. RUST_LOG=debug anime-notif serve).\
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

/// Initializes logging for `serve`: the existing stdout/stderr layer (so
/// systemd/journald keeps capturing it exactly as before) plus a
/// daily-rotating file under `anime_notif_core::paths::default_log_dir()`,
/// which `anime-notif logs` reads — this works the same on every
/// platform/init system, unlike `journalctl`. Returns the file appender's
/// guard, which must stay alive for the rest of the process (it owns the
/// background thread that flushes buffered writes to disk).
fn init_logging() -> tracing_appender::non_blocking::WorkerGuard {
    let log_dir = anime_notif_core::paths::default_log_dir();
    if let Err(err) = std::fs::create_dir_all(&log_dir) {
        eprintln!(
            "warning: failed to create log directory {}: {err} (file logging disabled, stdout logging still works)",
            log_dir.display()
        );
    }
    let file_appender =
        tracing_appender::rolling::daily(&log_dir, anime_notif_cli::logs::FILE_PREFIX);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
        .init();

    guard
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

    if let anime_notif_cli::Command::Logs {
        follow: true,
        lines,
        path_only,
    } = &command
    {
        let dir = anime_notif_core::paths::default_log_dir();
        if *path_only {
            println!("{}", dir.display());
            return;
        }
        let Some(path) = anime_notif_cli::logs::find_current_log_file(&dir) else {
            println!(
                "No log file found yet at {} — has `anime-notif serve` been run?",
                dir.display()
            );
            return;
        };
        if let Ok(tail) = anime_notif_cli::logs::read_tail(&path, *lines) {
            print!("{tail}");
        }
        if let Err(err) = anime_notif_cli::logs::follow(&path, |line| println!("{line}")) {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
        return;
    }

    let path = config_path();
    let config = load_config(&path);

    if matches!(command, anime_notif_cli::Command::Serve) {
        let _guard = init_logging();
        if let Err(err) = anime_notif_daemon::run(config).await {
            eprintln!("error: failed to start daemon: {err}");
            std::process::exit(1);
        }
        return;
    }

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
