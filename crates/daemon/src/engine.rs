//! Orchestrates the resolution-wait workflow's I/O: turns a batch of freshly
//! extracted [`Release`]s into database updates, notifications, and
//! downloads, using the pure decision logic in [`crate::resolution`].
//!
//! [`Engine::process_poll`] handles one source's poll results;
//! [`Engine::sweep_pending`] re-evaluates every pending episode for the
//! resolution-wait timeout (called after every poll — see its doc comment
//! for why that's safe); [`Engine::handle_action`] executes a
//! download/whitelist/blacklist action, used by both the control server
//! (link fallback) and native notification action callbacks.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anime_notif_core::config::CategoryDef;
use anime_notif_core::{Config, DownloadMethod, Release};
use anime_notif_download::{DownloadRequest, Downloader};
use anime_notif_notify::{Notification, NotificationAction, Notifier, Sound};
use anime_notif_store::{InteractionKind, SeriesRow, Store};

use crate::cover;
use crate::error::EngineError;
use crate::resolution;

/// Bundles everything [`Engine`] needs to process polls and actions.
///
/// Owns its dependencies via `Arc` (rather than borrowing) so an
/// `Arc<Engine>` can be shared, with a `'static` bound, across the tokio
/// scheduler tasks and the axum control server.
pub struct Engine {
    /// Database handle.
    pub store: Arc<Store>,
    /// Loaded config (categories, rules, download settings).
    pub config: Arc<Config>,
    /// Notification backend.
    pub notifier: Arc<dyn Notifier>,
    /// Download backend.
    pub downloader: Arc<dyn Downloader>,
    /// Base URL of the daemon's loopback control server, e.g.
    /// `http://127.0.0.1:4732`.
    pub control_base_url: String,
    /// Shared secret required on every control-server request.
    pub control_token: String,
    /// Per-source `resolution_wait` overrides (from each source plugin's
    /// `resolution_wait`), falling back to
    /// `config.downloads.resolution_wait` when a source has none.
    pub resolution_wait_by_source: HashMap<String, Duration>,
    /// HTTP client used to fetch cover art (separate from the one used to
    /// poll sources, so a slow/broken cover host can't starve polling).
    pub http_client: reqwest::Client,
    /// Where fetched cover art (and the default icon) are cached.
    pub cache_dir: std::path::PathBuf,
    /// Path to the bundled default icon, pre-written to `cache_dir` at
    /// startup; used when a release has no cover or its fetch fails.
    pub default_icon_path: Option<std::path::PathBuf>,
}

impl Engine {
    /// Processes one source's freshly-fetched releases: filters out
    /// already-seen variants, groups the rest by episode, and resolves
    /// each group (notify/download now, or record as pending). Finishes by
    /// sweeping every pending episode for the resolution-wait timeout.
    pub async fn process_poll(&self, releases: Vec<Release>) -> Result<(), EngineError> {
        let mut fresh = Vec::new();
        for release in releases {
            if !self.store.is_seen(&release.dedup_key()).await? {
                fresh.push(release);
            }
        }
        if fresh.is_empty() {
            return self.sweep_pending().await;
        }

        let mut groups: HashMap<String, Vec<Release>> = HashMap::new();
        for release in fresh {
            groups
                .entry(release.episode_key())
                .or_default()
                .push(release);
        }
        for variants in groups.into_values() {
            self.process_episode_group(variants).await?;
        }

        self.sweep_pending().await
    }

    async fn process_episode_group(&self, variants: Vec<Release>) -> Result<(), EngineError> {
        let first = variants[0].clone();
        let category_name = resolution::determine_category(
            &self.config.rules,
            &self.config.categories,
            &first.series_title,
        )
        .to_string();
        let series = self
            .store
            .upsert_series(
                &first.source_id,
                &first.series_title,
                &category_name,
                first.cover_url.as_deref(),
            )
            .await?;

        for variant in &variants {
            self.store
                .mark_seen(
                    &variant.dedup_key(),
                    series.id,
                    &variant.episode,
                    &variant.resolution,
                    variant.method.as_str(),
                    &variant.link,
                )
                .await?;
        }

        let category = self.category_for(&series.category);

        if let Some(chosen) = resolution::find_desired(
            &variants,
            &self.config.downloads.default_resolution,
            self.config.downloads.default_method,
        ) {
            self.resolve_episode(&series, &category, chosen).await?;
            self.store.clear_pending(series.id, &first.episode).await?;
            return Ok(());
        }

        self.update_pending(&series, &first.episode, &variants)
            .await
    }

