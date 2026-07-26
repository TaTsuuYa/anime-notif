//! The daemon's loopback HTTP control server: the single place
//! download/whitelist/blacklist actions are executed, reached either by a
//! notification's link-fallback URL (opened in a browser) or by a native
//! notification action callback hitting the same URL (see
//! `anime-notif-notify`'s Linux backend). Bound to `127.0.0.1` only, and
//! every request must carry the token generated at daemon startup.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
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
    tracing::info!(
        kind = %params.kind,
        series_id = params.series_id,
        episode = %params.episode,
        "control server received an action request"
    );

    if params.token != engine.control_token {
        tracing::warn!(
            kind = %params.kind,
            series_id = params.series_id,
            "action rejected: token mismatch (does this notification predate the daemon's current run? \
             tokens used to be regenerated on every restart, invalidating old notifications' buttons \
             silently — the token is now persisted across restarts specifically to avoid this, so seeing \
             this after upgrading past that fix likely means the persisted token file became unreadable \
             or was deleted)"
        );
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

fn generate_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

fn token_path(state_dir: &Path) -> PathBuf {
    state_dir.join("control_token")
}

/// Loads the control-server token from `<state_dir>/control_token`,
/// generating and persisting a new one if it doesn't exist yet (or can't
/// be read).
///
/// The token exists to stop another local user/process from triggering
/// actions, not to protect against anything beyond the local machine (the
/// server only ever binds to `127.0.0.1`) — so persisting it to disk
/// doesn't weaken what it actually protects against, and fixes a real bug:
/// a token regenerated on every restart silently invalidates every
/// notification action button shown before that restart (they carry the
/// old token in their URL), which looked exactly like "clicking the button
/// does nothing" with no error visible anywhere.
pub fn load_or_generate_token(state_dir: &Path) -> String {
    let path = token_path(state_dir);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            tracing::debug!(path = %path.display(), "loaded persisted control token");
            return trimmed.to_string();
        }
    }

    let token = generate_token();
    match std::fs::create_dir_all(state_dir).and_then(|()| write_token_file(&path, &token)) {
        Ok(()) => {
            tracing::debug!(path = %path.display(), "generated and persisted a new control token")
        }
        Err(err) => tracing::warn!(
            path = %path.display(),
            %err,
            "failed to persist control token; it will be regenerated on the next restart, \
             invalidating any notifications shown before then"
        ),
    }
    token
}

#[cfg(unix)]
fn write_token_file(path: &Path, token: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, token)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn write_token_file(path: &Path, token: &str) -> std::io::Result<()> {
    std::fs::write(path, token)
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

    #[test]
    fn token_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let first = load_or_generate_token(dir.path());
        let second = load_or_generate_token(dir.path());
        assert_eq!(
            first, second,
            "a fresh process pointed at the same state dir must reuse the same token, \
             or every notification shown before a restart silently stops working"
        );
    }

    #[test]
    fn different_state_dirs_get_different_tokens() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let a = load_or_generate_token(dir_a.path());
        let b = load_or_generate_token(dir_b.path());
        assert_ne!(a, b);
    }

    #[test]
    fn missing_or_empty_token_file_is_regenerated() {
        let dir = tempfile::tempdir().unwrap();
        let path = token_path(dir.path());
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(&path, "").unwrap();
        let token = load_or_generate_token(dir.path());
        assert_eq!(token.len(), 32);
    }
}
