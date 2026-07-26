//! The main, declarative `anime-notif` config file: sources, download paths,
//! categories and rules. This is the read-only, Nix-generatable half of the
//! service's state — mutable per-show state (category assignment, alias,
//! interaction history) lives in the database, not here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;
use crate::model::DownloadMethod;

/// Top-level config, as loaded from `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Global daemon settings.
    pub general: General,
    /// Download path resolution and favourite method/resolution.
    pub downloads: Downloads,
    /// Notification behavior not covered by categories (click-to-open, ...).
    pub notifications: NotificationsConfig,
    /// Category definitions (seeded with `liked`/`normal`/`uninterested`).
    pub categories: Vec<CategoryDef>,
    /// Declarative rules that seed/override a series' category by matching
    /// its title.
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Source plugin locations: local file paths or URLs to shared plugin
    /// files. Nix-provided sources are pre-resolved to store paths before
    /// reaching this struct.
    #[serde(default)]
    pub sources: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: General::default(),
            downloads: Downloads::default(),
            notifications: NotificationsConfig::default(),
            categories: CategoryDef::defaults(),
            rules: Vec::new(),
            sources: Vec::new(),
        }
    }
}

/// Notification behavior that isn't per-category.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NotificationsConfig {
    /// Whether clicking a notification (its body, not one of the
    /// download/whitelist/blacklist buttons) opens the show's page — see
    /// `fields.show_url` in `docs/sources.md`. Has no effect for a release
    /// whose source doesn't provide a `show_url`. Defaults to `true`.
    pub open_show_page: bool,
    /// Command used to open a show's page, e.g. `"firefox {url}"`. Unset
    /// (the default) uses the platform default opener (`xdg-open` on
    /// Linux, `open` on macOS, `cmd /C start` on Windows) — the same
    /// mechanism `magnet` links use, but configured independently, since a
    /// user who points `downloads.methods.magnet.command` at a torrent
    /// client would not want show-page clicks going there too.
    pub open_command: Option<String>,
    /// Default notification sound: a specific sound file. Takes
    /// precedence over `sound_name` if both are set.
    #[serde(default)]
    pub sound_file: Option<PathBuf>,
    /// Default notification sound: a freedesktop sound-theme name (e.g.
    /// `"message-new-instant"`), used when `sound_file` is unset.
    #[serde(default)]
    pub sound_name: Option<String>,
    /// Per-source sound overrides, keyed by source id. A source with
    /// either field set here uses this table exclusively (not merged with
    /// the global `sound_file`/`sound_name`) — see `docs/notifications.md`.
    #[serde(default)]
    pub sources: HashMap<String, SourceNotificationOverride>,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            open_show_page: true,
            open_command: None,
            sound_file: None,
            sound_name: None,
            sources: HashMap::new(),
        }
    }
}

/// A per-source notification override (currently just sound).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SourceNotificationOverride {
    /// This source's notification sound file, overriding the global
    /// `notifications.sound_file`/`sound_name`.
    pub sound_file: Option<PathBuf>,
    /// This source's notification sound-theme name, overriding the global
    /// default. Ignored if `sound_file` is also set here.
    pub sound_name: Option<String>,
}

/// Global daemon settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    /// How often to poll a source when it doesn't set its own `interval`.
    #[serde(with = "humantime_serde")]
    pub default_interval: Duration,
    /// Where series/episode/interaction state is stored.
    pub db: DbConfig,
}

impl Default for General {
    fn default() -> Self {
        Self {
            default_interval: Duration::from_secs(15 * 60),
            db: DbConfig::default(),
        }
    }
}

/// Where the sqlite-compatible database lives.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum DbConfig {
    /// A local SQLite file on disk.
    Local {
        /// Path to the database file (`~` and `$VAR` are expanded).
        path: PathBuf,
    },
    /// A remote libSQL/Turso database.
    Remote {
        /// The `libsql://...` connection URL.
        url: String,
        /// Optional auth token (commonly `${TURSO_TOKEN}` via env expansion).
        auth_token: Option<String>,
    },
}

impl Default for DbConfig {
    fn default() -> Self {
        Self::Local {
            path: crate::paths::default_db_path(),
        }
    }
}

