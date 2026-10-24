#![expect(
    clippy::unwrap_used,
    reason = "Protocol fixtures and asserted decode results must fail the test on error"
)]

use scry_addons::{Addon, Catalog, Stream, manifest_url, resource_url};
use serde_json::json;

#[test]
fn catalog_paging_preserves_configured_paths_and_requires_provider_support() {
    let addon: Addon = serde_json::from_value(json!({
        "manifest_url":"https://example.com/config/private/manifest.json",
        "manifest":{"id":"test", "name":"Test", "catalogs":[]}
    }))
    .unwrap();
    let mut catalog: Catalog =
        serde_json::from_value(json!({"id":"popular", "type":"movie", "extra":[{"name":"skip"}]}))
            .unwrap();
    assert_eq!(
        scry_addons::catalog_page_url(&addon, &catalog, 0)
            .unwrap()
            .path(),
        "/config/private/catalog/movie/popular.json"
    );
    assert_eq!(
        scry_addons::catalog_page_url(&addon, &catalog, 100)
            .unwrap()
            .path(),
        "/config/private/catalog/movie/popular/skip=100.json"
    );
    catalog.extra[0].is_required = true;
    assert_eq!(
        scry_addons::catalog_page_url(&addon, &catalog, 0)
            .unwrap()
            .path(),
        "/config/private/catalog/movie/popular/skip=0.json"
    );
    catalog.extra.clear();
    assert!(scry_addons::catalog_page_url(&addon, &catalog, 100).is_err());
}

#[test]
fn title_logo_is_optional_and_preserved_with_the_backdrop() {
    let meta: scry_addons::Meta = serde_json::from_value(json!({
        "id":"tt123", "type":"movie", "name":"A movie",
        "logo":"https://images.example/logo.png",
        "background":"https://images.example/background.jpg"
    }))
    .unwrap();
    assert_eq!(
        meta.logo.as_deref(),
        Some("https://images.example/logo.png")
    );
    assert_eq!(
        meta.background.as_deref(),
        Some("https://images.example/background.jpg")
    );
    let missing: scry_addons::Meta = serde_json::from_value(json!({
        "id":"tt123", "type":"movie", "name":"A movie"
    }))
    .unwrap();
    assert!(missing.logo.is_none());
}

#[test]
fn cinemeta_can_send_both_year_and_release_info() {
    let meta: scry_addons::Meta = serde_json::from_value(json!({
        "id":"tt123", "type":"movie", "name":"A movie", "year":"2026",
        "releaseInfo":"2026–", "poster":"https://images.example/poster.jpg"
    }))
    .unwrap();
    assert_eq!(meta.year, "2026–");
    assert_eq!(
        meta.poster.as_deref(),
        Some("https://images.example/poster.jpg")
    );
}

#[test]
fn poster_and_release_metadata_are_preserved_for_movie_cards() {
    let meta: scry_addons::Meta = serde_json::from_value(json!({
        "id":"tt123", "type":"movie", "name":"A movie",
        "poster":"https://images.example/poster.jpg", "releaseInfo":"2024–2026",
        "genres":["Drama","Mystery"]
    }))
    .unwrap();
    assert_eq!(
        meta.poster.as_deref(),
        Some("https://images.example/poster.jpg")
    );
    assert_eq!(meta.year, "2024–2026");
    assert_eq!(meta.genres, ["Drama", "Mystery"]);
    let numeric: scry_addons::Meta = serde_json::from_value(json!({
        "id":"tt123", "type":"movie", "name":"A movie", "year":2024,"genres":null
    }))
    .unwrap();
    assert_eq!(numeric.year, "2024");
    assert!(numeric.genres.is_empty());
}

#[test]
fn absent_optional_video_labels_do_not_hide_an_entire_series() {
    let meta: scry_addons::Meta = serde_json::from_value(json!({
        "id":"tt123", "type":"series", "name":"A series", "description":null,
        "videos":[{"id":"tt123:1:1", "title":null, "season":1, "episode":1}]
    }))
    .unwrap();
    assert_eq!(meta.videos.len(), 1);
    assert_eq!(meta.videos[0].id, "tt123:1:1");
    assert!(meta.videos[0].title.is_empty());
    assert!(meta.description.is_empty());
}