    async fn update_pending(
        &self,
        series: &SeriesRow,
        episode: &str,
        variants: &[Release],
    ) -> Result<(), EngineError> {
        let fallback = &self.config.downloads.resolution_fallback;
        let batch_best =
            resolution::best_variant(variants, fallback, self.config.downloads.default_method);

        let existing = self.store.get_pending(series.id, episode).await?;
        let should_update = match existing.as_ref().and_then(|p| p.best_resolution.as_deref()) {
            None => true,
            Some(existing_res) => {
                resolution::rank_resolution(&batch_best.resolution, fallback)
                    < resolution::rank_resolution(existing_res, fallback)
            }
        };

        let (best_res, best_json) = if should_update {
            (
                Some(batch_best.resolution.clone()),
                Some(serde_json::to_string(batch_best).expect("Release always serializes")),
            )
        } else {
            (None, None)
        };

        self.store
            .upsert_pending(
                series.id,
                episode,
                &self.config.downloads.default_resolution,
                best_res.as_deref(),
                best_json.as_deref(),
            )
            .await?;
        Ok(())
    }

    /// Re-evaluates every pending episode against its `resolution_wait`
    /// timeout, falling back to the best-so-far variant for any that have
    /// expired. Safe (if slightly redundant) to call after every source's
    /// poll rather than only the owning source's: pending rows for other
    /// sources are cheap to re-check and simply won't have timed out yet.
    pub async fn sweep_pending(&self) -> Result<(), EngineError> {
        let now = chrono::Utc::now();
        for pending in self.store.list_pending().await? {
            let Some(series) = self.store.get_by_id(pending.series_id).await? else {
                continue; // orphaned pending row (series deleted); ignore
            };
            let wait = self
                .resolution_wait_by_source
                .get(&series.source_id)
                .copied()
                .unwrap_or(self.config.downloads.resolution_wait);
            let elapsed = (now - pending.first_seen_at).to_std().unwrap_or_default();
            if elapsed < wait {
                continue;
            }

            let Some(best_json) = &pending.best_variant_json else {
                continue; // shouldn't happen (we always set it when creating), but don't crash on it
            };
            let release: Release =
                serde_json::from_str(best_json).map_err(|source| EngineError::CorruptPending {
                    series_id: series.id,
                    episode: pending.episode.clone(),
                    source,
                })?;
            let category = self.category_for(&series.category);
            self.resolve_episode(&series, &category, &release).await?;
            self.store
                .clear_pending(series.id, &pending.episode)
                .await?;
        }
        Ok(())
    }

    async fn resolve_episode(
        &self,
        series: &SeriesRow,
        category: &CategoryDef,
        chosen: &Release,
    ) -> Result<(), EngineError> {
        self.store
            .set_last_episode(series.id, &chosen.episode)
            .await?;
        if category.auto_download {
            self.trigger_download(series, chosen).await?;
        }
        if category.notify {
            self.send_notification(series, category, chosen).await?;
        }
        Ok(())
    }

    async fn trigger_download(
        &self,
        series: &SeriesRow,
        chosen: &Release,
    ) -> Result<(), EngineError> {
        let dir = self
            .config
            .downloads
            .resolve_dir(&chosen.source_id, chosen.method);
        let command = self
            .config
            .downloads
            .resolve_command(&chosen.source_id, chosen.method);
        let request = DownloadRequest {
            method: chosen.method,
            link: chosen.link.clone(),
            dir,
            command,
            file_stem: format!(
                "{} - {} - {}",
                series.title, chosen.episode, chosen.resolution
            ),
        };
        self.downloader.download(&request).await?;
        self.store
            .log_interaction(
                series.id,
                InteractionKind::Download,
                Some(&chosen.resolution),
            )
            .await?;
        Ok(())
    }

    async fn send_notification(
        &self,
        series: &SeriesRow,
        category: &CategoryDef,
        chosen: &Release,
    ) -> Result<(), EngineError> {
        let mut actions = vec![NotificationAction {
            id: "download".into(),
            label: "Download".into(),
            url: self.action_url("download", series.id, &chosen.episode, None),
        }];
        // Categories that don't auto-download are the "undecided" ones —
        // offer to classify the show, per the notification spec.
        if !category.auto_download {
            actions.push(NotificationAction {
                id: "whitelist".into(),
                label: "Whitelist".into(),
                url: self.action_url("whitelist", series.id, &chosen.episode, None),
            });
            actions.push(NotificationAction {
                id: "blacklist".into(),
                label: "Blacklist".into(),
                url: self.action_url("blacklist", series.id, &chosen.episode, None),
            });
        }
        // "default" is the reserved action id most Linux notification
        // daemons invoke when the notification body itself (not a named
        // button) is clicked — see docs/notifications.md.
        if self.config.notifications.open_show_page {
            if let Some(show_url) = &chosen.show_url {
                actions.push(NotificationAction {
                    id: "default".into(),
                    label: "Open".into(),
                    url: self.action_url("open_show", series.id, &chosen.episode, Some(show_url)),
                });
            }
        }

        // Small app/source badge: the source's own icon (e.g. its
        // favicon) when configured, falling back to our bundled default —
        // never the cover art, which goes in `image_path` instead (see
        // `Notification`'s doc comment for why these are two different
        // things).
        let icon_path = match &chosen.source_icon_url {
            Some(url) => cover::fetch_cover_cached(&self.http_client, &self.cache_dir, url)
                .await
                .or_else(|| self.default_icon_path.clone()),
            None => self.default_icon_path.clone(),
        };
        // Big content image: the show's cover art, shown in the
        // notification body. Left unset (no fallback) if unavailable —
        // there's nothing sensible to substitute for "a picture of this
        // specific show".
        let image_path = match &chosen.cover_url {
            Some(url) => cover::fetch_cover_cached(&self.http_client, &self.cache_dir, url).await,
            None => None,
        };

        let notification = Notification {
            title: series.title.clone(),
            body: format!(
                "Episode {} [{}] via {}",
                chosen.episode, chosen.resolution, chosen.method
            ),
            icon_path,
            image_path,
            sound: self.resolve_sound(&chosen.source_id),
            actions,
        };
        self.notifier.notify(&notification)?;
        Ok(())
    }

