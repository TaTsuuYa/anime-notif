//! The `seen` dedup table: episode variants already notified/downloaded, so
//! a poll never re-announces the same release.

use chrono::Utc;

use crate::error::StoreError;
use crate::Store;

impl Store {
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