/// Download path resolution and favourite method/resolution settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Downloads {
    /// Base directory releases are downloaded under, when no override
    /// applies: `<base_dir>/<source>/<method>`.
    pub base_dir: PathBuf,
    /// Favourite download method for liked/auto-downloaded shows.
    pub default_method: DownloadMethod,
    /// Desired resolution; releases below this trigger the resolution-wait
    /// workflow (see [`Downloads::resolution_wait`]). By convention,
    /// resolution labels are bare digit strings (`"1080"`, not `"1080p"`) —
    /// source plugins normalize to this with a `(\d+)` regex on the raw
    /// value, since sources vary in whether they include the `p` suffix.
    pub default_resolution: String,
    /// Preference order used to pick a fallback resolution once
    /// `resolution_wait` elapses without the desired resolution appearing.
    pub resolution_fallback: Vec<String>,
    /// How long to wait for the desired resolution before falling back.
    #[serde(with = "humantime_serde")]
    pub resolution_wait: Duration,
    /// Per-method overrides, applied to every source unless a per-source
    /// override exists. Keyed by [`DownloadMethod::as_str`] (validated when
    /// the config is loaded); a plain `String` key is used rather than
    /// `DownloadMethod` itself since deserializing an enum as a TOML table
    /// key is not reliably supported across `toml`/serde versions.
    #[serde(default)]
    pub methods: HashMap<String, MethodOverride>,
    /// Per-source overrides, keyed by source id.
    #[serde(default)]
    pub sources: HashMap<String, SourceDownloadOverride>,
}

impl Default for Downloads {
    fn default() -> Self {
        Self {
            base_dir: crate::paths::default_download_dir(),
            default_method: DownloadMethod::Direct,
            default_resolution: "1080".to_string(),
            resolution_fallback: vec!["1080".into(), "720".into(), "480".into()],
            resolution_wait: Duration::from_secs(30 * 60),
            methods: HashMap::new(),
            sources: HashMap::new(),
        }
    }
}

/// A download-path/behavior override, applicable per-method and/or
/// per-source.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodOverride {
    /// Directory to save direct downloads / `.torrent` files fetched for
    /// inspection into.
    pub dir: Option<PathBuf>,
    /// For `torrent`: directory a torrent client watches for new files.
    pub watch_dir: Option<PathBuf>,
    /// For `magnet` (or any method): a shell command to hand the link to,
    /// with `{magnet}` / `{link}` substituted. Defaults to `xdg-open {magnet}`
    /// on Linux.
    pub command: Option<String>,
}

impl MethodOverride {
    fn merge_over(&self, base: &MethodOverride) -> MethodOverride {
        MethodOverride {
            dir: self.dir.clone().or_else(|| base.dir.clone()),
            watch_dir: self.watch_dir.clone().or_else(|| base.watch_dir.clone()),
            command: self.command.clone().or_else(|| base.command.clone()),
        }
    }
}

/// Per-source download overrides: a directory for the whole source, and/or
/// per-method overrides scoped to that source.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDownloadOverride {
    /// Directory for all of this source's downloads, regardless of method.
    pub dir: Option<PathBuf>,
    /// Per-source, per-method overrides (highest precedence). Keyed the same
    /// way as [`Downloads::methods`].
    #[serde(default)]
    pub methods: HashMap<String, MethodOverride>,
}

impl Downloads {
    /// Resolves the effective [`MethodOverride`] for `(source_id, method)`,
    /// applying the documented precedence: per-source+method > per-method >
    /// per-source > (nothing — caller falls back to `base_dir`).
    fn effective_override(&self, source_id: &str, method: DownloadMethod) -> MethodOverride {
        let per_method = self
            .methods
            .get(method.as_str())
            .cloned()
            .unwrap_or_default();
        let Some(source) = self.sources.get(source_id) else {
            return per_method;
        };
        let per_source = MethodOverride {
            dir: source.dir.clone(),
            watch_dir: None,
            command: None,
        };
        let merged = per_source.merge_over(&per_method);
        match source.methods.get(method.as_str()) {
            Some(per_source_method) => per_source_method.merge_over(&merged),
            None => merged,
        }
    }

