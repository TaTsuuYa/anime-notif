//! The shareable source-plugin schema: a TOML file describing how to poll a
//! JSON API for anime releases and where, in its response, to find each
//! field — using jq-style paths ([`crate::jqpath`]), optional regex
//! extraction, defaults, and a URL prefix for relative links/images.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::jqpath::JqPath;

/// Errors loading or compiling a source plugin file.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// The plugin file could not be read from disk.
    #[error("failed to read source plugin {path}: {source}")]
    Read {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The plugin file's contents were not valid TOML, or didn't match the
    /// expected schema.
    #[error("failed to parse source plugin {path}: {source}")]
    Parse {
        /// Path that failed to parse.
        path: PathBuf,
        /// Underlying TOML deserialization error.
        #[source]
        source: Box<toml::de::Error>,
    },
    /// The plugin parsed but failed a semantic check: an unparseable jq
    /// path, an invalid regex, or a missing required field.
    #[error("invalid source plugin: {0}")]
    Invalid(String),
}

fn default_http_method() -> String {
    "GET".to_string()
}

fn default_variants_path() -> String {
    ".".to_string()
}

fn default_true() -> bool {
    true
}

/// One field extraction rule: a jq path, an optional regex applied to the
/// path's result (first capture group, or the whole match if the pattern
/// has none), an optional default when the value is missing/unmatched, and
/// an optional prefix to make a relative URL absolute.
///
/// `path` is optional so a field can be a constant: `{ default = "magnet" }`
/// with no `path` always yields that constant.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldExtractor {
    /// jq-style path to the raw value, relative to the item/variant being
    /// extracted from.
    #[serde(default)]
    pub path: Option<String>,
    /// Regex applied to the raw value; the first capture group is used, or
    /// the whole match if the pattern has no groups.
    #[serde(default)]
    pub regex: Option<String>,
    /// Value used when `path` is unset, matches nothing, or (after `regex`)
    /// fails to match.
    #[serde(default)]
    pub default: Option<String>,
    /// String prepended to the extracted value, e.g. to absolutize a
    /// relative image/link URL.
    #[serde(default)]
    pub prefix: Option<String>,
    /// String appended to the extracted value, e.g. to build a page URL
    /// from a slug (`prefix = "https://example.com/shows/"`,
    /// `suffix = "/"`).
    #[serde(default)]
    pub suffix: Option<String>,
}

/// The `resolution`/`method`/`link` fields extracted per download variant.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantFields {
    /// e.g. `"1080p"`.
    #[serde(default)]
    pub resolution: FieldExtractor,
    /// Must resolve (after regex/default) to `direct`/`torrent`/`magnet`
    /// (or a recognized synonym — see [`crate::model::DownloadMethod::parse`]).
    #[serde(default)]
    pub method: FieldExtractor,
    /// The URL or magnet URI.
    #[serde(default)]
    pub link: FieldExtractor,
}

/// All field extraction rules for a source: the series-level fields plus
/// the nested variant fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldSet {
    /// Show title. Required.
    pub series: FieldExtractor,
    /// Episode identifier. Kept as a string; give it a `default` if the API
    /// sometimes omits it.
    #[serde(default)]
    pub episode: FieldExtractor,
    /// Season identifier, if the source distinguishes seasons.
    #[serde(default)]
    pub season: FieldExtractor,
    /// Cover image URL.
    #[serde(default)]
    pub cover: FieldExtractor,
    /// The show's page on the source's website, e.g. built from a slug via
    /// `prefix`/`suffix`. Used as a notification's click-to-open target
    /// (`docs/notifications.md`).
    #[serde(default)]
    pub show_url: FieldExtractor,
    /// A source-provided stable id, used for dedup in preference to a
    /// computed hash.
    #[serde(default)]
    pub id: FieldExtractor,
    /// Per-variant fields, extracted from each element of `variants`.
    #[serde(default)]
    pub variant: VariantFields,
}

