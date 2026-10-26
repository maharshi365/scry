use super::{make_link, metadata_json};
use crate::local_media::infer;
use scry_addons::Meta;
use std::path::Path;

fn series() -> Meta {
    serde_json::from_value(
        serde_json::json!({"id":"show-id","type":"series","name":"Show","videos":[
            {"id":"provider:actual-episode-a","season":1,"episode":1,"title":"First"},
            {"id":"provider:actual-episode-b","season":1,"episode":2,"title":"Second"}
        ]}),
    )
    .unwrap()
}

#[test]
fn episode_mapping_uses_real_provider_ids_and_does_not_invent_missing_episodes() {
    let meta = series();
    let hints = infer(Path::new("Show/Show.S01E02.mkv"));
    let link = make_link("provider", &meta, &hints, None, false).unwrap();
    assert_eq!(link.external_id, "provider:actual-episode-b");
    assert_eq!(link.kind, "episode");
    let hints = infer(Path::new("Show/Show.S01E03.mkv"));
    assert!(make_link("provider", &meta, &hints, None, false).is_none());
}

#[test]
fn only_imdb_style_identities_use_the_imdb_namespace() {
    let addon = serde_json::from_value(serde_json::json!({"manifest_url":"https://example.invalid/manifest.json","manifest":{"id":"provider","name":"Provider"}})).unwrap();
    assert_eq!(super::namespace(&addon, "tt123:1:1"), "imdb");
    assert_eq!(super::namespace(&addon, "tt-provider-title"), "provider");
}

#[test]
fn ambiguous_numbering_requires_explicit_episode_choice() {
    let hints = infer(Path::new("Show/Show.S01E01E02.mkv"));
    let meta = series();
    assert!(make_link("provider", &meta, &hints, None, false).is_none());
    let link = make_link(
        "provider",
        &meta,
        &hints,
        Some("provider:actual-episode-b"),
        true,
    )
    .unwrap();
    assert_eq!(link.external_id, "provider:actual-episode-b");
    assert!(link.manual);
}

#[test]
fn cached_file_metadata_is_small_but_group_inventory_keeps_all_episodes() {
    let meta = series();
    let link = make_link(
        "provider",
        &meta,
        &infer(Path::new("Show.S01E01.mkv")),
        None,
        false,
    )
    .unwrap();
    let file_meta: Meta = serde_json::from_str(&link.metadata).unwrap();
    assert_eq!(file_meta.videos.len(), 1);
    assert_eq!(file_meta.videos[0].id, "provider:actual-episode-a");
    let group_meta: Meta = serde_json::from_str(&metadata_json(&meta, None)).unwrap();
    assert_eq!(group_meta.videos.len(), 2);
    assert_eq!(group_meta.videos[1].id, "provider:actual-episode-b");
}

#[test]
fn confirmed_show_matches_existing_and_future_numbered_files_without_network() -> anyhow::Result<()>
{
    use super::{Matcher, namespace};
    use scry_addons::{Addon, Client};
    use scry_library::Library;
    use std::sync::Arc;
    let folder = tempfile::tempdir()?;
    let show_root = folder.path().join("Unclear");
    std::fs::create_dir_all(show_root.join("Season 1"))?;
    for episode in ["01.mkv", "02.mkv"] {
        std::fs::write(show_root.join("Season 1").join(episode), b"episode")?;
    }
    let db = Library::memory()?;
    db.add_local_root(&show_root.to_string_lossy())?;
    let runtime = Arc::new(tokio::runtime::Runtime::new()?);
    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    let matcher = Matcher {
        client: Client::new()?,
        addons: Vec::new(),
        runtime,
        cancel: Arc::default(),
        inbox: super::super::Inbox {
            tx,
            wake: Arc::new(|| {}),
        },
    };
    matcher.scan(&db)?;
    assert!(
        db.local_files(0, false)?
            .iter()
            .all(|f| f.external_id.is_none())
    );
    let addon: Addon = serde_json::from_value(serde_json::json!({
        "manifest_url":"https://example.invalid/manifest.json", "manifest":{"id":"provider","name":"Provider"}
    }))?;
    let mut meta = series();
    let third: scry_addons::Video = serde_json::from_value(
        serde_json::json!({"id":"third-actual-id","season":1,"episode":3,"title":"Third"}),
    )?;
    meta.videos.push(third);
    let id = db.local_files(0, false)?[0].id;
    matcher.confirm(&db, id, &addon, &meta, None, true)?;
    assert!(
        db.local_files(0, false)?
            .iter()
            .all(|f| f.manual && f.external_id.is_some())
    );
    std::fs::write(show_root.join("Season 1").join("03.mkv"), b"episode")?;
    matcher.scan(&db)?;
    let third = db.local_sources(&namespace(&addon, &meta.id), "episode", "third-actual-id")?;
    assert_eq!(third.len(), 1);
    assert!(!third[0].manual);
    let cached: Meta =
        serde_json::from_str(&db.local_metadata("provider", "series", "show-id")?.unwrap())?;
    assert_eq!(cached.videos.len(), 3);
    Ok(())
}

#[test]
fn automatic_scan_links_movies_and_episodes_to_provider_identities() -> anyhow::Result<()> {
    use super::Matcher;
    use scry_addons::{Addon, Client};
    use scry_library::Library;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let runtime = Arc::new(tokio::runtime::Runtime::new()?);
    let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))?;
    let address = listener.local_addr()?;
    let server = runtime.spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { break; };
            tokio::spawn(async move {
                let mut request = [0; 4096];
                let count = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..count]);
                let movie = serde_json::json!({"id":"tt2543164","type":"movie","name":"Arrival","year":"2016"});
                let show = serde_json::json!({"id":"show-provider-id","type":"series","name":"Show","videos":[{"id":"actual-episode-id","season":1,"episode":1,"title":"First"}]});
                let body = if request.contains("/catalog/movie/") { serde_json::json!({"metas":[movie]}) }
                    else if request.contains("/catalog/anime/") { serde_json::json!({"metas":[show]}) }
                    else if request.contains("/meta/movie/") { serde_json::json!({"meta":movie}) }
                    else { serde_json::json!({"meta":show}) }.to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            });
        }
    });
    let addon: Addon = serde_json::from_value(serde_json::json!({
        "manifest_url":format!("http://{address}/manifest.json"),
        "manifest":{"id":"provider","name":"Provider","resources":["meta"],"catalogs":[
            {"id":"movies","type":"movie","extra":[{"name":"search"}]},
            {"id":"shows","type":"anime","extra":[{"name":"search"}]}
        ]}
    }))?;
    let folder = tempfile::tempdir()?;
    std::fs::create_dir_all(folder.path().join("Show").join("Season 1"))?;
    std::fs::write(folder.path().join("Arrival.2016.mkv"), b"movie")?;
    std::fs::write(
        folder.path().join("Show").join("Season 1").join("01.mkv"),
        b"episode",
    )?;
    let db = Library::memory()?;
    db.add_local_root(&folder.path().to_string_lossy())?;
    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    let matcher = Matcher {
        client: Client::new()?,
        addons: vec![addon],
        runtime,
        cancel: Arc::default(),
        inbox: super::super::Inbox {
            tx,
            wake: Arc::new(|| {}),
        },
    };
    matcher.scan(&db)?;
    server.abort();
    assert_eq!(db.local_sources("imdb", "movie", "tt2543164")?.len(), 1);
    assert_eq!(
        db.local_sources("provider", "episode", "actual-episode-id")?
            .len(),
        1
    );
    assert!(db.local_files(0, true)?.is_empty());
    Ok(())
}