    /// Resolves the directory a `(source_id, method)` download should be
    /// saved/dropped into, applying the override precedence documented in
    /// `docs/downloads.md`: per-source+method dir > per-method dir >
    /// per-source dir > `<base_dir>/<source_id>/<method>`.
    ///
    /// For `torrent`, prefers `watch_dir` over `dir` when both could apply,
    /// since a watch-folder is the more specific intent for that method.
    pub fn resolve_dir(&self, source_id: &str, method: DownloadMethod) -> PathBuf {
        let effective = self.effective_override(source_id, method);
        if method == DownloadMethod::Torrent {
            if let Some(watch_dir) = effective.watch_dir {
                return watch_dir;
            }
        }
        effective
            .dir
            .unwrap_or_else(|| self.base_dir.join(source_id).join(method.as_str()))
    }

    /// Resolves the hand-off command for `(source_id, method)`, if any
    /// override configures one (see [`MethodOverride::command`]).
    pub fn resolve_command(&self, source_id: &str, method: DownloadMethod) -> Option<String> {
        self.effective_override(source_id, method).command
    }
}

/// A category definition: a name plus the behavior it implies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategoryDef {
    /// Category name, referenced by rules and by the CLI's `set category`.
    pub name: String,
    /// Whether new episodes for series in this category trigger a
    /// notification.
    pub notify: bool,
    /// Whether new episodes for series in this category are automatically
    /// downloaded.
    pub auto_download: bool,
}

impl CategoryDef {
    /// The three seeded categories: `liked` (notify + auto-download),
    /// `normal` (notify only), `uninterested` (neither — this is the
    /// blacklist).
    pub fn defaults() -> Vec<Self> {
        vec![
            Self {
                name: "liked".into(),
                notify: true,
                auto_download: true,
            },
            Self {
                name: "normal".into(),
                notify: true,
                auto_download: false,
            },
            Self {
                name: "uninterested".into(),
                notify: false,
                auto_download: false,
            },
        ]
    }
}

/// A declarative rule seeding/overriding a series' category by matching its
/// title. Exactly one of `match_exact`/`match_regex` should be set.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Exact (case-sensitive) title match.
    #[serde(rename = "match", default)]
    pub match_exact: Option<String>,
    /// Regex title match, used when `match` isn't set.
    #[serde(default)]
    pub match_regex: Option<String>,
    /// Category to assign when this rule matches.
    pub category: String,
}

impl Config {
    /// Loads and validates a config file, expanding `${VAR}` environment
    /// references (unresolved references are left as-is rather than
    /// erroring) before parsing as TOML.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&raw, path)
    }

    /// Parses and validates config from an already-read string. `path` is
    /// only used to annotate error messages.
    pub fn parse(raw: &str, path: &Path) -> Result<Self, ConfigError> {
        let expanded =
            shellexpand::env_with_context_no_errors(raw, |name| std::env::var(name).ok());
        let mut config: Config =
            toml::from_str(&expanded).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                source: Box::new(source),
            })?;
        config.expand_paths();
        config.validate()?;
        Ok(config)
    }

    /// Applies `~`/env expansion to path fields that TOML parsing doesn't
    /// touch (env vars in string *values* are already expanded pre-parse;
    /// this handles `~` specifically, which we don't want expanded inside
    /// arbitrary strings like regexes).
    fn expand_paths(&mut self) {
        if let DbConfig::Local { path } = &mut self.general.db {
            *path = expand_path(path);
        }
        self.downloads.base_dir = expand_path(&self.downloads.base_dir);
        for m in self.downloads.methods.values_mut() {
            expand_method_override(m);
        }
        for s in self.downloads.sources.values_mut() {
            if let Some(dir) = &mut s.dir {
                *dir = expand_path(dir);
            }
            for m in s.methods.values_mut() {
                expand_method_override(m);
            }
        }
    }

    /// Semantic validation beyond what serde/TOML parsing already enforces:
    /// unique, non-empty category names, and rules/defaults referencing only
    /// defined categories.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.categories.is_empty() {
            return Err(ConfigError::Invalid(
                "at least one category must be defined".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for c in &self.categories {
            if c.name.trim().is_empty() {
                return Err(ConfigError::Invalid("category name cannot be empty".into()));
            }
            if !seen.insert(c.name.as_str()) {
                return Err(ConfigError::Invalid(format!(
                    "duplicate category name: {}",
                    c.name
                )));
            }
        }
        for key in self.downloads.methods.keys().chain(
            self.downloads
                .sources
                .values()
                .flat_map(|s| s.methods.keys()),
        ) {
            if DownloadMethod::parse(key).is_none() {
                return Err(ConfigError::Invalid(format!(
                    "unknown download method in config: {key} (expected direct/torrent/magnet)"
                )));
            }
        }
        for rule in &self.rules {
            if rule.match_exact.is_none() && rule.match_regex.is_none() {
                return Err(ConfigError::Invalid(
                    "rule must set `match` or `match_regex`".into(),
                ));
            }
            if let Some(pattern) = &rule.match_regex {
                regex::Regex::new(pattern).map_err(|e| {
                    ConfigError::Invalid(format!("invalid match_regex {pattern:?}: {e}"))
                })?;
            }
            if !seen.contains(rule.category.as_str()) {
                return Err(ConfigError::Invalid(format!(
                    "rule references undefined category: {}",
                    rule.category
                )));
            }
        }
        Ok(())
    }
}