/// Describes how to recognize a "batch" release for this source — one
/// release bundling multiple episodes together (e.g. SubsPlease reports
/// these with an `episode` value like `"01-22"` instead of a single
/// number) — and whether to skip them.
///
/// Optional: a plugin with no `[batch]` table never flags anything as a
/// batch, so existing plugins are unaffected.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchConfig {
    /// jq path, relative to the item, to match `regex` against. Defaults
    /// to the already-extracted `episode` field's value when unset — the
    /// common case, since a batch is usually signalled by the episode
    /// field itself being a range.
    #[serde(default)]
    pub path: Option<String>,
    /// If this regex matches the target value, the item is a batch. Only
    /// whether it matches is used — capture groups, if any, are ignored.
    pub regex: String,
    /// Whether to skip batch releases entirely (not extracted into any
    /// `Release` at all). Defaults to `true`.
    #[serde(default = "default_true")]
    pub ignore: bool,
}

/// A source plugin, as loaded from TOML: the endpoint to poll, how to
/// authenticate, and where to find each field in the response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePlugin {
    /// Unique id for this source, used as `Release::source_id`, in download
    /// path overrides, and in the `pending`/`seen` tables.
    pub id: String,
    /// The API endpoint to poll.
    pub endpoint: String,
    /// HTTP method: `GET` or `POST`.
    #[serde(default = "default_http_method")]
    pub method: String,
    /// Poll interval override; falls back to `general.default_interval`
    /// when unset.
    #[serde(default, with = "humantime_serde::option")]
    pub interval: Option<Duration>,
    /// Resolution-wait override; falls back to `downloads.resolution_wait`
    /// when unset.
    #[serde(default, with = "humantime_serde::option")]
    pub resolution_wait: Option<Duration>,
    /// Extra HTTP headers (values may reference `${VAR}`).
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Query string parameters.
    #[serde(default)]
    pub query: HashMap<String, String>,
    /// Request body, for `POST`.
    #[serde(default)]
    pub body: Option<String>,
    /// jq path to the array (or object, whose values are iterated) of
    /// releases in the response.
    pub items: String,
    /// jq path, relative to each item, to its download variants. Defaults
    /// to `"."` (the item itself is the single variant) for APIs that don't
    /// nest multiple resolutions/methods per item.
    #[serde(default = "default_variants_path")]
    pub variants: String,
    /// Field extraction rules.
    pub fields: FieldSet,
    /// How to recognize (and whether to skip) batch releases. Omit to
    /// never flag anything as a batch.
    #[serde(default)]
    pub batch: Option<BatchConfig>,
}

impl SourcePlugin {
    /// Loads and compiles a plugin file.
    pub fn load(path: &Path) -> Result<CompiledSource, SourceError> {
        let raw = std::fs::read_to_string(path).map_err(|source| SourceError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&raw, path)
    }

    /// Parses and compiles plugin content from an already-read string.
    /// `path` is only used to annotate error messages.
    pub fn parse(raw: &str, path: &Path) -> Result<CompiledSource, SourceError> {
        let expanded =
            shellexpand::env_with_context_no_errors(raw, |name| std::env::var(name).ok());
        let plugin: SourcePlugin =
            toml::from_str(&expanded).map_err(|source| SourceError::Parse {
                path: path.to_path_buf(),
                source: Box::new(source),
            })?;
        plugin.compile()
    }

