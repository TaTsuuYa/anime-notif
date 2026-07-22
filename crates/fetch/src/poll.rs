//! Polls a compiled source's endpoint and extracts its releases.

use anime_notif_core::{extract, CompiledSource, ExtractionResult};
use serde_json::Value;

use crate::error::FetchError;

/// Fetches `source`'s endpoint and extracts every release from the
/// response, using its configured method, headers, query parameters, and
/// body.
pub async fn poll(
    client: &reqwest::Client,
    source: &CompiledSource,
) -> Result<ExtractionResult, FetchError> {
    let value = fetch_json(client, source).await?;
    Ok(extract(source, &value))
}

async fn fetch_json(
    client: &reqwest::Client,
    source: &CompiledSource,
) -> Result<Value, FetchError> {
    let mut builder = match source.method.to_ascii_uppercase().as_str() {
        "POST" => client.post(&source.endpoint),
        _ => client.get(&source.endpoint),
    };
    for (key, value) in &source.headers {
        builder = builder.header(key, value);
    }
    if !source.query.is_empty() {
        builder = builder.query(&source.query);
    }
    if let Some(body) = &source.body {
        builder = builder.body(body.clone());
    }

    let response = builder.send().await?.error_for_status()?;
    let text = response.text().await?;
    serde_json::from_str(&text).map_err(|error| FetchError::InvalidJson {
        source_id: source.id.clone(),
        error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anime_notif_core::SourcePlugin;
    use std::path::Path;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SUBSPLEASE_LIKE_TOML: &str = r#"
id = "subsplease"
endpoint = "PLACEHOLDER"
method = "GET"
query = { f = "latest", tz = "Africa/Casablanca" }
headers = { "X-Test" = "yes" }

items = ".[]"
variants = ".downloads[]"

[fields]
series  = { path = ".show" }
episode = { path = ".episode", default = "?" }

[fields.variant]
resolution = { path = ".res", regex = "(\\d+)", default = "480" }
method     = { default = "magnet" }
link       = { path = ".magnet" }
"#;

    #[tokio::test]
    async fn polls_and_extracts_releases() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/"))
            .and(query_param("f", "latest"))
            .and(query_param("tz", "Africa/Casablanca"))
            .and(header("X-Test", "yes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "x": {
                    "show": "One Piece",
                    "episode": "1121",
                    "downloads": [
                        {"res": "1080p", "magnet": "magnet:?xt=aaa"}
                    ]
                }
            })))
            .mount(&server)
            .await;

        let toml = SUBSPLEASE_LIKE_TOML.replace("PLACEHOLDER", &format!("{}/api/", server.uri()));
        let source = SourcePlugin::parse(&toml, Path::new("t.toml")).unwrap();

        let client = reqwest::Client::new();
        let result = poll(&client, &source).await.unwrap();

        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
        assert_eq!(result.releases.len(), 1);
        assert_eq!(result.releases[0].series_title, "One Piece");
        assert_eq!(result.releases[0].resolution, "1080");
    }

    #[tokio::test]
    async fn http_error_status_is_reported() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let toml = SUBSPLEASE_LIKE_TOML.replace("PLACEHOLDER", &format!("{}/api/", server.uri()));
        let source = SourcePlugin::parse(&toml, Path::new("t.toml")).unwrap();

        let client = reqwest::Client::new();
        let err = poll(&client, &source).await.unwrap_err();
        assert!(matches!(err, FetchError::Http(_)));
    }

    #[tokio::test]
    async fn invalid_json_is_reported() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let toml = SUBSPLEASE_LIKE_TOML.replace("PLACEHOLDER", &format!("{}/api/", server.uri()));
        let source = SourcePlugin::parse(&toml, Path::new("t.toml")).unwrap();

        let client = reqwest::Client::new();
        let err = poll(&client, &source).await.unwrap_err();
        assert!(matches!(err, FetchError::InvalidJson { .. }));
    }
}