    /// Resolves the sound to play for a notification from `source_id`: a
    /// per-source override (`notifications.sources.<id>`) if it sets
    /// either `sound_file`/`sound_name`, otherwise the global
    /// `notifications.sound_file`/`sound_name`, preferring a file over a
    /// themed name at whichever level applies. `None` if nothing is
    /// configured at all (the notification daemon's own default applies).
    fn resolve_sound(&self, source_id: &str) -> Option<Sound> {
        let cfg = &self.config.notifications;
        let over = cfg.sources.get(source_id);
        let has_override = over.is_some_and(|o| o.sound_file.is_some() || o.sound_name.is_some());

        let (sound_file, sound_name) = if has_override {
            let over = over.unwrap();
            (over.sound_file.clone(), over.sound_name.clone())
        } else {
            (cfg.sound_file.clone(), cfg.sound_name.clone())
        };

        sound_file
            .map(Sound::File)
            .or_else(|| sound_name.map(Sound::Name))
    }

    /// Builds an action URL; `target_url` is only used by the `open_show`
    /// kind (the show page to open).
    fn action_url(
        &self,
        kind: &str,
        series_id: i64,
        episode: &str,
        target_url: Option<&str>,
    ) -> String {
        let episode_enc: String =
            url::form_urlencoded::byte_serialize(episode.as_bytes()).collect();
        let mut built = format!(
            "{}/action?token={}&kind={kind}&series_id={series_id}&episode={episode_enc}",
            self.control_base_url, self.control_token
        );
        if let Some(target_url) = target_url {
            let encoded: String =
                url::form_urlencoded::byte_serialize(target_url.as_bytes()).collect();
            built.push_str(&format!("&url={encoded}"));
        }
        built
    }

    fn category_for(&self, name: &str) -> CategoryDef {
        self.config
            .categories
            .iter()
            .find(|c| c.name == name)
            .cloned()
            .unwrap_or(CategoryDef {
                name: name.to_string(),
                notify: true,
                auto_download: false,
            })
    }

    /// Executes a `download`/`whitelist`/`blacklist`/`open_show` action —
    /// the shared handler behind both the control server's HTTP endpoint
    /// and native notification action callbacks. `url` is only meaningful
    /// for `open_show` (the show page to open). Returns a short
    /// human-readable result message, and shows a confirmation
    /// notification for every kind except `open_show` (whose own visible
    /// effect, a browser opening, is confirmation enough).
    pub async fn handle_action(
        &self,
        kind: &str,
        series_id: i64,
        episode: &str,
        url: Option<&str>,
    ) -> Result<String, EngineError> {
        let Some(series) = self.store.get_by_id(series_id).await? else {
            tracing::warn!(kind, series_id, "action for unknown series");
            return Ok(format!("No such show (id {series_id})"));
        };
        tracing::info!(kind, series = %series.title, episode, "handling notification action");

        let result = match kind {
            "download" => self.handle_download_action(&series, episode).await,
            "whitelist" => {
                self.handle_reclassify_action(
                    &series,
                    episode,
                    |c| c.auto_download,
                    "liked",
                    "No auto-download category is defined",
                )
                .await
            }
            "blacklist" => {
                self.handle_reclassify_action(
                    &series,
                    episode,
                    |c| !c.notify && !c.auto_download,
                    "uninterested",
                    "No blacklist-like category is defined",
                )
                .await
            }
            "open_show" => self.handle_open_show_action(url).await,
            other => Ok(format!("Unknown action {other:?}")),
        };

        match &result {
            Ok(message) => tracing::info!(kind, series = %series.title, %message, "action handled"),
            Err(err) => tracing::error!(kind, series = %series.title, %err, "action failed"),
        }

        if kind != "open_show" {
            if let Ok(message) = &result {
                self.send_confirmation(&series.title, message).await;
            }
        }

        result
    }