fn addon(resources: serde_json::Value) -> Addon {
    let mut value = json!({
        "manifest_url": "https://addon.example/private-token/manifest.json",
        "manifest": {"id": "example", "name": "Example", "types": ["movie"],
            "idPrefixes": ["tt"],
            "catalogs": [{"type": "series", "id": "featured"}]}
    });
    value["manifest"]["resources"] = resources;
    serde_json::from_value(value).unwrap()
}

#[test]
fn required_extra_uses_protocol_casing() {
    let catalog: Catalog = serde_json::from_value(json!({"id":"search", "type":"movie",
        "extra":[{"name":"search", "isRequired":true}]}))
    .unwrap();
    assert!(catalog.extra[0].is_required);
    assert_eq!(
        serde_json::to_value(&catalog.extra[0]).unwrap()["isRequired"],
        true
    );
}

#[test]
fn simple_resources_use_global_filters() {
    let addon = addon(json!(["stream", "meta"]));
    assert!(addon.supports("stream", "movie", "tt123"));
    assert!(!addon.supports("stream", "series", "tt123"));
    assert!(!addon.supports("meta", "movie", "other123"));
    assert!(!addon.supports("subtitles", "movie", "tt123"));
}

#[test]
fn object_resources_replace_global_filters() {
    let addon = addon(json!([{"name":"stream", "types":["series"]}]));
    assert!(addon.supports("stream", "series", "custom:1:2"));
    assert!(!addon.supports("stream", "movie", "tt123"));
    let addon = addon_with_empty_filters();
    assert!(!addon.supports("stream", "movie", "tt123"));
}

fn addon_with_empty_filters() -> Addon {
    addon(json!([{"name":"stream", "types":["movie"], "idPrefixes":[]}]))
}

#[test]
fn catalogs_match_declared_type_and_id_not_content_prefixes() {
    let addon = addon(json!(["catalog"]));
    assert!(addon.supports("catalog", "series", "featured"));
    assert!(!addon.supports("catalog", "movie", "featured"));
    assert!(!addon.supports("catalog", "series", "unknown"));
}

#[test]
fn search_is_a_query_encoded_value_inside_a_path() {
    let url = resource_url(
        "https://addon.example/private%2Ftoken/manifest.json",
        "catalog",
        "movie",
        "top",
        Some("a/b&skip=100+é%?#"),
    )
    .unwrap();
    assert_eq!(
        url.as_str(),
        "https://addon.example/private%2Ftoken/catalog/movie/top/search=a%2Fb%26skip%3D100%2B%C3%A9%25%3F%23.json"
    );
    assert!(url.query().is_none());
    assert!(url.fragment().is_none());
}

#[test]
fn invalid_manifest_errors_do_not_disclose_input() {
    for input in [
        "https://user:secret@addon.example/token/manifest.json",
        "https://addon.example/token/manifest.json?key=secret",
        "ftp://addon.example/token/manifest.json",
        "not a url secret",
    ] {
        let error = format!("{:#}", manifest_url(input).unwrap_err());
        assert!(!error.contains("secret"));
        assert!(!error.contains("token"));
        assert!(!error.contains("addon.example"));
    }
}

#[test]
fn catalog_required_extras_are_checked_before_requesting() {
    let search: Catalog = serde_json::from_value(json!({"id":"search", "type":"movie",
        "extra":[{"name":"search", "isRequired":true}]}))
    .unwrap();
    assert!(search.validate_search(None).is_err());
    assert!(search.validate_search(Some("")).is_err());
    assert!(search.validate_search(Some("public domain")).is_ok());
    let genre: Catalog = serde_json::from_value(json!({"id":"genre", "type":"movie",
        "extra":[{"name":"genre", "isRequired":true}, {"name":"search"}]}))
    .unwrap();
    assert!(genre.validate_search(Some("public domain")).is_err());
    let feed: Catalog = serde_json::from_value(json!({"id":"feed", "type":"movie"})).unwrap();
    assert!(feed.validate_search(None).is_ok());
    assert!(feed.validate_search(Some("public domain")).is_err());
}

