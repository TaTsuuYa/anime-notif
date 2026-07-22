//! Error types for database access.

/// Errors that can occur opening the database, running migrations, or
/// executing queries.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The underlying libSQL/SQLite driver returned an error.
    #[error("database error: {0}")]
    Db(#[from] libsql::Error),

    /// A write violated the alias-uniqueness constraint.
    #[error("alias {0:?} is already in use by another show")]
    AliasTaken(String),

    /// A lookup by id/alias/title found no matching series.
    #[error("no show found matching {0:?}")]
    NotFound(String),

    /// A row was malformed in a way that couldn't be decoded.
    #[error("corrupt row: {0}")]
    Decode(String),

    /// Failed to create the parent directory for a local database file.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
