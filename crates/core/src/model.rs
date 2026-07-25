//! Normalized domain types shared across the workspace: releases fetched from
//! sources, and the download methods they can be delivered by.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How a release's payload is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadMethod {
    /// A plain HTTP(S) URL pointing directly at the file.
    Direct,
    /// A URL to a `.torrent` file.
    Torrent,
    /// A `magnet:` URI.
    Magnet,
}

impl DownloadMethod {
    /// Parses a method from a source's raw string, matching case-insensitively
    /// and accepting a few common synonyms.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "direct" | "http" | "https" | "url" => Some(Self::Direct),
            "torrent" | "torrent_file" => Some(Self::Torrent),
            "magnet" => Some(Self::Magnet),
            _ => None,
        }
    }

    /// The lowercase name used in config keys and download-path segments.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Torrent => "torrent",
            Self::Magnet => "magnet",
        }
    }
}

impl std::fmt::Display for DownloadMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One resolution/method/link combination for a given episode, as extracted
/// from a source's raw JSON response and normalized against its plugin config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    /// Id of the source plugin this release came from.
    pub source_id: String,
    /// The series' title as reported by the source.
    pub series_title: String,
    /// Episode number, kept as a string since sources use varied formats
    /// ("12", "12.5", "S2E03").
    pub episode: String,
    /// Season, when the source distinguishes it.
    pub season: Option<String>,
    /// Resolution label, e.g. "1080p".
    pub resolution: String,
    /// How this variant is downloaded.
    pub method: DownloadMethod,
    /// The URL or magnet URI for this variant.
    pub link: String,
    /// Series cover image URL, if the source provides one.
    pub cover_url: Option<String>,
    /// The show's page on the source's website, if the source provides
    /// enough information to build one. Used as a notification's
    /// click-to-open target (`docs/notifications.md`).
    pub show_url: Option<String>,
    /// A source-provided stable id for the release, if any, used for dedup
    /// in preference to a computed hash.
    pub raw_id: Option<String>,
}

impl Release {
    /// A stable key identifying "this exact episode variant" across polls.
    ///
    /// Uses the source-provided [`Release::raw_id`] when present (it is
    /// combined with the variant fields, since one raw id may cover several
    /// resolution/method combinations); otherwise falls back to a SHA-256
    /// hash of the identifying fields.
    pub fn dedup_key(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.source_id.as_bytes());
        hasher.update(b"\0");
        if let Some(raw_id) = &self.raw_id {
            hasher.update(raw_id.as_bytes());
        } else {
            hasher.update(self.series_title.as_bytes());
        }
        hasher.update(b"\0");
        hasher.update(self.episode.as_bytes());
        hasher.update(b"\0");
        hasher.update(self.resolution.as_bytes());
        hasher.update(b"\0");
        hasher.update(self.method.as_str().as_bytes());
        hasher.update(b"\0");
        hasher.update(self.link.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// A key identifying "this episode of this series", independent of
    /// resolution/method — used to group variants for the pending/
    /// resolution-wait workflow.
    pub fn episode_key(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.source_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(self.series_title.as_bytes());
        hasher.update(b"\0");
        hasher.update(self.episode.as_bytes());
        if let Some(season) = &self.season {
            hasher.update(b"\0");
            hasher.update(season.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(resolution: &str, link: &str) -> Release {
        Release {
            source_id: "test".into(),
            series_title: "Show".into(),
            episode: "1".into(),
            season: None,
            resolution: resolution.into(),
            method: DownloadMethod::Direct,
            link: link.into(),
            cover_url: None,
            show_url: None,
            raw_id: None,
        }
    }

    #[test]
    fn dedup_key_differs_by_resolution() {
        let a = release("1080p", "http://x/a");
        let b = release("720p", "http://x/a");
        assert_ne!(a.dedup_key(), b.dedup_key());
    }

    #[test]
    fn dedup_key_stable_for_identical_input() {
        let a = release("1080p", "http://x/a");
        let b = release("1080p", "http://x/a");
        assert_eq!(a.dedup_key(), b.dedup_key());
    }

    #[test]
    fn episode_key_ignores_resolution() {
        let a = release("1080p", "http://x/a");
        let b = release("720p", "http://x/b");
        assert_eq!(a.episode_key(), b.episode_key());
    }

    #[test]
    fn download_method_parse_synonyms() {
        assert_eq!(DownloadMethod::parse("HTTP"), Some(DownloadMethod::Direct));
        assert_eq!(
            DownloadMethod::parse("Magnet"),
            Some(DownloadMethod::Magnet)
        );
        assert_eq!(
            DownloadMethod::parse("torrent_file"),
            Some(DownloadMethod::Torrent)
        );
        assert_eq!(DownloadMethod::parse("carrier-pigeon"), None);
    }
}