    /// Reconstructs every known variant of `(series, episode)` from the
    /// `seen` log and picks the favourite one (desired resolution/method,
    /// falling back like the automatic resolve path does). Shared by the
    /// manual "Download" action and by whitelisting triggering an
    /// immediate download of the episode that was being notified about.
    async fn pick_best_available(
        &self,
        series: &SeriesRow,
        episode: &str,
    ) -> Result<Option<Release>, EngineError> {
        let seen = self.store.list_seen_for_episode(series.id, episode).await?;
        if seen.is_empty() {
            return Ok(None);
        }
        let releases: Vec<Release> = seen
            .iter()
            .map(|s| Release {
                source_id: series.source_id.clone(),
                series_title: series.title.clone(),
                episode: s.episode.clone(),
                season: None,
                resolution: s.resolution.clone(),
                method: DownloadMethod::parse(&s.method).unwrap_or(DownloadMethod::Direct),
                link: s.link.clone(),
                cover_url: None,
                source_icon_url: None,
                show_url: None,
                raw_id: None,
            })
            .collect();
        let chosen = resolution::find_desired(
            &releases,
            &self.config.downloads.default_resolution,
            self.config.downloads.default_method,
        )
        .unwrap_or_else(|| {
            resolution::best_variant(
                &releases,
                &self.config.downloads.resolution_fallback,
                self.config.downloads.default_method,
            )
        })
        .clone();
        Ok(Some(chosen))
    }

    async fn handle_download_action(
        &self,
        series: &SeriesRow,
        episode: &str,
    ) -> Result<String, EngineError> {
        match self.pick_best_available(series, episode).await? {
            Some(chosen) => {
                self.trigger_download(series, &chosen).await?;
                Ok(format!(
                    "Downloading {:?} episode {} [{}]",
                    series.title, chosen.episode, chosen.resolution
                ))
            }
            None => Ok(format!(
                "No known download for {:?} episode {episode}",
                series.title
            )),
        }
    }

    /// Reclassifies `series` to whichever category matches `matches`
    /// (preferring one literally named `preferred_name`), and, if that
    /// category auto-downloads, immediately downloads `episode` too —
    /// whitelisting a show should act on the episode you were just
    /// notified about, not just change behavior for future ones.
    async fn handle_reclassify_action(
        &self,
        series: &SeriesRow,
        episode: &str,
        matches: impl Fn(&CategoryDef) -> bool,
        preferred_name: &str,
        none_found_message: &str,
    ) -> Result<String, EngineError> {
        let target = self
            .config
            .categories
            .iter()
            .find(|c| c.name == preferred_name)
            .or_else(|| self.config.categories.iter().find(|c| matches(c)))
            .cloned();
        let Some(target) = target else {
            return Ok(none_found_message.to_string());
        };
        self.store.set_category(series.id, &target.name).await?;

        if !target.auto_download {
            return Ok(format!(
                "{:?} set to category {:?}",
                series.title, target.name
            ));
        }

        match self.pick_best_available(series, episode).await? {
            Some(chosen) => {
                self.trigger_download(series, &chosen).await?;
                Ok(format!(
                    "{:?} set to category {:?}; downloading episode {} [{}]",
                    series.title, target.name, chosen.episode, chosen.resolution
                ))
            }
            None => Ok(format!(
                "{:?} set to category {:?} (no known download yet for episode {episode})",
                series.title, target.name
            )),
        }
    }

    async fn handle_open_show_action(&self, url: Option<&str>) -> Result<String, EngineError> {
        let Some(url) = url else {
            return Ok("No show URL was provided".to_string());
        };
        anime_notif_download::open_url(url, self.config.notifications.open_command.as_deref())?;
        Ok(format!("Opened {url}"))
    }

