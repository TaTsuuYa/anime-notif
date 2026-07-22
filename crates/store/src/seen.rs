//! The `seen` dedup table: episode variants already notified/downloaded, so
//! a poll never re-announces the same release.

use chrono::Utc;

use crate::error::StoreError;
use crate::types::SeenRow;
use crate::Store;

impl Store {
    /// Lists every recorded variant of `(series_id, episode)`, e.g. to let
    /// a manual "Download" action re-derive the favourite method/resolution
    /// available right now.
    pub async fn list_seen_for_episode(
        &self,
        series_id: i64,
        episode: &str,
    ) -> Result<Vec<SeenRow>, StoreError> {
        let mut rows = self
            .conn
            .query(
                "SELECT episode, resolution, method, link FROM seen WHERE series_id = ?1 AND episode = ?2",
                libsql::params![series_id, episode],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(SeenRow {
                episode: row.get(0)?,
                resolution: row.get(1)?,
                method: row.get(2)?,
                link: row.get(3)?,
            });
        }
        Ok(out)
    }

    /// Whether a given dedup key has already been recorded as seen.
    pub async fn is_seen(&self, dedup_key: &str) -> Result<bool, StoreError> {
        let mut rows = self
            .conn
            .query(
                "SELECT 1 FROM seen WHERE dedup_key = ?1",
                libsql::params![dedup_key],
            )
            .await?;
        Ok(rows.next().await?.is_some())
    }

    /// Records a release variant as seen. Idempotent: re-marking an
    /// already-seen key is a no-op.
    #[allow(clippy::too_many_arguments)]
    pub async fn mark_seen(
        &self,
        dedup_key: &str,
        series_id: i64,
        episode: &str,
        resolution: &str,
        method: &str,
        link: &str,
    ) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO seen (dedup_key, series_id, episode, resolution, method, link, first_seen_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                libsql::params![
                    dedup_key,
                    series_id,
                    episode,
                    resolution,
                    method,
                    link,
                    Utc::now().to_rfc3339()
                ],
            )
            .await?;
        Ok(())
    }
}
