//! Platform-appropriate default locations (XDG on Linux, `%APPDATA%` on
//! Windows, `~/Library` on macOS) via the `directories` crate.

use std::path::PathBuf;

use directories::ProjectDirs;

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("dev", "anime-notif", "anime-notif")
}

/// Default config directory, e.g. `~/.config/anime-notif` on Linux.
pub fn default_config_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Default path to the main `config.toml`.
pub fn default_config_path() -> PathBuf {
    default_config_dir().join("config.toml")
}

/// Default state/data directory, e.g. `~/.local/share/anime-notif` on Linux.
pub fn default_data_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Default path to the local SQLite database file.
pub fn default_db_path() -> PathBuf {
    default_data_dir().join("data.db")
}

/// Default cache directory, used for fetched cover images and remote source
/// plugin caches.
pub fn default_cache_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Default base download directory, e.g. `~/Downloads/anime-notif`.
pub fn default_download_dir() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|u| u.download_dir().map(|p| p.join("anime-notif")))
        .unwrap_or_else(|| PathBuf::from("anime-notif-downloads"))
}
