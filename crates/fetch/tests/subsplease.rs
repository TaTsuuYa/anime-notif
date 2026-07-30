//! Validates the shipped `sources/subsplease.toml` example plugin against a
//! captured real API response, and (opt-in) against the live endpoint.

use std::path::{Path, PathBuf};

use anime_notif_core::{extract, DownloadMethod, SourcePlugin};

fn plugin_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sources/subsplease.toml")
}

fn fixture_json() -> serde_json::Value {
    let raw = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/subsplease_latest.json"),
    )
    .expect("fixture file should exist");
    serde_json::from_str(&raw).expect("fixture should be valid JSON")
}

#[test]
fn extracts_captured_subsplease_response() {
    let source = SourcePlugin::load(&plugin_path()).expect("plugin should load and compile");
    let root = fixture_json();
    let result = extract(&source, &root);

    // The captured fixture contains one real batch release (Dr. Stone S3,
    // episode "01-22") among its entries; the shipped plugin's [batch]
    // config skips it by default, which shows up as exactly one warning.
    assert_eq!(
        result.warnings.len(),
        1,
        "expected exactly the one known batch-skip warning, got: {:?}",
        result.warnings
    );
    assert!(result.warnings[0].contains("skipped batch release"));
    assert!(
        !result.releases.iter().any(|r| r.episode == "01-22"),
        "the batch release should have been filtered out"
    );
    assert!(!result.releases.is_empty(), "expected at least one release");

    // The fixture also has a synthetic version bump ("Toukutsu Ou - 03v2",
    // added alongside the real "Toukutsu Ou - 03" entry) exercising the
    // shipped plugin's [version] table end to end.
    let versioned = result
        .releases
        .iter()
        .find(|r| r.series_title == "Toukutsu Ou" && r.version == 2)
        .expect("the 03v2 entry should have been parsed as version 2");
    assert_eq!(
        versioned.episode, "03",
        "episode should be the base, version suffix stripped"
    );
    assert!(
        result
            .releases
            .iter()
            .any(|r| r.series_title == "Toukutsu Ou" && r.episode == "03" && r.version == 1),
        "the original unversioned 03 entry should still be present as version 1"
    );

    for release in &result.releases {
        assert_eq!(release.source_id, "subsplease");
        assert_eq!(release.method, DownloadMethod::Magnet);
        assert!(
            release.link.starts_with("magnet:?xt="),
            "link should be a magnet URI, got {:?}",
            release.link
        );
        assert!(
            !release.resolution.is_empty()
                && release.resolution.chars().all(|c| c.is_ascii_digit()),
            "resolution {:?} should be normalized to bare digits",
            release.resolution
        );
        assert!(!release.series_title.is_empty());
        if let Some(cover) = &release.cover_url {
            assert!(
                cover.starts_with("https://subsplease.org"),
                "cover url should be absolutized, got {cover:?}"
            );
        }
        let show_url = release
            .show_url
            .as_ref()
            .expect("every entry in the fixture has a page slug, so show_url should be set");
        assert!(
            show_url.starts_with("https://subsplease.org/shows/") && show_url.ends_with('/'),
            "show url should be built from the page slug, got {show_url:?}"
        );
        assert_eq!(
            release.source_icon_url.as_deref(),
            Some("https://subsplease.org/favicon.ico")
        );
    }
}

#[tokio::test]
#[ignore = "hits the real network; run explicitly with `cargo test -- --ignored`"]
async fn polls_live_subsplease_api() {
    let source = SourcePlugin::load(&plugin_path()).expect("plugin should load and compile");
    let client = anime_notif_fetch::client();
    let result = anime_notif_fetch::poll(&client, &source)
        .await
        .expect("live poll should succeed");
    assert!(
        !result.releases.is_empty(),
        "expected at least one release from the live API"
    );
}