    /// Validates and pre-parses every jq path and regex in this plugin,
    /// producing a [`CompiledSource`] ready for repeated use by the fetch
    /// loop without re-parsing on every poll.
    pub fn compile(&self) -> Result<CompiledSource, SourceError> {
        if self.id.trim().is_empty() {
            return Err(SourceError::Invalid("source id cannot be empty".into()));
        }
        let items = parse_path("items", &self.items)?;
        let variants = parse_path("variants", &self.variants)?;
        let fields = CompiledFieldSet {
            series: compile_extractor("fields.series", &self.fields.series)?,
            episode: compile_extractor("fields.episode", &self.fields.episode)?,
            season: compile_extractor("fields.season", &self.fields.season)?,
            cover: compile_extractor("fields.cover", &self.fields.cover)?,
            show_url: compile_extractor("fields.show_url", &self.fields.show_url)?,
            id: compile_extractor("fields.id", &self.fields.id)?,
            variant: CompiledVariantFields {
                resolution: compile_extractor(
                    "fields.variant.resolution",
                    &self.fields.variant.resolution,
                )?,
                method: compile_extractor("fields.variant.method", &self.fields.variant.method)?,
                link: compile_extractor("fields.variant.link", &self.fields.variant.link)?,
            },
        };
        let batch = self
            .batch
            .as_ref()
            .map(|b| {
                Ok::<_, SourceError>(CompiledBatch {
                    path: b
                        .path
                        .as_deref()
                        .map(|p| parse_path("batch.path", p))
                        .transpose()?,
                    regex: Regex::new(&b.regex)
                        .map_err(|e| SourceError::Invalid(format!("batch.regex: {e}")))?,
                    ignore: b.ignore,
                })
            })
            .transpose()?;

        Ok(CompiledSource {
            id: self.id.clone(),
            endpoint: self.endpoint.clone(),
            method: self.method.clone(),
            interval: self.interval,
            resolution_wait: self.resolution_wait,
            headers: self.headers.clone(),
            query: self.query.clone(),
            body: self.body.clone(),
            items,
            variants,
            fields,
            batch,
        })
    }
}

fn parse_path(field: &str, raw: &str) -> Result<JqPath, SourceError> {
    JqPath::parse(raw).map_err(|e| SourceError::Invalid(format!("{field}: {e}")))
}

fn compile_extractor(field: &str, raw: &FieldExtractor) -> Result<CompiledExtractor, SourceError> {
    let path = raw
        .path
        .as_deref()
        .map(|p| parse_path(field, p))
        .transpose()?;
    let regex = raw
        .regex
        .as_deref()
        .map(|r| Regex::new(r).map_err(|e| SourceError::Invalid(format!("{field}.regex: {e}"))))
        .transpose()?;
    Ok(CompiledExtractor {
        path,
        regex,
        default: raw.default.clone(),
        prefix: raw.prefix.clone(),
        suffix: raw.suffix.clone(),
    })
}

/// A [`FieldExtractor`] with its path pre-parsed and regex pre-compiled.
#[derive(Debug, Clone)]
pub struct CompiledExtractor {
    path: Option<JqPath>,
    regex: Option<Regex>,
    default: Option<String>,
    prefix: Option<String>,
    suffix: Option<String>,
}

impl CompiledExtractor {
    /// Extracts this field's value from `value` (an item or a variant):
    /// evaluate `path` (if any) → apply `regex` (if any) → fall back to
    /// `default` → apply `prefix`/`suffix`.
    pub fn extract(&self, value: &Value) -> Option<String> {
        let mut result = self
            .path
            .as_ref()
            .and_then(|p| p.eval_first(value))
            .and_then(value_to_string);

        if let Some(re) = &self.regex {
            result = result.and_then(|s| {
                re.captures(&s)
                    .and_then(|c| c.get(1).or_else(|| c.get(0)))
                    .map(|m| m.as_str().to_string())
            });
        }

        let result = result.or_else(|| self.default.clone());

        result.map(|s| {
            let prefixed = match &self.prefix {
                Some(prefix) => format!("{prefix}{s}"),
                None => s,
            };
            match &self.suffix {
                Some(suffix) => format!("{prefixed}{suffix}"),
                None => prefixed,
            }
        })
    }
}

pub(crate) fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// A compiled [`BatchConfig`].
#[derive(Debug, Clone)]
pub struct CompiledBatch {
    path: Option<JqPath>,
    regex: Regex,
    /// Whether to skip items this detects as a batch.
    pub ignore: bool,
}

impl CompiledBatch {
    /// Whether `item` is a batch release: evaluates `path` against `item`
    /// (falling back to `episode`, the already-extracted episode value,
    /// when `path` is unset) and checks whether `regex` matches it.
    pub fn matches(&self, item: &Value, episode: &str) -> bool {
        let target = match &self.path {
            Some(path) => path.eval_first(item).and_then(value_to_string),
            None => Some(episode.to_string()),
        };
        target.is_some_and(|t| self.regex.is_match(&t))
    }
}

