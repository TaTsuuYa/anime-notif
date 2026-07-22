//! Row types returned by [`crate::Store`] queries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A row from the `series` table: a show tracked from a given source, with
/// its mutable, CLI-owned state (category, alias, last episode/interaction).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeriesRow {
    /// Auto-generated numeric id.
    pub id: i64,
    /// Id of the source plugin this show comes from.
    pub source_id: String,
    /// The show's title, as reported by the source.
    pub title: String,
    /// User-assigned unique alias, if set.
    pub alias: Option<String>,
    /// Current category name.
    pub category: String,
    /// Last episode interacted with (notified/downloaded), if any.
    pub last_episode: Option<String>,
    /// Timestamp of the last interaction (download/whitelist/blacklist/
    /// category/alias change), if any.
    pub last_interaction_at: Option<DateTime<Utc>>,
    /// Cached cover image URL, if the source provides one.
    pub cover_url: Option<String>,
}

/// The kind of interaction recorded in the `interactions` log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionKind {
    /// A release was downloaded for this show.
    Download,
    /// The show's category was changed.
    Category,
    /// The show's alias was changed.
    Alias,
    /// The show was newly created (first seen from a source).
    Created,
}

impl InteractionKind {
    /// The lowercase, snake_case string stored in the `kind` column.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Download => "download",
            Self::Category => "category",
            Self::Alias => "alias",
            Self::Created => "created",
        }
    }

    /// Parses a kind back from its stored string form.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "download" => Some(Self::Download),
            "category" => Some(Self::Category),
            "alias" => Some(Self::Alias),
            "created" => Some(Self::Created),
            _ => None,
        }
    }
}

/// A row from the `interactions` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionRow {
    /// Auto-generated numeric id.
    pub id: i64,
    /// The show this interaction is about.
    pub series_id: i64,
    /// What kind of interaction this was.
    pub kind: InteractionKind,
    /// Free-form detail, e.g. the new category/alias/resolution.
    pub detail: Option<String>,
    /// When this interaction happened.
    pub at: DateTime<Utc>,
}

/// A row from the `pending` table: an episode seen without its desired
/// resolution yet, awaiting either the desired resolution or a fallback
/// timeout (see `docs/downloads.md`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingRow {
    /// The show this pending episode belongs to.
    pub series_id: i64,
    /// The episode identifier (as reported by the source).
    pub episode: String,
    /// When this episode was first observed without the desired resolution.
    pub first_seen_at: DateTime<Utc>,
    /// The resolution that would satisfy this pending entry immediately.
    pub desired_resolution: String,
    /// Best resolution seen so far for this episode, if any variant has been
    /// recorded.
    pub best_resolution: Option<String>,
    /// The best-so-far release, serialized as JSON
    /// ([`anime_notif_core::Release`]), used as the fallback if the
    /// desired resolution never appears in time.
    pub best_variant_json: Option<String>,
}