fn expand_method_override(m: &mut MethodOverride) {
    if let Some(dir) = &mut m.dir {
        *dir = expand_path(dir);
    }
    if let Some(watch_dir) = &mut m.watch_dir {
        *watch_dir = expand_path(watch_dir);
    }
}

fn expand_path(path: &Path) -> PathBuf {
    PathBuf::from(shellexpand::tilde(&path.to_string_lossy()).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        Config::default().validate().unwrap();
    }

    #[test]
    fn notifications_defaults_to_open_show_page_enabled() {
        let cfg = Config::default();
        assert!(cfg.notifications.open_show_page);
        assert_eq!(cfg.notifications.open_command, None);
    }

    #[test]
    fn notifications_can_be_configured() {
        let toml = r#"
            [notifications]
            open_show_page = false
            open_command = "firefox {url}"
        "#;
        let cfg = Config::parse(toml, Path::new("t.toml")).unwrap();
        assert!(!cfg.notifications.open_show_page);
        assert_eq!(
            cfg.notifications.open_command.as_deref(),
            Some("firefox {url}")
        );
    }

    #[test]
    fn notifications_sound_defaults_to_none() {
        let cfg = Config::default();
        assert_eq!(cfg.notifications.sound_file, None);
        assert_eq!(cfg.notifications.sound_name, None);
        assert!(cfg.notifications.sources.is_empty());
    }

    #[test]
    fn notifications_sound_can_be_configured_globally_and_per_source() {
        let toml = r#"
            [notifications]
            sound_file = "/usr/share/sounds/default.oga"

            [notifications.sources.subsplease]
            sound_file = "/usr/share/sounds/subsplease.oga"

            [notifications.sources.nyaa]
            sound_name = "message-new-instant"
        "#;
        let cfg = Config::parse(toml, Path::new("t.toml")).unwrap();
        assert_eq!(
            cfg.notifications.sound_file.as_deref(),
            Some(Path::new("/usr/share/sounds/default.oga"))
        );
        assert_eq!(
            cfg.notifications
                .sources
                .get("subsplease")
                .and_then(|o| o.sound_file.as_deref()),
            Some(Path::new("/usr/share/sounds/subsplease.oga"))
        );
        assert_eq!(
            cfg.notifications
                .sources
                .get("nyaa")
                .and_then(|o| o.sound_name.as_deref()),
            Some("message-new-instant")
        );
    }

    #[test]
    fn parses_minimal_toml_with_defaults() {
        let cfg = Config::parse("", Path::new("test.toml")).unwrap();
        assert_eq!(cfg.categories.len(), 3);
        assert_eq!(cfg.downloads.default_resolution, "1080");
    }

    #[test]
    fn rejects_duplicate_category_names() {
        let toml = r#"
            [[categories]]
            name = "liked"
            notify = true
            auto_download = true

            [[categories]]
            name = "liked"
            notify = false
            auto_download = false
        "#;
        let err = Config::parse(toml, Path::new("t.toml")).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid(_)));
    }

    #[test]
    fn rejects_rule_with_undefined_category() {
        let toml = r#"
            [[rules]]
            match = "One Piece"
            category = "does-not-exist"
        "#;
        let err = Config::parse(toml, Path::new("t.toml")).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid(_)));
    }

    #[test]
    fn env_vars_are_expanded() {
        std::env::set_var("ANIME_NOTIF_TEST_TOKEN", "secret123");
        let toml = r#"
            [general.db]
            kind = "remote"
            url = "libsql://example"
            auth_token = "${ANIME_NOTIF_TEST_TOKEN}"
        "#;
        let cfg = Config::parse(toml, Path::new("t.toml")).unwrap();
        match cfg.general.db {
            DbConfig::Remote { auth_token, .. } => {
                assert_eq!(auth_token.as_deref(), Some("secret123"));
            }
            _ => panic!("expected remote db"),
        }
        std::env::remove_var("ANIME_NOTIF_TEST_TOKEN");
    }

    #[test]
    fn unresolved_env_vars_are_left_as_is() {
        let toml = r#"
            [general.db]
            kind = "remote"
            url = "libsql://example"
            auth_token = "${ANIME_NOTIF_DEFINITELY_UNSET}"
        "#;
        let cfg = Config::parse(toml, Path::new("t.toml")).unwrap();
        match cfg.general.db {
            DbConfig::Remote { auth_token, .. } => {
                assert_eq!(
                    auth_token.as_deref(),
                    Some("${ANIME_NOTIF_DEFINITELY_UNSET}")
                );
            }
            _ => panic!("expected remote db"),
        }
    }

    #[test]
    fn tilde_is_expanded_in_paths() {
        let toml = r#"
            [downloads]
            base_dir = "~/Downloads/anime"
        "#;
        let cfg = Config::parse(toml, Path::new("t.toml")).unwrap();
        assert!(!cfg.downloads.base_dir.to_string_lossy().starts_with('~'));
    }

    #[test]
    fn resolve_dir_precedence() {
        let mut d = Downloads {
            base_dir: PathBuf::from("/base"),
            ..Downloads::default()
        };
        // No overrides: base/<source>/<method>
        assert_eq!(
            d.resolve_dir("nyaa", DownloadMethod::Direct),
            PathBuf::from("/base/nyaa/direct")
        );

        // Per-method override applies to all sources.
        d.methods.insert(
            DownloadMethod::Direct.as_str().to_string(),
            MethodOverride {
                dir: Some(PathBuf::from("/per-method")),
                ..Default::default()
            },
        );
        assert_eq!(
            d.resolve_dir("nyaa", DownloadMethod::Direct),
            PathBuf::from("/per-method")
        );

        // Per-source override beats per-method.
        d.sources.insert(
            "nyaa".into(),
            SourceDownloadOverride {
                dir: Some(PathBuf::from("/per-source")),
                methods: HashMap::new(),
            },
        );
        assert_eq!(
            d.resolve_dir("nyaa", DownloadMethod::Direct),
            PathBuf::from("/per-source")
        );

        // Per-source+method beats everything.
        d.sources.get_mut("nyaa").unwrap().methods.insert(
            DownloadMethod::Direct.as_str().to_string(),
            MethodOverride {
                dir: Some(PathBuf::from("/per-source-method")),
                ..Default::default()
            },
        );
        assert_eq!(
            d.resolve_dir("nyaa", DownloadMethod::Direct),
            PathBuf::from("/per-source-method")
        );

        // A different source is unaffected by nyaa's overrides, but still
        // sees the per-method override.
        assert_eq!(
            d.resolve_dir("other", DownloadMethod::Direct),
            PathBuf::from("/per-method")
        );
    }

    #[test]
    fn torrent_prefers_watch_dir_over_dir() {
        let mut d = Downloads {
            base_dir: PathBuf::from("/base"),
            ..Downloads::default()
        };
        d.methods.insert(
            DownloadMethod::Torrent.as_str().to_string(),
            MethodOverride {
                dir: Some(PathBuf::from("/torrent-dir")),
                watch_dir: Some(PathBuf::from("/watch-dir")),
                command: None,
            },
        );
        assert_eq!(
            d.resolve_dir("nyaa", DownloadMethod::Torrent),
            PathBuf::from("/watch-dir")
        );
    }
}