/// Compiled variant field extractors.
#[derive(Debug, Clone)]
pub struct CompiledVariantFields {
    /// Resolution extractor.
    pub resolution: CompiledExtractor,
    /// Download method extractor.
    pub method: CompiledExtractor,
    /// Link/magnet-URI extractor.
    pub link: CompiledExtractor,
}

/// Compiled field extractors for a source.
#[derive(Debug, Clone)]
pub struct CompiledFieldSet {
    /// Series title extractor.
    pub series: CompiledExtractor,
    /// Episode extractor.
    pub episode: CompiledExtractor,
    /// Season extractor.
    pub season: CompiledExtractor,
    /// Cover image URL extractor.
    pub cover: CompiledExtractor,
    /// Show page URL extractor.
    pub show_url: CompiledExtractor,
    /// Stable id extractor.
    pub id: CompiledExtractor,
    /// Per-variant extractors.
    pub variant: CompiledVariantFields,
}

/// A [`SourcePlugin`] with every path parsed and every regex compiled,
/// ready to be polled and have its responses extracted repeatedly. Build
/// one with [`SourcePlugin::compile`], [`SourcePlugin::load`], or
/// [`SourcePlugin::parse`].
#[derive(Debug, Clone)]
pub struct CompiledSource {
    /// Source id.
    pub id: String,
    /// API endpoint.
    pub endpoint: String,
    /// HTTP method.
    pub method: String,
    /// Poll interval override.
    pub interval: Option<Duration>,
    /// Resolution-wait override.
    pub resolution_wait: Option<Duration>,
    /// Extra HTTP headers.
    pub headers: HashMap<String, String>,
    /// Query string parameters.
    pub query: HashMap<String, String>,
    /// Request body.
    pub body: Option<String>,
    /// Path to the release array.
    pub items: JqPath,
    /// Path (relative to each item) to its download variants.
    pub variants: JqPath,
    /// Field extractors.
    pub fields: CompiledFieldSet,
    /// Batch-release detection, if configured.
    pub batch: Option<CompiledBatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUBSPLEASE_TOML: &str = r#"
id = "subsplease"
endpoint = "https://subsplease.org/api/"
method = "GET"
interval = "10m"
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

    #[test]
    fn compiles_subsplease_example() {
        let compiled = SourcePlugin::parse(SUBSPLEASE_TOML, Path::new("subsplease.toml")).unwrap();
        assert_eq!(compiled.id, "subsplease");
        assert_eq!(compiled.query.get("f").map(String::as_str), Some("latest"));
        assert_eq!(compiled.interval, Some(Duration::from_secs(600)));
    }

    #[test]
    fn rejects_bad_jq_path() {
        let bad = SUBSPLEASE_TOML.replace("items = \".[]\"", "items = \"not-a-path\"");
        let err = SourcePlugin::parse(&bad, Path::new("t.toml")).unwrap_err();
        assert!(matches!(err, SourceError::Invalid(_)));
    }

    #[test]
    fn rejects_bad_regex() {
        let bad = SUBSPLEASE_TOML.replace("regex = \"(\\\\d+)\"", "regex = \"(unclosed\"");
        let err = SourcePlugin::parse(&bad, Path::new("t.toml")).unwrap_err();
        assert!(matches!(err, SourceError::Invalid(_)));
    }

    #[test]
    fn constant_field_extracts_default_with_no_path() {
        let compiled = SourcePlugin::parse(SUBSPLEASE_TOML, Path::new("t.toml")).unwrap();
        let empty = Value::Null;
        assert_eq!(
            compiled.fields.variant.method.extract(&empty),
            Some("magnet".to_string())
        );
    }

    #[test]
    fn prefix_is_applied() {
        let compiled = SourcePlugin::parse(SUBSPLEASE_TOML, Path::new("t.toml")).unwrap();
        let item = serde_json::json!({"image_url": "/img/x.jpg"});
        assert_eq!(
            compiled.fields.cover.extract(&item),
            Some("https://subsplease.org/img/x.jpg".to_string())
        );
    }