#[test]
fn streams_requiring_headers_are_explicitly_unsupported() {
    let stream: Stream = serde_json::from_value(json!({"url":"https://media.example/private-token/video.mp4",
        "behaviorHints":{"notWebReady":true, "proxyHeaders":{"request":{"Authorization":"secret"}}}})).unwrap();
    let error = stream.validate_playback().unwrap_err().to_string();
    assert!(error.contains("proxy headers"));
    assert!(!error.contains("secret"));
    assert!(!error.contains("private-token"));
}

#[test]
fn direct_and_torrent_streams_keep_protocol_fields() {
    let direct: Stream = serde_json::from_value(
        json!({"description":"1080p", "url":"https://media.example/video.mkv",
        "behaviorHints":{"notWebReady":true}}),
    )
    .unwrap();
    assert_eq!(direct.description, "1080p");
    assert!(direct.validate_playback().is_ok());
    let torrent: Stream = serde_json::from_value(
        json!({"infoHash":"0123456789abcdef0123456789abcdef01234567", "fileIdx":null,
        "sources":["tracker:udp://tracker.example:80"]}),
    )
    .unwrap();
    assert!(torrent.validate_playback().is_ok());
    assert_eq!(torrent.file_idx, None);
    assert_eq!(torrent.sources, ["tracker:udp://tracker.example:80"]);
}

#[test]
fn invalid_or_external_only_streams_are_not_sent_to_player() {
    for value in [
        json!({"url":"file:///private-token"}),
        json!({"url":"https://user:secret@media.example/video"}),
        json!({"url":"invalid private-token"}),
        json!({"infoHash":"invalid private-token"}),
        json!({"ytId":"example"}),
        json!({"externalUrl":"https://example.com/private-token"}),
    ] {
        let stream: Stream = serde_json::from_value(value).unwrap();
        let error = format!("{:#}", stream.validate_playback().unwrap_err());
        assert!(!error.contains("private-token"));
        assert!(!error.contains("secret"));
    }
}

#[test]
fn resource_segments_cannot_disappear_during_url_construction() {
    for (resource, kind, id) in [
        ("", "movie", "top"),
        ("catalog", "..", "top"),
        ("catalog", "movie", "."),
    ] {
        assert!(
            resource_url(
                "https://addon.example/manifest.json",
                resource,
                kind,
                id,
                Some("example")
            )
            .is_err()
        );
    }
}

#[test]
fn stremio_install_scheme_keeps_configured_path_and_search_spaces() {
    let base = manifest_url("stremio://addon.example/private%2Ftoken").unwrap();
    assert_eq!(
        base.as_str(),
        "https://addon.example/private%2Ftoken/manifest.json"
    );
    let url = resource_url(
        base.as_str(),
        "catalog",
        "movie",
        "top",
        Some("public domain"),
    )
    .unwrap();
    assert!(url.as_str().ends_with("/search=public%20domain.json"));
}

#[test]
fn episode_names_accept_cinemeta_name_and_prefer_nonempty_title() {
    for (fields, expected) in [
        (serde_json::json!({"name":"First Period"}), "First Period"),
        (
            serde_json::json!({"title":"Provider title","name":"Other name"}),
            "Provider title",
        ),
        (
            serde_json::json!({"title":"  ","name":"Half Loop"}),
            "Half Loop",
        ),
        (serde_json::json!({"title":null,"name":"Pilot"}), "Pilot"),
        (serde_json::json!({}), ""),
    ] {
        let mut data = fields;
        data["id"] = serde_json::json!("show:1:1");
        let video: scry_addons::Video = serde_json::from_value(data).unwrap();
        assert_eq!(video.title, expected);
    }
}
