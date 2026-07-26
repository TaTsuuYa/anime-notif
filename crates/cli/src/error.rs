//! Error types for CLI argument parsing and command execution.

/// Errors that can occur parsing CLI arguments or executing a command.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Arguments didn't match any known command shape.
    #[error("usage error: {0}")]
    Usage(String),

    /// No show matches the given id/alias/name.
    #[error("no show found matching {0:?}")]
    NotFound(String),

    /// A name selector matched more than one show; nothing was changed.
    #[error("{0:?} matches {1} shows with that name; use the numeric id or an alias instead")]
    AmbiguousName(String, usize),

    /// `set <field>` was given a field other than `category`/`alias`.
    #[error("unknown field {0:?}; expected \"category\" or \"alias\"")]
    UnknownField(String),

    /// `set category <value>` didn't match any defined category.
    #[error("unknown category {0:?}")]
    UnknownCategory(String),

    /// `set category <value>` matched more than one category by prefix.
    #[error("{0:?} matches multiple categories ({1}); type more of the name to disambiguate")]
    AmbiguousCategory(String, String),

    /// A database operation failed.
    #[error(transparent)]
    Store(#[from] anime_notif_store::StoreError),

    /// Loading/compiling a source plugin, or polling its endpoint, failed.
    #[error(transparent)]
    Fetch(#[from] anime_notif_fetch::FetchError),

    /// Reading or writing `config.toml` failed.
    #[error("failed to read/write config file {path}: {source}")]
    ConfigIo {
        /// Path that failed.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Serializing the config back to TOML (for `categories add/rm`,
    /// `source add`) failed.
    #[error("failed to serialize config: {0}")]
    ConfigSerialize(#[from] toml::ser::Error),

    /// Reading or following the log file failed.
    #[error("failed to read log file {path}: {source}")]
    LogIo {
        /// Path that failed.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}
