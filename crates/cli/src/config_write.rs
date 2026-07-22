//! Mutates `config.toml` on disk for the handful of CLI commands that
//! change declarative config rather than database state (`categories
//! add/rm`, `source add`).
//!
//! This rewrites the whole file via `toml::to_string_pretty`, so it does
//! **not** preserve comments or manual formatting — documented in
//! `docs/cli.md`. A config file that isn't writable (e.g. a Nix store path
//! symlinked in by a NixOS/home-manager module) fails here with a clear
//! I/O error; the fix in that case is to edit the Nix configuration
//! instead of using these commands.

use std::path::Path;

use anime_notif_core::config::CategoryDef;
use anime_notif_core::Config;

use crate::error::CliError;

fn write(path: &Path, config: &Config) -> Result<(), CliError> {
    let serialized = toml::to_string_pretty(config)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| CliError::ConfigIo {
            path: path.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(path, serialized).map_err(|source| CliError::ConfigIo {
        path: path.to_path_buf(),
        source,
    })
}

/// Adds a new category definition and rewrites the config file.
pub fn add_category(
    path: &Path,
    current: &Config,
    name: &str,
    notify: bool,
    auto_download: bool,
) -> Result<(), CliError> {
    let mut config = current.clone();
    if config.categories.iter().any(|c| c.name == name) {
        return Err(CliError::Usage(format!("category {name:?} already exists")));
    }
    config.categories.push(CategoryDef {
        name: name.to_string(),
        notify,
        auto_download,
    });
    write(path, &config)
}

/// Removes a category definition and rewrites the config file. Refuses to
/// remove an unknown category, or the last remaining one (every show needs
/// a valid category to belong to).
pub fn remove_category(path: &Path, current: &Config, name: &str) -> Result<(), CliError> {
    let mut config = current.clone();
    let before = config.categories.len();
    config.categories.retain(|c| c.name != name);
    if config.categories.len() == before {
        return Err(CliError::Usage(format!("no such category {name:?}")));
    }
    if config.categories.is_empty() {
        return Err(CliError::Usage(
            "cannot remove the last remaining category".into(),
        ));
    }
    write(path, &config)
}

/// Adds a source plugin location (path or URL) and rewrites the config
/// file.
pub fn add_source(path: &Path, current: &Config, location: &str) -> Result<(), CliError> {
    let mut config = current.clone();
    if config.sources.iter().any(|s| s == location) {
        return Err(CliError::Usage(format!(
            "source {location:?} is already configured"
        )));
    }
    config.sources.push(location.to_string());
    write(path, &config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anime_notif_core::Config;

    #[test]
    fn add_and_remove_category_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config::default();

        add_category(&path, &config, "watching", true, false).unwrap();
        let reloaded = Config::load(&path).unwrap();
        assert!(reloaded.categories.iter().any(|c| c.name == "watching"));

        remove_category(&path, &reloaded, "watching").unwrap();
        let reloaded = Config::load(&path).unwrap();
        assert!(!reloaded.categories.iter().any(|c| c.name == "watching"));
    }

    #[test]
    fn rejects_duplicate_category() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config::default();
        let err = add_category(&path, &config, "liked", true, true).unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }

    #[test]
    fn refuses_to_remove_last_category() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut config = Config::default();
        config.categories.truncate(1);
        let only = config.categories[0].name.clone();
        let err = remove_category(&path, &config, &only).unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }

    #[test]
    fn add_source_round_trips_and_rejects_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config::default();

        add_source(&path, &config, "sources/subsplease.toml").unwrap();
        let reloaded = Config::load(&path).unwrap();
        assert_eq!(reloaded.sources, vec!["sources/subsplease.toml"]);

        let err = add_source(&path, &reloaded, "sources/subsplease.toml").unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }
}