    /// Shows a plain, action-less notification confirming the result of a
    /// download/whitelist/blacklist click — headless action clicks (a
    /// background HTTP request, not a browser tab) otherwise give the user
    /// no feedback at all that anything happened.
    async fn send_confirmation(&self, title: &str, message: &str) {
        let notification = Notification {
            title: title.to_string(),
            body: message.to_string(),
            icon_path: self.default_icon_path.clone(),
            image_path: None,
            sound: None,
            actions: Vec::new(),
        };
        if let Err(err) = self.notifier.notify(&notification) {
            tracing::warn!(%err, "failed to show confirmation notification");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anime_notif_notify::NullNotifier;
    use std::sync::Mutex;
    use std::time::Duration;

    #[derive(Default)]
    struct FakeDownloader {
        requests: Mutex<Vec<DownloadRequest>>,
    }

    #[async_trait::async_trait]
    impl Downloader for FakeDownloader {
        async fn download(
            &self,
            request: &DownloadRequest,
        ) -> Result<std::path::PathBuf, anime_notif_download::DownloadError> {
            self.requests.lock().unwrap().push(request.clone());
            Ok(request.dir.clone())
        }
    }

    fn release(series: &str, episode: &str, resolution: &str, method: DownloadMethod) -> Release {
        Release {
            source_id: "subsplease".into(),
            series_title: series.into(),
            episode: episode.into(),
            season: None,
            resolution: resolution.into(),
            method,
            link: format!("magnet:?xt={series}-{episode}-{resolution}"),
            cover_url: None,
            source_icon_url: None,
            show_url: None,
            raw_id: None,
        }
    }

    fn release_with_cover(
        series: &str,
        episode: &str,
        resolution: &str,
        method: DownloadMethod,
        cover_url: &str,
    ) -> Release {
        Release {
            cover_url: Some(cover_url.to_string()),
            ..release(series, episode, resolution, method)
        }
    }

    fn release_with_source_icon(
        series: &str,
        episode: &str,
        resolution: &str,
        method: DownloadMethod,
        source_icon_url: &str,
    ) -> Release {
        Release {
            source_icon_url: Some(source_icon_url.to_string()),
            ..release(series, episode, resolution, method)
        }
    }

    async fn make_engine(config: Config) -> (Engine, Arc<NullNotifier>, Arc<FakeDownloader>) {
        let store = Arc::new(Store::open_in_memory().await.unwrap());
        let notifier = Arc::new(NullNotifier::new());
        let downloader = Arc::new(FakeDownloader::default());
        let engine = Engine {
            store,
            config: Arc::new(config),
            notifier: notifier.clone(),
            downloader: downloader.clone(),
            control_base_url: "http://127.0.0.1:1".into(),
            control_token: "test-token".into(),
            resolution_wait_by_source: HashMap::new(),
            http_client: reqwest::Client::new(),
            cache_dir: std::env::temp_dir(),
            default_icon_path: None,
        };
        (engine, notifier, downloader)
    }

    #[tokio::test]
    async fn desired_resolution_present_notifies_immediately_no_pending() {
        let (engine, notifier, _downloader) = make_engine(Config::default()).await;
        let variants = vec![
            release("One Piece", "1121", "1080", DownloadMethod::Magnet),
            release("One Piece", "1121", "720", DownloadMethod::Magnet),
        ];
        engine.process_poll(variants).await.unwrap();

        assert_eq!(notifier.sent.lock().unwrap().len(), 1);
        assert_eq!(engine.store.list_pending().await.unwrap().len(), 0);
        let series = engine.store.list_all().await.unwrap();
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].last_episode.as_deref(), Some("1121"));
    }

    #[tokio::test]
    async fn liked_category_auto_downloads() {
        let mut config = Config::default();
        config.rules.push(anime_notif_core::config::Rule {
            match_exact: Some("One Piece".into()),
            match_regex: None,
            category: "liked".into(),
        });
        let (engine, notifier, downloader) = make_engine(config).await;

        let variants = vec![release("One Piece", "1121", "1080", DownloadMethod::Magnet)];
        engine.process_poll(variants).await.unwrap();

        assert_eq!(downloader.requests.lock().unwrap().len(), 1);
        // liked still notifies (notify=true for the default liked category).
        assert_eq!(notifier.sent.lock().unwrap().len(), 1);
        // auto_download categories don't get whitelist/blacklist actions.
        let actions = &notifier.sent.lock().unwrap()[0].actions;
        assert!(actions.iter().any(|a| a.id == "download"));
        assert!(!actions.iter().any(|a| a.id == "whitelist"));
    }

    #[tokio::test]
    async fn uninterested_category_neither_notifies_nor_downloads() {
        let mut config = Config::default();
        config.rules.push(anime_notif_core::config::Rule {
            match_exact: Some("One Piece".into()),
            match_regex: None,
            category: "uninterested".into(),
        });
        let (engine, notifier, downloader) = make_engine(config).await;

        let variants = vec![release("One Piece", "1121", "1080", DownloadMethod::Magnet)];
        engine.process_poll(variants).await.unwrap();

        assert!(notifier.sent.lock().unwrap().is_empty());
        assert!(downloader.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_desired_resolution_creates_pending_without_notifying() {
        let (engine, notifier, _downloader) = make_engine(Config::default()).await;
        // default_resolution is "1080"; only 480p is available.
        let variants = vec![release("One Piece", "1121", "480", DownloadMethod::Magnet)];
        engine.process_poll(variants).await.unwrap();

        assert!(notifier.sent.lock().unwrap().is_empty());
        let pending = engine.store.list_pending().await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].best_resolution.as_deref(), Some("480"));
    }

