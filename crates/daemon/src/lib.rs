//! The anime-notif background daemon: polls every configured source on its
//! own schedule, runs the resolution-wait workflow to decide when/what to
//! notify or auto-download, and serves the loopback control server that
//! handles notification actions.
//!
//! See [`engine`] for the resolution-decision workflow itself,
//! [`resolution`] for its pure decision logic, [`scheduler`] for the
//! per-source polling loop, and [`control`] for the action-handling HTTP
//! server.

#![warn(missing_docs)]

pub mod control;
pub mod engine;
pub mod error;
pub mod resolution;
pub mod scheduler;

pub use engine::Engine;
pub use error::{DaemonError, EngineError};

use std::collections::HashMap;
use std::sync::Arc;

use anime_notif_core::Config;
use anime_notif_store::Store;

/// Loads every source in `config.sources`, starts the control server and
/// one polling task per successfully-loaded source, and runs until the
/// process receives a shutdown signal (Ctrl-C).
///
/// A source that fails to load (bad plugin file, unreachable URL) is
/// logged and skipped rather than aborting the whole daemon — one broken
/// source shouldn't take down every other one.
pub async fn run(config: Config) -> Result<(), DaemonError> {
    let store = Store::open(&config.general.db).await?;
    let client = anime_notif_fetch::client();
    let cache_dir = anime_notif_core::paths::default_cache_dir();

    let mut compiled_sources = Vec::new();
    let mut resolution_wait_by_source = HashMap::new();
    for location in &config.sources {
        match anime_notif_fetch::resolve_source(&client, location, &cache_dir, None).await {
            Ok(source) => {
                if let Some(wait) = source.resolution_wait {
                    resolution_wait_by_source.insert(source.id.clone(), wait);
                }
                compiled_sources.push(source);
            }
            Err(err) => {
                tracing::error!(location, %err, "failed to load source plugin; skipping");
            }
        }
    }
    if compiled_sources.is_empty() {
        tracing::warn!("no sources loaded; the daemon will idle with nothing to poll");
    }

    let (control_addr, listener) = control::bind_listener(None).await?;
    tracing::info!(%control_addr, "control server listening");

    let engine = Arc::new(Engine {
        store: Arc::new(store),
        config: Arc::new(config.clone()),
        notifier: Arc::from(anime_notif_notify::default_notifier()),
        downloader: Arc::new(anime_notif_download::StdDownloader::new()),
        control_base_url: format!("http://{control_addr}"),
        control_token: control::generate_token(),
        resolution_wait_by_source,
    });

    tokio::spawn(control::serve(engine.clone(), listener));

    let mut handles = Vec::new();
    for source in compiled_sources {
        tracing::info!(source = %source.id, "starting poller");
        handles.push(scheduler::spawn_source_task(
            engine.clone(),
            client.clone(),
            source,
            config.general.default_interval,
        ));
    }

    tokio::signal::ctrl_c().await.ok();
    tracing::info!("shutting down");
    for handle in handles {
        handle.abort();
    }
    Ok(())
}
