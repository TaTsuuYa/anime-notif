//! SQLite/libSQL-backed storage for anime-notif's mutable, CLI-owned state:
//! tracked series (category, alias, interaction history), the dedup log of
//! already-notified episode variants, and episodes pending a desired
//! resolution.
//!
//! Backed by the `libsql` crate, which speaks both a local SQLite file and a
//! remote libSQL/Turso database through the same API — so "local vs cloud"
//! is a [`anime_notif_core::config::DbConfig`] choice, not a second code
//! path.

#![warn(missing_docs)]

mod error;
mod interactions;
mod migrations;
mod pending;
mod seen;
mod series;
mod types;

pub use error::StoreError;
pub use types::{InteractionKind, InteractionRow, PendingRow, SeriesRow};

use anime_notif_core::config::DbConfig;

/// A handle to the anime-notif database. Cheap to clone (wraps a pooled
/// [`libsql::Connection`]); construct one with [`Store::open`].
#[derive(Clone)]
pub struct Store {
    conn: libsql::Connection,
}

impl Store {
    /// Opens the database described by `db` (creating a local file and its
    /// parent directory if needed, or connecting to a remote libSQL/Turso
    /// instance), and applies any pending schema migrations.
    pub async fn open(db: &DbConfig) -> Result<Self, StoreError> {
        let database = match db {
            DbConfig::Local { path } => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                libsql::Builder::new_local(path).build().await?
            }
            DbConfig::Remote { url, auth_token } => {
                libsql::Builder::new_remote(url.clone(), auth_token.clone().unwrap_or_default())
                    .build()
                    .await?
            }
        };
        let conn = database.connect()?;
        migrations::run(&conn).await?;
        Ok(Self { conn })
    }

    /// Opens an in-memory database for tests: no file, no persistence.
    #[doc(hidden)]
    pub async fn open_in_memory() -> Result<Self, StoreError> {
        let database = libsql::Builder::new_local(":memory:").build().await?;
        let conn = database.connect()?;
        migrations::run(&conn).await?;
        Ok(Self { conn })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_apply_cleanly() {
        Store::open_in_memory().await.unwrap();
    }

    #[tokio::test]
    async fn upsert_series_is_idempotent_and_updates_cover() {
        let store = Store::open_in_memory().await.unwrap();
        let first = store
            .upsert_series("subsplease", "One Piece", "liked", None)
            .await
            .unwrap();
        assert_eq!(first.category, "liked");
        assert_eq!(first.cover_url, None);

        let second = store
            .upsert_series(
                "subsplease",
                "One Piece",
                "normal",
                Some("https://x/cover.jpg"),
            )
            .await
            .unwrap();
        // Same row (same id), category untouched by a repeat "creation".
        assert_eq!(second.id, first.id);
        assert_eq!(second.category, "liked");
        assert_eq!(second.cover_url.as_deref(), Some("https://x/cover.jpg"));

        assert_eq!(store.list_all().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn set_category_logs_interaction() {
        let store = Store::open_in_memory().await.unwrap();
        let series = store
            .upsert_series("subsplease", "One Piece", "normal", None)
            .await
            .unwrap();
        store.set_category(series.id, "liked").await.unwrap();

        let updated = store.get_by_id(series.id).await.unwrap().unwrap();
        assert_eq!(updated.category, "liked");
        assert!(updated.last_interaction_at.is_some());

        let history = store.list_interactions(series.id).await.unwrap();
        assert_eq!(history.len(), 2); // created + category change
        assert_eq!(history[0].kind, InteractionKind::Category);
        assert_eq!(history[0].detail.as_deref(), Some("liked"));
    }

    #[tokio::test]
    async fn set_alias_rejects_duplicates() {
        let store = Store::open_in_memory().await.unwrap();
        let a = store
            .upsert_series("subsplease", "One Piece", "normal", None)
            .await
            .unwrap();
        let b = store
            .upsert_series("subsplease", "Naruto", "normal", None)
            .await
            .unwrap();

        store.set_alias(a.id, "op").await.unwrap();
        let err = store.set_alias(b.id, "op").await.unwrap_err();
        assert!(matches!(err, StoreError::AliasTaken(_)));

        // Re-setting your own alias to the same value is fine (not a conflict).
        store.set_alias(a.id, "op").await.unwrap();
    }

    #[tokio::test]
    async fn find_by_title_reports_duplicates() {
        let store = Store::open_in_memory().await.unwrap();
        store
            .upsert_series("subsplease", "One Piece", "normal", None)
            .await
            .unwrap();
        store
            .upsert_series("nyaa", "One Piece", "normal", None)
            .await
            .unwrap();

        let matches = store.find_by_title("One Piece").await.unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[tokio::test]
    async fn seen_dedup_roundtrip() {
        let store = Store::open_in_memory().await.unwrap();
        let series = store
            .upsert_series("subsplease", "One Piece", "liked", None)
            .await
            .unwrap();

        assert!(!store.is_seen("key1").await.unwrap());
        store
            .mark_seen("key1", series.id, "1", "1080p", "magnet", "magnet:?xt=1")
            .await
            .unwrap();
        assert!(store.is_seen("key1").await.unwrap());

        // Re-marking is a no-op, not an error.
        store
            .mark_seen("key1", series.id, "1", "1080p", "magnet", "magnet:?xt=1")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn pending_preserves_first_seen_at_across_updates() {
        let store = Store::open_in_memory().await.unwrap();
        let series = store
            .upsert_series("subsplease", "One Piece", "liked", None)
            .await
            .unwrap();

        store
            .upsert_pending(
                series.id,
                "5",
                "1080p",
                Some("480p"),
                Some("{\"res\":\"480p\"}"),
            )
            .await
            .unwrap();
        let first = store.get_pending(series.id, "5").await.unwrap().unwrap();

        store
            .upsert_pending(
                series.id,
                "5",
                "1080p",
                Some("720p"),
                Some("{\"res\":\"720p\"}"),
            )
            .await
            .unwrap();
        let second = store.get_pending(series.id, "5").await.unwrap().unwrap();

        assert_eq!(first.first_seen_at, second.first_seen_at);
        assert_eq!(second.best_resolution.as_deref(), Some("720p"));

        store.clear_pending(series.id, "5").await.unwrap();
        assert!(store.get_pending(series.id, "5").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_series_cascades() {
        let store = Store::open_in_memory().await.unwrap();
        let series = store
            .upsert_series("subsplease", "One Piece", "liked", None)
            .await
            .unwrap();
        store
            .mark_seen("key1", series.id, "1", "1080p", "magnet", "magnet:?xt=1")
            .await
            .unwrap();
        store
            .upsert_pending(series.id, "2", "1080p", None, None)
            .await
            .unwrap();

        store.delete_series(series.id).await.unwrap();

        assert!(store.get_by_id(series.id).await.unwrap().is_none());
        assert!(!store.is_seen("key1").await.unwrap());
        assert!(store.get_pending(series.id, "2").await.unwrap().is_none());
    }
}
