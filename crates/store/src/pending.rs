//! The `pending` table: episodes seen without their desired resolution yet,
//! awaiting either the desired resolution or a fallback timeout (see the
//! resolution-wait workflow documented in `docs/downloads.md`).

use chrono::{DateTime, Utc};
use libsql::Row;

use crate::error::StoreError;
use crate::types::PendingRow;
use crate::Store;

fn decode_row(row: Row) -> Result<PendingRow, StoreError> {
    let first_seen_raw: String = row.get(2)?;
    Ok(PendingRow {
        series_id: row.get(0)?,
        episode: row.get(1)?,
        first_seen_at: DateTime::parse_from_rfc3339(&first_seen_raw)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| StoreError::Decode(format!("bad timestamp {first_seen_raw:?}: {e}")))?,
        desired_resolution: row.get(3)?,
        best_resolution: row.get(4)?,
        best_variant_json: row.get(5)?,
    })
}

const SELECT_COLUMNS: &str =
    "series_id, episode, first_seen_at, desired_resolution, best_resolution, best_variant_json";

impl Store {
    /// Looks up a single pending entry for `(series_id, episode)`.
    pub async fn get_pending(
        &self,
        series_id: i64,
        episode: &str,
    ) -> Result<Option<PendingRow>, StoreError> {
        let mut rows = self
            .conn
            .query(
                &format!(
                    "SELECT {SELECT_COLUMNS} FROM pending WHERE series_id = ?1 AND episode = ?2"
                ),
                libsql::params![series_id, episode],
            )
            .await?;
        rows.next().await?.map(decode_row).transpose()
    }

    /// Lists every pending entry, across all series, oldest first (the
    /// order the scheduler re-evaluates them in).
    pub async fn list_pending(&self) -> Result<Vec<PendingRow>, StoreError> {
        let mut rows = self
            .conn
            .query(
                &format!("SELECT {SELECT_COLUMNS} FROM pending ORDER BY first_seen_at ASC"),
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(decode_row(row)?);
        }
        Ok(out)
    }

    /// Inserts a new pending entry, or, if one already exists for
    /// `(series_id, episode)`, updates its best-so-far variant while
    /// preserving the original `first_seen_at` (that timestamp is what the
    /// `resolution_wait` timeout is measured against).
    pub async fn upsert_pending(
        &self,
        series_id: i64,
        episode: &str,
        desired_resolution: &str,
        best_resolution: Option<&str>,
        best_variant_json: Option<&str>,
    ) -> Result<(), StoreError> {
        if let Some(existing) = self.get_pending(series_id, episode).await? {
            let best_resolution = best_resolution.or(existing.best_resolution.as_deref());
            let best_variant_json = best_variant_json.or(existing.best_variant_json.as_deref());
            self.conn
                .execute(
                    "UPDATE pending SET best_resolution = ?1, best_variant_json = ?2 WHERE series_id = ?3 AND episode = ?4",
                    libsql::params![best_resolution, best_variant_json, series_id, episode],
                )
                .await?;
        } else {
            self.conn
                .execute(
                    "INSERT INTO pending (series_id, episode, first_seen_at, desired_resolution, best_resolution, best_variant_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    libsql::params![
                        series_id,
                        episode,
                        Utc::now().to_rfc3339(),
                        desired_resolution,
                        best_resolution,
                        best_variant_json
                    ],
                )
                .await?;
        }
        Ok(())
    }

    /// Removes a pending entry once it has been resolved (notified/
    /// downloaded, at the desired resolution or a fallback).
    pub async fn clear_pending(&self, series_id: i64, episode: &str) -> Result<(), StoreError> {
        self.conn
            .execute(
                "DELETE FROM pending WHERE series_id = ?1 AND episode = ?2",
                libsql::params![series_id, episode],
            )
            .await?;
        Ok(())
    }
}