    #[test]
    fn prefix_and_suffix_both_apply_for_show_url() {
        let toml = SUBSPLEASE_TOML.replace(
            "id      = { path = \".page\" }",
            "id      = { path = \".page\" }\nshow_url = { path = \".page\", prefix = \"https://subsplease.org/shows/\", suffix = \"/\" }",
        );
        let compiled = SourcePlugin::parse(&toml, Path::new("t.toml")).unwrap();
        let item = serde_json::json!({"page": "hanaori-san-wa-tensei-shitemo-kenka-ga-shitai"});
        assert_eq!(
            compiled.fields.show_url.extract(&item),
            Some(
                "https://subsplease.org/shows/hanaori-san-wa-tensei-shitemo-kenka-ga-shitai/"
                    .to_string()
            )
        );
    }

    #[test]
    fn regex_extracts_capture_group() {
        let compiled = SourcePlugin::parse(SUBSPLEASE_TOML, Path::new("t.toml")).unwrap();
        let variant = serde_json::json!({"res": "1080p"});
        assert_eq!(
            compiled.fields.variant.resolution.extract(&variant),
            Some("1080".to_string())
        );
    }

    #[test]
    fn regex_miss_falls_back_to_default() {
        let compiled = SourcePlugin::parse(SUBSPLEASE_TOML, Path::new("t.toml")).unwrap();
        let variant = serde_json::json!({"res": "unknown"});
        assert_eq!(
            compiled.fields.variant.resolution.extract(&variant),
            Some("480".to_string())
        );
    }

    #[test]
    fn no_batch_table_means_batch_is_never_configured() {
        let compiled = SourcePlugin::parse(SUBSPLEASE_TOML, Path::new("t.toml")).unwrap();
        assert!(compiled.batch.is_none());
    }

    #[test]
    fn batch_defaults_to_ignore_true() {
        let toml = format!("{SUBSPLEASE_TOML}\n[batch]\nregex = \"^\\\\d+\\\\s*-\\\\s*\\\\d+$\"\n");
        let compiled = SourcePlugin::parse(&toml, Path::new("t.toml")).unwrap();
        let batch = compiled.batch.as_ref().unwrap();
        assert!(batch.ignore);
    }

    #[test]
    fn batch_matches_episode_range_by_default() {
        let toml = format!("{SUBSPLEASE_TOML}\n[batch]\nregex = \"^\\\\d+\\\\s*-\\\\s*\\\\d+$\"\n");
        let compiled = SourcePlugin::parse(&toml, Path::new("t.toml")).unwrap();
        let batch = compiled.batch.as_ref().unwrap();
        let item = serde_json::json!({});
        assert!(batch.matches(&item, "01-22"));
        assert!(!batch.matches(&item, "22"));
    }

    #[test]
    fn batch_can_match_a_different_field_via_path() {
        let toml =
            format!("{SUBSPLEASE_TOML}\n[batch]\npath = \".title\"\nregex = \"(?i)batch\"\n");
        let compiled = SourcePlugin::parse(&toml, Path::new("t.toml")).unwrap();
        let batch = compiled.batch.as_ref().unwrap();
        let item = serde_json::json!({"title": "[SubsPlease] Dr. Stone S3 (01-22) [Batch]"});
        assert!(batch.matches(&item, "irrelevant"));
        let non_batch_item = serde_json::json!({"title": "[SubsPlease] Dr. Stone S3 - 15"});
        assert!(!batch.matches(&non_batch_item, "irrelevant"));
    }

    #[test]
    fn batch_ignore_can_be_disabled() {
        let toml = format!(
            "{SUBSPLEASE_TOML}\n[batch]\nregex = \"^\\\\d+\\\\s*-\\\\s*\\\\d+$\"\nignore = false\n"
        );
        let compiled = SourcePlugin::parse(&toml, Path::new("t.toml")).unwrap();
        assert!(!compiled.batch.unwrap().ignore);
    }
}
