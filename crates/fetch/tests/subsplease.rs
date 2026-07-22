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

    assert!(
        result.warnings.is_empty(),
        "unexpected extraction warnings: {:?}",
        result.warnings
    );
    assert!(!result.releases.is_empty(), "expected at least one release");

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
