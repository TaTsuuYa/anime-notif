//! CRUD operations on the `series` table: the mutable, CLI-owned half of a
//! show's state (category, alias, last episode/interaction).

use chrono::{DateTime, Utc};
use libsql::Row;

use crate::error::StoreError;
use crate::types::{InteractionKind, SeriesRow};
use crate::Store;

const SELECT_COLUMNS: &str =
    "id, source_id, title, alias, category, last_episode, last_interaction_at, cover_url";

fn decode_row(row: Row) -> Result<SeriesRow, StoreError> {
    Ok(SeriesRow {
        id: row.get(0)?,
        source_id: row.get(1)?,
        title: row.get(2)?,
        alias: row.get(3)?,
        category: row.get(4)?,
        last_episode: row.get(5)?,
        last_interaction_at: parse_ts(row.get(6)?)?,
        cover_url: row.get(7)?,
    })
}

fn parse_ts(raw: Option<String>) -> Result<Option<DateTime<Utc>>, StoreError> {
    raw.map(|s| {
        DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| StoreError::Decode(format!("bad timestamp {s:?}: {e}")))
    })
    .transpose()
}

impl Store {
    /// Inserts a new series, or, if `(source_id, title)` already exists,
    /// updates its cached `cover_url` (when a non-null one is provided)
    /// without touching its category/alias/interaction history. Returns the
    /// resulting row.
    pub async fn upsert_series(
        &self,
        source_id: &str,
        title: &str,
        default_category: &str,
        cover_url: Option<&str>,
    ) -> Result<SeriesRow, StoreError> {
        let existing = self.get_by_source_title(source_id, title).await?;
        if let Some(existing) = existing {
            if let Some(cover) = cover_url {
                self.conn
                    .execute(
                        "UPDATE series SET cover_url = ?1 WHERE id = ?2",
                        libsql::params![cover, existing.id],
                    )
                    .await?;
            }
            return self
                .get_by_id(existing.id)
                .await?
                .ok_or_else(|| StoreError::NotFound(title.to_string()));
        }

        self.conn
            .execute(
                "INSERT INTO series (source_id, title, category, cover_url) VALUES (?1, ?2, ?3, ?4)",
                libsql::params![source_id, title, default_category, cover_url],
            )
            .await?;
        let row = self
            .get_by_source_title(source_id, title)
            .await?
            .ok_or_else(|| StoreError::NotFound(title.to_string()))?;
        self.log_interaction(row.id, InteractionKind::Created, Some(default_category))
            .await?;
        Ok(row)
    }

    /// Looks up a series by its auto-generated id.
    pub async fn get_by_id(&self, id: i64) -> Result<Option<SeriesRow>, StoreError> {
        let mut rows = self
            .conn
            .query(
                &format!("SELECT {SELECT_COLUMNS} FROM series WHERE id = ?1"),
                libsql::params![id],
            )
            .await?;
        rows.next().await?.map(decode_row).transpose()
    }

    /// Looks up a series by its unique user-assigned alias.
    pub async fn get_by_alias(&self, alias: &str) -> Result<Option<SeriesRow>, StoreError> {
        let mut rows = self
            .conn
            .query(
                &format!("SELECT {SELECT_COLUMNS} FROM series WHERE alias = ?1"),
                libsql::params![alias],
            )
            .await?;
        rows.next().await?.map(decode_row).transpose()
    }

    /// Looks up a series by exact `(source_id, title)`.
    pub async fn get_by_source_title(
        &self,
        source_id: &str,
        title: &str,
    ) -> Result<Option<SeriesRow>, StoreError> {
        let mut rows = self
            .conn
            .query(
                &format!("SELECT {SELECT_COLUMNS} FROM series WHERE source_id = ?1 AND title = ?2"),
                libsql::params![source_id, title],
            )
            .await?;
        rows.next().await?.map(decode_row).transpose()
    }

    /// Finds all series with an exact title match, across sources. Used by
    /// the CLI's name-based selector to detect ambiguous names.
    pub async fn find_by_title(&self, title: &str) -> Result<Vec<SeriesRow>, StoreError> {
        let mut rows = self
            .conn
            .query(
                &format!("SELECT {SELECT_COLUMNS} FROM series WHERE title = ?1 ORDER BY id"),
                libsql::params![title],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(decode_row(row)?);
        }
        Ok(out)
    }

    /// Lists every tracked series, ordered by id.
    pub async fn list_all(&self) -> Result<Vec<SeriesRow>, StoreError> {
        let mut rows = self
            .conn
            .query(
                &format!("SELECT {SELECT_COLUMNS} FROM series ORDER BY id"),
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(decode_row(row)?);
        }
        Ok(out)
    }

    /// Changes a series' category, logging the change as an interaction.
    pub async fn set_category(&self, id: i64, category: &str) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE series SET category = ?1, last_interaction_at = ?2 WHERE id = ?3",
                libsql::params![category, now, id],
            )
            .await?;
        self.log_interaction(id, InteractionKind::Category, Some(category))
            .await?;
        Ok(())
    }

    /// Changes a series' alias, checked for uniqueness first (returns
    /// [`StoreError::AliasTaken`] rather than relying on the DB constraint
    /// error, whose shape isn't stable across drivers).
    pub async fn set_alias(&self, id: i64, alias: &str) -> Result<(), StoreError> {
        if let Some(existing) = self.get_by_alias(alias).await? {
            if existing.id != id {
                return Err(StoreError::AliasTaken(alias.to_string()));
            }
        }
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE series SET alias = ?1, last_interaction_at = ?2 WHERE id = ?3",
                libsql::params![alias, now, id],
            )
            .await?;
        self.log_interaction(id, InteractionKind::Alias, Some(alias))
            .await?;
        Ok(())
    }

    /// Records that `episode` was the most recent one interacted with
    /// (downloaded/notified) for this series.
    pub async fn set_last_episode(&self, id: i64, episode: &str) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE series SET last_episode = ?1, last_interaction_at = ?2 WHERE id = ?3",
                libsql::params![episode, now, id],
            )
            .await?;
        Ok(())
    }

    /// Deletes a series and all of its `seen`/`pending`/`interactions` rows
    /// (via `ON DELETE CASCADE`).
    pub async fn delete_series(&self, id: i64) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM series WHERE id = ?1", libsql::params![id])
            .await?;
        Ok(())
    }
}
