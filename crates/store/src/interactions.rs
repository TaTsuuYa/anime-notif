//! The `interactions` audit log: every category/alias change, download, and
//! initial creation for a series.

use chrono::Utc;

use crate::error::StoreError;
use crate::types::{InteractionKind, InteractionRow};
use crate::Store;

impl Store {
    /// Appends an interaction record for `series_id`.
    pub async fn log_interaction(
        &self,
        series_id: i64,
        kind: InteractionKind,
        detail: Option<&str>,
    ) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO interactions (series_id, kind, detail, at) VALUES (?1, ?2, ?3, ?4)",
                libsql::params![series_id, kind.as_str(), detail, Utc::now().to_rfc3339()],
            )
            .await?;
        Ok(())
    }

    /// Lists a series' interaction history, most recent first.
    pub async fn list_interactions(
        &self,
        series_id: i64,
    ) -> Result<Vec<InteractionRow>, StoreError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, series_id, kind, detail, at FROM interactions WHERE series_id = ?1 ORDER BY at DESC, id DESC",
                libsql::params![series_id],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let kind_raw: String = row.get(2)?;
            let at_raw: String = row.get(4)?;
            out.push(InteractionRow {
                id: row.get(0)?,
                series_id: row.get(1)?,
                kind: InteractionKind::parse(&kind_raw).ok_or_else(|| {
                    StoreError::Decode(format!("unknown interaction kind {kind_raw:?}"))
                })?,
                detail: row.get(3)?,
                at: chrono::DateTime::parse_from_rfc3339(&at_raw)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Decode(format!("bad timestamp {at_raw:?}: {e}")))?,
            });
        }
        Ok(out)
    }
}
