//! Spawns one polling task per configured source, on its own interval
//! (falling back to `general.default_interval` when the source doesn't
//! set one).

use std::sync::Arc;
use std::time::Duration;

use anime_notif_core::CompiledSource;

use crate::engine::Engine;

/// Spawns a task that polls `source` forever, on its configured interval
/// (or `default_interval`), feeding results into `engine`.
pub fn spawn_source_task(
    engine: Arc<Engine>,
    client: reqwest::Client,
    source: CompiledSource,
    default_interval: Duration,
) -> tokio::task::JoinHandle<()> {
    let interval = source.interval.unwrap_or(default_interval);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            poll_once(&engine, &client, &source).await;
        }
    })
}

async fn poll_once(engine: &Engine, client: &reqwest::Client, source: &CompiledSource) {
    match anime_notif_fetch::poll(client, source).await {
        Ok(result) => {
            for warning in &result.warnings {
                tracing::warn!(source = %source.id, %warning, "extraction warning");
            }
            if let Err(err) = engine.process_poll(result.releases).await {
                tracing::error!(source = %source.id, %err, "failed to process poll results");
            }
        }
        Err(err) => {
            tracing::error!(source = %source.id, %err, "failed to poll source");
        }
    }
}
