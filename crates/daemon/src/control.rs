//! The daemon's loopback HTTP control server: the single place
//! download/whitelist/blacklist actions are executed, reached either by a
//! notification's link-fallback URL (opened in a browser) or by a native
//! notification action callback hitting the same URL (see
//! `anime-notif-notify`'s Linux backend). Bound to `127.0.0.1` only, and
//! every request must carry the token generated at daemon startup.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde::Deserialize;
use tokio::net::TcpListener;

use crate::engine::Engine;

#[derive(Debug, Deserialize)]
struct ActionParams {
    token: String,
    kind: String,
    series_id: i64,
    episode: String,
    /// Only meaningful for `kind = "open_show"`: the show page to open.
    #[serde(default)]
    url: Option<String>,
}

async fn action_handler(
    State(engine): State<Arc<Engine>>,
    Query(params): Query<ActionParams>,
) -> Html<String> {
    if params.token != engine.control_token {
        return Html("<h1>Forbidden</h1><p>Invalid or missing token.</p>".to_string());
    }
    match engine
        .handle_action(
            &params.kind,
            params.series_id,
            &params.episode,
            params.url.as_deref(),
        )
        .await
    {
        Ok(message) => Html(format!("<h1>anime-notif</h1><p>{message}</p>")),
        Err(err) => Html(format!("<h1>anime-notif</h1><p>Error: {err}</p>")),
    }
}

async fn health_handler() -> &'static str {
    "ok"
}

/// Generates a random per-run token required on every control-server
/// request, so another local user/process can't trigger actions.
pub fn generate_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

/// Binds the control server's listening socket to `127.0.0.1:0` (an
/// OS-assigned ephemeral port, unless `preferred_port` is set) and returns
/// the bound address.
///
/// Split from [`serve`] because the bound address (specifically its port)
/// is needed to build the [`Engine`] (for its `control_base_url`, used in
/// notification action URLs) — but `serve` needs the already-built
/// `Engine` as axum state. Binding first breaks that ordering cycle.
pub async fn bind_listener(
    preferred_port: Option<u16>,
) -> std::io::Result<(SocketAddr, TcpListener)> {
    let addr = SocketAddr::from(([127, 0, 0, 1], preferred_port.unwrap_or(0)));
    let listener = TcpListener::bind(addr).await?;
    let bound_addr = listener.local_addr()?;
    Ok((bound_addr, listener))
}

/// Serves the control server on an already-bound `listener` until the
/// process exits or the server errors. Intended to be `tokio::spawn`ed.
pub async fn serve(engine: Arc<Engine>, listener: TcpListener) {
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/action", get(action_handler))
        .with_state(engine);

    if let Err(err) = axum::serve(listener, app).await {
        tracing::error!(%err, "control server exited unexpectedly");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_reasonably_unique_and_sized() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