    #[tokio::test]
    async fn desired_resolution_arriving_later_clears_pending_and_notifies() {
        let (engine, notifier, _downloader) = make_engine(Config::default()).await;
        engine
            .process_poll(vec![release(
                "One Piece",
                "1121",
                "480",
                DownloadMethod::Magnet,
            )])
            .await
            .unwrap();
        assert_eq!(engine.store.list_pending().await.unwrap().len(), 1);

        // Next poll: the same 480p variant re-appears (already seen, ignored)
        // alongside the desired 1080p, which should resolve the episode.
        engine
            .process_poll(vec![
                release("One Piece", "1121", "480", DownloadMethod::Magnet),
                release("One Piece", "1121", "1080", DownloadMethod::Magnet),
            ])
            .await
            .unwrap();

        assert_eq!(engine.store.list_pending().await.unwrap().len(), 0);
        assert_eq!(notifier.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn resolution_wait_timeout_falls_back_to_best_available() {
        let mut config = Config::default();
        config.downloads.resolution_wait = Duration::from_millis(20);
        let (engine, notifier, _downloader) = make_engine(config).await;

        engine
            .process_poll(vec![release(
                "One Piece",
                "1121",
                "480",
                DownloadMethod::Magnet,
            )])
            .await
            .unwrap();
        assert!(notifier.sent.lock().unwrap().is_empty());

        tokio::time::sleep(Duration::from_millis(50)).await;
        engine.sweep_pending().await.unwrap();

        assert_eq!(notifier.sent.lock().unwrap().len(), 1);
        assert_eq!(engine.store.list_pending().await.unwrap().len(), 0);
        let series = engine.store.list_all().await.unwrap();
        assert_eq!(series[0].last_episode.as_deref(), Some("1121"));
    }

    #[tokio::test]
    async fn pending_upgrades_to_better_resolution_without_resolving() {
        let (engine, notifier, _downloader) = make_engine(Config::default()).await;
        engine
            .process_poll(vec![release(
                "One Piece",
                "1121",
                "480",
                DownloadMethod::Magnet,
            )])
            .await
            .unwrap();
        engine
            .process_poll(vec![release(
                "One Piece",
                "1121",
                "720",
                DownloadMethod::Magnet,
            )])
            .await
            .unwrap();

        assert!(notifier.sent.lock().unwrap().is_empty());
        let pending = engine.store.list_pending().await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].best_resolution.as_deref(), Some("720"));
    }

