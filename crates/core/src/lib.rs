//! Shared domain types and configuration for anime-notif: the normalized
//! [`model::Release`] type, the declarative [`config::Config`] schema, and
//! platform-default paths. Other crates in the workspace depend on this one
//! rather than redefining these types.

#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod model;
pub mod paths;

pub use config::Config;
pub use error::ConfigError;
pub use model::{DownloadMethod, Release};
