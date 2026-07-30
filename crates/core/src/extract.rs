//! Turns a source's raw JSON response into normalized [`Release`]s, using a
//! [`CompiledSource`]'s field extractors. Malformed items are skipped and
//! reported as warnings rather than aborting the whole poll — one bad entry
//! in an API response shouldn't hide every other release.

use serde_json::Value;

use crate::model::{DownloadMethod, Release};
use crate::source::CompiledSource;

/// The output of [`extract`]: every successfully normalized release, plus
/// human-readable warnings for items/variants that were skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractionResult {
    /// Successfully extracted releases.
    pub releases: Vec<Release>,
    /// One entry per skipped item/variant, e.g. `"item 3: missing required
    /// field 'series'"` — surfaced by `anime-notif source test`.
    pub warnings: Vec<String>,
}

/// Extracts every release from `root` (the parsed JSON response) using
/// `source`'s compiled field rules.
pub fn extract(source: &CompiledSource, root: &Value) -> ExtractionResult {
    let mut result = ExtractionResult::default();

    for (i, item) in source.items.eval_all(root).into_iter().enumerate() {
        let Some(series_title) = source.fields.series.extract(item) else {
            result
                .warnings
                .push(format!("item {i}: missing required field 'series'"));
            continue;
        };
        let episode = source.fields.episode.extract(item).unwrap_or_default();

        if let Some(batch) = &source.batch {
            if batch.ignore && batch.matches(item, &episode) {
                result.warnings.push(format!(
                    "item {i} ({series_title:?}, episode {episode:?}): skipped batch release"
                ));
                continue;
            }
        }

        let (episode, version) = match source
            .version
            .as_ref()
            .and_then(|v| v.extract(item, &episode))
        {
            Some((base, n)) => (base, n),
            None => (episode, 1),
        };

        let season = source.fields.season.extract(item);
        let cover_url = source.fields.cover.extract(item);
        let show_url = source.fields.show_url.extract(item);
        let raw_id = source.fields.id.extract(item);

        for (j, variant) in source.variants.eval_all(item).into_iter().enumerate() {
            let Some(resolution) = source.fields.variant.resolution.extract(variant) else {
                result.warnings.push(format!(
                    "item {i} variant {j} ({series_title:?}): missing required field 'resolution'"
                ));
                continue;
            };
            let Some(method_raw) = source.fields.variant.method.extract(variant) else {
                result.warnings.push(format!(
                    "item {i} variant {j} ({series_title:?}): missing required field 'method'"
                ));
                continue;
            };
            let Some(method) = DownloadMethod::parse(&method_raw) else {
                result.warnings.push(format!(
                    "item {i} variant {j} ({series_title:?}): unknown download method {method_raw:?}"
                ));
                continue;
            };
            let Some(link) = source.fields.variant.link.extract(variant) else {
                result.warnings.push(format!(
                    "item {i} variant {j} ({series_title:?}): missing required field 'link'"
                ));
                continue;
            };

            result.releases.push(Release {
                source_id: source.id.clone(),
                series_title: series_title.clone(),
                episode: episode.clone(),
                season: season.clone(),
                resolution,
                method,
                link,
                cover_url: cover_url.clone(),
                source_icon_url: source.icon_url.clone(),
                show_url: show_url.clone(),
                raw_id: raw_id.clone(),
                version,
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourcePlugin;
    use std::path::Path;

    const SUBSPLEASE_TOML: &str = r#"
id = "subsplease"
endpoint = "https://subsplease.org/api/"
method = "GET"
query = { f = "latest", tz = "Africa/Casablanca" }

items = ".[]"
variants = ".downloads[]"

[fields]
series  = { path = ".show" }
episode = { path = ".episode", default = "?" }
season  = { path = ".season", default = "1" }
cover   = { path = ".image_url", prefix = "https://subsplease.org" }
id      = { path = ".page" }

[fields.variant]
resolution = { path = ".res", regex = "(\\d+)", default = "480" }
method     = { default = "magnet" }
link       = { path = ".magnet" }
"#;

    fn subsplease_sample() -> Value {
        serde_json::json!({
            "25-abc": {
                "show": "One Piece",
                "episode": "1121",
                "image_url": "/wp-content/uploads/one-piece.jpg",
                "page": "one-piece",
                "downloads": [
                    {"res": "1080p", "magnet": "magnet:?xt=urn:btih:aaa"},
                    {"res": "720p", "magnet": "magnet:?xt=urn:btih:bbb"}
                ]
            },
            "25-def": {
                "show": "Naruto",
                "episode": "?",
                "downloads": [
                    {"res": "unknown-quality", "magnet": "magnet:?xt=urn:btih:ccc"}
                ]
            }
        })
    }

    #[test]
    fn extracts_full_subsplease_sample() {
        let source = SourcePlugin::parse(SUBSPLEASE_TOML, Path::new("subsplease.toml")).unwrap();
        let result = extract(&source, &subsplease_sample());

        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
        assert_eq!(result.releases.len(), 3);

        let op_1080 = result
            .releases
            .iter()
            .find(|r| r.series_title == "One Piece" && r.resolution == "1080")
            .unwrap();
        assert_eq!(op_1080.episode, "1121");
        assert_eq!(op_1080.method, DownloadMethod::Magnet);
        assert_eq!(op_1080.link, "magnet:?xt=urn:btih:aaa");
        assert_eq!(
            op_1080.cover_url.as_deref(),
            Some("https://subsplease.org/wp-content/uploads/one-piece.jpg")
        );
        assert_eq!(op_1080.raw_id.as_deref(), Some("one-piece"));

        let naruto = result
            .releases
            .iter()
            .find(|r| r.series_title == "Naruto")
            .unwrap();
        // regex didn't match "unknown-quality" -> falls back to default "480"
        assert_eq!(naruto.resolution, "480");
        assert_eq!(naruto.cover_url, None);
    }

    #[test]
    fn missing_series_field_is_a_warning_not_a_panic() {
        let source = SourcePlugin::parse(SUBSPLEASE_TOML, Path::new("t.toml")).unwrap();
        let root = serde_json::json!({"x": {"downloads": []}});
        let result = extract(&source, &root);
        assert_eq!(result.releases.len(), 0);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing required field 'series'"));
    }

    #[test]
    fn item_with_no_variants_produces_no_releases_but_no_warning() {
        let source = SourcePlugin::parse(SUBSPLEASE_TOML, Path::new("t.toml")).unwrap();
        let root = serde_json::json!({"x": {"show": "Empty Show", "downloads": []}});
        let result = extract(&source, &root);
        assert_eq!(result.releases.len(), 0);
        assert!(result.warnings.is_empty());
    }

    const SUBSPLEASE_TOML_WITH_BATCH: &str = r#"
id = "subsplease"
endpoint = "https://subsplease.org/api/"
method = "GET"

items = ".[]"
variants = ".downloads[]"

[fields]
series  = { path = ".show" }
episode = { path = ".episode", default = "?" }

[fields.variant]
resolution = { path = ".res", regex = "(\\d+)", default = "480" }
method     = { default = "magnet" }
link       = { path = ".magnet" }

[batch]
regex = "^\\d+\\s*-\\s*\\d+$"
"#;

    fn batch_sample() -> Value {
        // Shaped after a real SubsPlease response: a normal episode
        // alongside a batch release (episode = "01-22").
        serde_json::json!({
            "dr-stone-1": {
                "show": "Dr. Stone S3",
                "episode": "22",
                "downloads": [{"res": "1080p", "magnet": "magnet:?xt=urn:btih:single"}]
            },
            "dr-stone-batch": {
                "show": "Dr. Stone S3",
                "episode": "01-22",
                "downloads": [{"res": "1080p", "magnet": "magnet:?xt=urn:btih:batch"}]
            }
        })
    }

    #[test]
    fn batch_release_is_skipped_by_default() {
        let source = SourcePlugin::parse(SUBSPLEASE_TOML_WITH_BATCH, Path::new("t.toml")).unwrap();
        let result = extract(&source, &batch_sample());

        assert_eq!(result.releases.len(), 1);
        assert_eq!(result.releases[0].episode, "22");
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("skipped batch release"));
        assert!(result.warnings[0].contains("01-22"));
    }

    #[test]
    fn batch_release_is_included_when_ignore_is_false() {
        let toml = SUBSPLEASE_TOML_WITH_BATCH.replace(
            "regex = \"^\\\\d+\\\\s*-\\\\s*\\\\d+$\"",
            "regex = \"^\\\\d+\\\\s*-\\\\s*\\\\d+$\"\nignore = false",
        );
        let source = SourcePlugin::parse(&toml, Path::new("t.toml")).unwrap();
        let result = extract(&source, &batch_sample());

        assert_eq!(result.releases.len(), 2);
        assert!(result.warnings.is_empty());
        assert!(result.releases.iter().any(|r| r.episode == "01-22"));
    }

    #[test]
    fn no_batch_config_never_filters_anything() {
        // SUBSPLEASE_TOML (no [batch] table) applied to data containing a
        // range-like episode: nothing should be filtered.
        let source = SourcePlugin::parse(SUBSPLEASE_TOML, Path::new("t.toml")).unwrap();
        let root = serde_json::json!({
            "x": {
                "show": "Dr. Stone S3",
                "episode": "01-22",
                "downloads": [{"res": "1080p", "magnet": "magnet:?xt=urn:btih:batch"}]
            }
        });
        let result = extract(&source, &root);
        assert_eq!(result.releases.len(), 1);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn source_icon_url_is_copied_onto_every_release() {
        let with_icon = SUBSPLEASE_TOML.replace(
            "endpoint = \"https://subsplease.org/api/\"",
            "endpoint = \"https://subsplease.org/api/\"\nicon = \"https://subsplease.org/favicon.ico\"",
        );
        let source = SourcePlugin::parse(&with_icon, Path::new("t.toml")).unwrap();
        let result = extract(&source, &subsplease_sample());

        assert!(!result.releases.is_empty());
        for release in &result.releases {
            assert_eq!(
                release.source_icon_url.as_deref(),
                Some("https://subsplease.org/favicon.ico")
            );
        }
    }

    #[test]
    fn source_icon_url_is_none_when_not_configured() {
        let source = SourcePlugin::parse(SUBSPLEASE_TOML, Path::new("t.toml")).unwrap();
        let result = extract(&source, &subsplease_sample());
        assert!(result.releases.iter().all(|r| r.source_icon_url.is_none()));
    }

    #[test]
    fn no_version_table_means_every_release_is_version_one() {
        let source = SourcePlugin::parse(SUBSPLEASE_TOML, Path::new("t.toml")).unwrap();
        let root = serde_json::json!({
            "x": {
                "show": "Show",
                "episode": "08v2",
                "downloads": [{"res": "1080p", "magnet": "magnet:?xt=urn:btih:x"}]
            }
        });
        let result = extract(&source, &root);
        assert_eq!(result.releases.len(), 1);
        assert_eq!(result.releases[0].version, 1);
        assert_eq!(result.releases[0].episode, "08v2");
    }

    const SUBSPLEASE_TOML_WITH_VERSION: &str = r#"
id = "subsplease"
endpoint = "https://subsplease.org/api/"
method = "GET"

items = ".[]"
variants = ".downloads[]"

[fields]
series  = { path = ".show" }
episode = { path = ".episode", default = "?" }

[fields.variant]
resolution = { path = ".res", regex = "(\\d+)", default = "480" }
method     = { default = "magnet" }
link       = { path = ".magnet" }

[version]
regex = "^(?P<episode>\\d+)v(?P<version>\\d+)$"
"#;

    #[test]
    fn versioned_episode_is_split_into_base_episode_and_version() {
        let source =
            SourcePlugin::parse(SUBSPLEASE_TOML_WITH_VERSION, Path::new("t.toml")).unwrap();
        let root = serde_json::json!({
            "x": {
                "show": "Show",
                "episode": "08v2",
                "downloads": [{"res": "1080p", "magnet": "magnet:?xt=urn:btih:x"}]
            }
        });
        let result = extract(&source, &root);
        assert_eq!(result.releases.len(), 1);
        assert_eq!(result.releases[0].episode, "08");
        assert_eq!(result.releases[0].version, 2);
    }

    #[test]
    fn unversioned_episode_is_left_unchanged_with_version_table_configured() {
        let source =
            SourcePlugin::parse(SUBSPLEASE_TOML_WITH_VERSION, Path::new("t.toml")).unwrap();
        let root = serde_json::json!({
            "x": {
                "show": "Show",
                "episode": "08",
                "downloads": [{"res": "1080p", "magnet": "magnet:?xt=urn:btih:x"}]
            }
        });
        let result = extract(&source, &root);
        assert_eq!(result.releases.len(), 1);
        assert_eq!(result.releases[0].episode, "08");
        assert_eq!(result.releases[0].version, 1);
    }
}