    #[tokio::test]
    async fn handle_action_download_uses_desired_resolution_when_available() {
        let (engine, _notifier, downloader) = make_engine(Config::default()).await;
        engine
            .process_poll(vec![
                release("One Piece", "1121", "480", DownloadMethod::Magnet),
                release("One Piece", "1121", "1080", DownloadMethod::Magnet),
            ])
            .await
            .unwrap();
        // The automatic resolve already downloads nothing (normal category
        // doesn't auto-download); clear recorded requests before the manual
        // action so we can assert on it in isolation.
        downloader.requests.lock().unwrap().clear();

        let series = &engine.store.list_all().await.unwrap()[0];
        let msg = engine
            .handle_action("download", series.id, "1121", None)
            .await
            .unwrap();
        assert!(msg.contains("1080"));
        assert_eq!(downloader.requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn handle_action_whitelist_and_blacklist_change_category() {
        let (engine, _notifier, _downloader) = make_engine(Config::default()).await;
        engine
            .process_poll(vec![release("Naruto", "1", "480", DownloadMethod::Magnet)])
            .await
            .unwrap();
        let series = &engine.store.list_all().await.unwrap()[0];
        assert_eq!(series.category, "normal");

        engine
            .handle_action("whitelist", series.id, "1", None)
            .await
            .unwrap();
        let updated = engine.store.get_by_id(series.id).await.unwrap().unwrap();
        assert_eq!(updated.category, "liked");

        engine
            .handle_action("blacklist", series.id, "1", None)
            .await
            .unwrap();
        let updated = engine.store.get_by_id(series.id).await.unwrap().unwrap();
        assert_eq!(updated.category, "uninterested");
    }

    #[tokio::test]
    async fn whitelist_triggers_immediate_download_of_current_episode() {
        let (engine, _notifier, downloader) = make_engine(Config::default()).await;
        engine
            .process_poll(vec![release("Naruto", "1", "1080", DownloadMethod::Magnet)])
            .await
            .unwrap();
        let series = &engine.store.list_all().await.unwrap()[0];
        assert!(downloader.requests.lock().unwrap().is_empty());

        let msg = engine
            .handle_action("whitelist", series.id, "1", None)
            .await
            .unwrap();

        assert_eq!(downloader.requests.lock().unwrap().len(), 1);
        assert!(msg.contains("liked"));
        assert!(msg.contains("downloading"));
    }

    #[tokio::test]
    async fn blacklist_does_not_trigger_a_download() {
        let (engine, _notifier, downloader) = make_engine(Config::default()).await;
        engine
            .process_poll(vec![release("Naruto", "1", "1080", DownloadMethod::Magnet)])
            .await
            .unwrap();

        let series = &engine.store.list_all().await.unwrap()[0];
        engine
            .handle_action("blacklist", series.id, "1", None)
            .await
            .unwrap();

        assert!(downloader.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn handle_action_unknown_series_is_reported_not_erred() {
        let (engine, _notifier, _downloader) = make_engine(Config::default()).await;
        let msg = engine
            .handle_action("download", 9999, "1", None)
            .await
            .unwrap();
        assert!(msg.contains("No such show"));
    }

    #[tokio::test]
    async fn open_show_action_runs_configured_command() {
        let (mut engine, _notifier, _downloader) = make_engine(Config::default()).await;
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("opened");
        engine.config = Arc::new({
            let mut cfg = Config::default();
            cfg.notifications.open_command = Some(format!("touch {}", marker.display()));
            cfg
        });
        engine
            .process_poll(vec![release("Naruto", "1", "1080", DownloadMethod::Magnet)])
            .await
            .unwrap();
        let series = &engine.store.list_all().await.unwrap()[0];

        let msg = engine
            .handle_action(
                "open_show",
                series.id,
                "1",
                Some("https://subsplease.org/shows/naruto/"),
            )
            .await
            .unwrap();
        assert!(msg.contains("Opened"));

        for _ in 0..50 {
            if marker.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(marker.exists());
    }

    #[tokio::test]
    async fn open_show_action_without_url_is_reported_not_erred() {
        let (engine, _notifier, _downloader) = make_engine(Config::default()).await;
        engine
            .process_poll(vec![release("Naruto", "1", "1080", DownloadMethod::Magnet)])
            .await
            .unwrap();
        let series = &engine.store.list_all().await.unwrap()[0];

        let msg = engine
            .handle_action("open_show", series.id, "1", None)
            .await
            .unwrap();
        assert!(msg.contains("No show URL"));
    }

    #[tokio::test]
    async fn notification_includes_default_action_when_show_url_present_and_enabled() {
        let (engine, notifier, _downloader) = make_engine(Config::default()).await;
        let release_with_url = Release {
            show_url: Some("https://subsplease.org/shows/naruto/".into()),
            ..release("Naruto", "1", "1080", DownloadMethod::Magnet)
        };
        engine.process_poll(vec![release_with_url]).await.unwrap();

        let sent = notifier.sent.lock().unwrap();
        let default_action = sent[0].actions.iter().find(|a| a.id == "default");
        assert!(default_action.is_some());
        assert!(default_action.unwrap().url.contains("kind=open_show"));
    }

    #[tokio::test]
    async fn notification_omits_default_action_when_open_show_page_disabled() {
        let mut config = Config::default();
        config.notifications.open_show_page = false;
        let (engine, notifier, _downloader) = make_engine(config).await;
        let release_with_url = Release {
            show_url: Some("https://subsplease.org/shows/naruto/".into()),
            ..release("Naruto", "1", "1080", DownloadMethod::Magnet)
        };
        engine.process_poll(vec![release_with_url]).await.unwrap();

        let sent = notifier.sent.lock().unwrap();
        assert!(!sent[0].actions.iter().any(|a| a.id == "default"));
    }

    #[tokio::test]
    async fn notification_omits_default_action_when_source_has_no_show_url() {
        let (engine, notifier, _downloader) = make_engine(Config::default()).await;
        engine
            .process_poll(vec![release("Naruto", "1", "1080", DownloadMethod::Magnet)])
            .await
            .unwrap();

        let sent = notifier.sent.lock().unwrap();
        assert!(!sent[0].actions.iter().any(|a| a.id == "default"));
    }

    #[tokio::test]
    async fn actions_other_than_open_show_send_a_confirmation_notification() {
        let (engine, notifier, _downloader) = make_engine(Config::default()).await;
        engine
            .process_poll(vec![release("Naruto", "1", "1080", DownloadMethod::Magnet)])
            .await
            .unwrap();
        let series = &engine.store.list_all().await.unwrap()[0];
        notifier.sent.lock().unwrap().clear();

        engine
            .handle_action("download", series.id, "1", None)
            .await
            .unwrap();

        // The original episode notification plus a confirmation = at least
        // one extra notification beyond whatever else was already sent.
        assert_eq!(notifier.sent.lock().unwrap().len(), 1);
        assert!(notifier.sent.lock().unwrap()[0].actions.is_empty());
    }

    #[tokio::test]
    async fn cover_art_becomes_the_big_image_not_the_small_icon() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cover.jpg"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"cover-bytes".to_vec()))
            .mount(&server)
            .await;

        let (mut engine, notifier, _downloader) = make_engine(Config::default()).await;
        let cover_cache_dir = tempfile::tempdir().unwrap();
        engine.cache_dir = cover_cache_dir.path().to_path_buf();

        let cover_url = format!("{}/cover.jpg", server.uri());
        engine
            .process_poll(vec![release_with_cover(
                "One Piece",
                "1121",
                "1080",
                DownloadMethod::Magnet,
                &cover_url,
            )])
            .await
            .unwrap();

        let sent = notifier.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        // Cover art is the big content image...
        let image = sent[0].image_path.as_ref().expect("expected an image path");
        assert_eq!(std::fs::read(image).unwrap(), b"cover-bytes");
        // ...and never the small app/source icon (no source_icon_url and
        // no default_icon_path configured in this test, so it's None).
        assert_eq!(sent[0].icon_path, None);
    }

    #[tokio::test]
    async fn notification_falls_back_to_default_icon_when_no_source_icon() {
        let (mut engine, notifier, _downloader) = make_engine(Config::default()).await;
        let icon_dir = tempfile::tempdir().unwrap();
        let default_icon = crate::cover::ensure_default_icon(icon_dir.path()).unwrap();
        engine.default_icon_path = Some(default_icon.clone());

        engine
            .process_poll(vec![release(
                "One Piece",
                "1121",
                "1080",
                DownloadMethod::Magnet,
            )])
            .await
            .unwrap();

        let sent = notifier.sent.lock().unwrap();
        assert_eq!(sent[0].icon_path, Some(default_icon));
        assert_eq!(sent[0].image_path, None);
    }

    #[tokio::test]
    async fn notification_uses_fetched_source_icon_as_the_small_badge() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/favicon.ico"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"icon-bytes".to_vec()))
            .mount(&server)
            .await;

        let (mut engine, notifier, _downloader) = make_engine(Config::default()).await;
        let cache_dir = tempfile::tempdir().unwrap();
        engine.cache_dir = cache_dir.path().to_path_buf();

        let icon_url = format!("{}/favicon.ico", server.uri());
        engine
            .process_poll(vec![release_with_source_icon(
                "One Piece",
                "1121",
                "1080",
                DownloadMethod::Magnet,
                &icon_url,
            )])
            .await
            .unwrap();

        let sent = notifier.sent.lock().unwrap();
        let icon = sent[0].icon_path.as_ref().expect("expected an icon path");
        assert_eq!(std::fs::read(icon).unwrap(), b"icon-bytes");
        assert_eq!(sent[0].image_path, None);
    }

    #[tokio::test]
    async fn resolve_sound_prefers_source_override_over_global() {
        let mut config = Config::default();
        config.notifications.sound_file = Some(std::path::PathBuf::from("/global.oga"));
        config.notifications.sources.insert(
            "subsplease".into(),
            anime_notif_core::config::SourceNotificationOverride {
                sound_file: Some(std::path::PathBuf::from("/subsplease.oga")),
                sound_name: None,
            },
        );
        let (engine, _notifier, _downloader) = make_engine(config).await;

        assert_eq!(
            engine.resolve_sound("subsplease"),
            Some(Sound::File("/subsplease.oga".into()))
        );
        // A source with no override at all falls back to the global default.
        assert_eq!(
            engine.resolve_sound("other-source"),
            Some(Sound::File("/global.oga".into()))
        );
    }

    #[tokio::test]
    async fn resolve_sound_name_used_when_no_file_configured() {
        let mut config = Config::default();
        config.notifications.sound_name = Some("message-new-instant".into());
        let (engine, _notifier, _downloader) = make_engine(config).await;
        assert_eq!(
            engine.resolve_sound("anything"),
            Some(Sound::Name("message-new-instant".into()))
        );
    }

    #[tokio::test]
    async fn resolve_sound_is_none_when_nothing_configured() {
        let (engine, _notifier, _downloader) = make_engine(Config::default()).await;
        assert_eq!(engine.resolve_sound("subsplease"), None);
    }

    #[tokio::test]
    async fn notification_carries_resolved_sound() {
        let mut config = Config::default();
        config.notifications.sound_file = Some(std::path::PathBuf::from("/global.oga"));
        let (engine, notifier, _downloader) = make_engine(config).await;

        engine
            .process_poll(vec![release(
                "One Piece",
                "1121",
                "1080",
                DownloadMethod::Magnet,
            )])
            .await
            .unwrap();

        let sent = notifier.sent.lock().unwrap();
        assert_eq!(sent[0].sound, Some(Sound::File("/global.oga".into())));
    }
}
