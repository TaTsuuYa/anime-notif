//! HTTP client glue between [`anime_notif_core`]'s source-plugin schema and
//! real network APIs: resolving a plugin's location (local file or shared
//! URL) into a [`anime_notif_core::CompiledSource`], and polling its
//! endpoint to produce extracted releases.

#![warn(missing_docs)]

mod error;
mod loader;
mod poll;

pub use error::FetchError;
pub use loader::{resolve_source, sha256_hex};
pub use poll::poll;

/// Builds the shared HTTP client used for both plugin-file fetches and
/// source polling.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("anime-notif/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("client config is static and valid")
}
