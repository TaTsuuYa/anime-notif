//! Error types shared by the config loader and other `core` utilities.

use std::path::PathBuf;

/// Errors that can occur while loading or validating a [`crate::config::Config`].
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The config file could not be read from disk.
    #[error("failed to read config file {path}: {source}")]
    Read {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The config file's contents were not valid TOML, or didn't match the
    /// expected schema.
    #[error("failed to parse config file {path}: {source}")]
    Parse {
        /// Path that failed to parse.
        path: PathBuf,
        /// Underlying TOML deserialization error.
        #[source]
        source: Box<toml::de::Error>,
    },

    /// The config parsed but failed a semantic check (e.g. duplicate
    /// category names, a rule referencing an undefined category).
    #[error("invalid config: {0}")]
    Invalid(String),
}
