#![forbid(unsafe_code)]

mod stats;
pub use stats::Stats;

use anyhow::{Context, Result, bail};
use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use librqbit::{AddTorrent, AddTorrentOptions, AddTorrentResponse, Api, Session, SessionOptions};
use scry_addons::Stream;
use std::{io::SeekFrom, path::PathBuf, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

// Bound URL parsing and magnet escaping even when an add-on supplies huge sources.
const MAX_TRACKER_URL_BYTES: usize = 8 * 1024;
// The engine opens every file in a pack, including files outside `only_files`.
const MAX_TORRENT_FILES: usize = 1024;

/// Created only when a torrent is selected. Exactly one torrent runs at a time.
pub struct Torrents {
    #[cfg(feature = "profiling")]
    peer: Option<std::net::SocketAddr>,
    session: Arc<Session>,
    api: Api,
    port: u16,
    token: String,
    server: tokio::task::JoinHandle<()>,
    folder: PathBuf,
}

#[derive(Clone)]
struct Server {
    api: Api,
    token: String,
}

impl Torrents {
    /// # Errors
    /// Returns an error if session storage, the torrent engine, or its local HTTP listener cannot start.
    pub async fn new(folder: PathBuf) -> Result<Self> {
        #[cfg(feature = "profiling")]
        let peer = std::env::var("SCRY_PROFILE_TORRENT_PEER")
            .ok()
            .map(|value| value.parse::<std::net::SocketAddr>())
            .transpose()
            .context("Invalid profiling peer")?;
        #[cfg(feature = "profiling")]
        if peer.is_some_and(|peer| !peer.ip().is_loopback()) {
            bail!("Profiling peer must be loopback");
        }
        // Each engine owns a new cache directory. Never remove files from a caller's
        // existing directory, or from another Scry session.
        let folder = folder.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&folder)?;
        let session = Session::new_with_opts(
            folder.clone(),
            SessionOptions {
                #[cfg(feature = "profiling")]
                disable_dht: peer.is_some(),
                disable_dht_persistence: true,
                enable_upnp_port_forwarding: false,
                concurrent_init_limit: Some(1),
                ..Default::default()
            },
        )
        .await
        .context("Cannot start torrent engine")?;
        let api = Api::new(session.clone(), None);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let token = uuid::Uuid::new_v4().to_string();
        let router = Router::new()
            .route("/:token/:id/:file", get(serve))
            .with_state(Server {
                api: api.clone(),
                token: token.clone(),
            });
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self {
            #[cfg(feature = "profiling")]
            peer,
            session,
            api,
            port,
            token,
            server,
            folder,
        })
    }

    /// # Errors
    /// Returns an error for invalid torrent sources, metadata timeouts, or file initialization failures.
    pub async fn play(&self, stream: &Stream) -> Result<String> {
        self.stop().await;
        let result = self.prepare(stream).await;
        if result.is_err() {
            self.stop().await;
        }
        result
    }

    async fn prepare(&self, stream: &Stream) -> Result<String> {
        let magnet = magnet(stream)?;
        // Inspect every path before the engine opens files, even with an explicit index.
        let listed = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            self.session.add_torrent(
                AddTorrent::from_url(magnet),
                Some(AddTorrentOptions {
                    #[cfg(feature = "profiling")]
                    initial_peers: self.peer.map(|peer| vec![peer]),
                    #[cfg(feature = "profiling")]
                    disable_trackers: self.peer.is_some(),
                    list_only: true,
                    sub_folder: Some(String::new()),
                    ..Default::default()
                }),
            ),
        )
        .await
        .context("Torrent metadata timed out")??;
        let AddTorrentResponse::ListOnly(listed) = listed else {
            bail!("Unexpected torrent metadata response");
        };
        let files = listed
            .info
            .iter_file_details()?
            .take(MAX_TORRENT_FILES + 1)
            .map(|file| Ok((file.filename.to_string()?, file.len)))
            .collect::<Result<Vec<_>>>()?;
        let index = select_torrent_file(
            &files
                .iter()
                .map(|(name, len)| (name.as_str(), *len))
                .collect::<Vec<_>>(),
            stream.file_idx,
        )?;
        let initial_peers = listed.seen_peers;
        #[cfg(feature = "profiling")]
        let initial_peers = self.peer.map_or(initial_peers, |peer| vec![peer]);
        let added = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            self.api.api_add_torrent(
                AddTorrent::from_bytes(listed.torrent_bytes),
                Some(AddTorrentOptions {
                    initial_peers: Some(initial_peers),
                    #[cfg(feature = "profiling")]
                    disable_trackers: self.peer.is_some(),
                    paused: true,
                    only_files: Some(vec![index]),
                    sub_folder: Some(String::new()),
                    overwrite: true,
                    ..Default::default()
                }),
            ),
        )
        .await
        .context("Torrent metadata timed out")??;
        let id = added.id.context("Torrent has no session ID")?;
        let handle = self.api.mgr_handle(id.into())?;
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            handle.wait_until_initialized(),
        )
        .await
        .context("Torrent file initialization timed out")??;
        self.session.unpause(&handle).await?;
        Ok(format!(
            "http://127.0.0.1:{}/{}/{id}/{index}",
            self.port, self.token
        ))
    }

    pub async fn stop(&self) {
        let ids = self
            .session
            .with_torrents(|items| items.map(|(id, _)| id).collect::<Vec<_>>());
        for id in ids {
            let _ = self.session.delete(id.into(), true).await;
        }
    }
}

impl Drop for Torrents {
    fn drop(&mut self) {
        self.server.abort();
        self.session.cancellation_token().cancel();
        // Only remove an empty directory here; interrupted downloads are not swept.
        let _ = std::fs::remove_dir(&self.folder);
    }
}

/// # Errors
/// Returns an error if the stream lacks a valid 40-character hexadecimal torrent hash.
pub fn magnet(stream: &Stream) -> Result<String> {
    let hash = stream
        .info_hash
        .as_deref()
        .context("Stream has no torrent hash")?;
    if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("Invalid torrent info hash");
    }
    let mut url = url::Url::parse("magnet:")?;
    url.query_pairs_mut()
        .append_pair("xt", &format!("urn:btih:{hash}"));
    for source in stream.sources.iter().take(32) {
        if let Some(tracker) = source.strip_prefix("tracker:")
            && tracker.len() <= MAX_TRACKER_URL_BYTES
            && url::Url::parse(tracker)
                .is_ok_and(|u| matches!(u.scheme(), "udp" | "http" | "https"))
        {
            url.query_pairs_mut().append_pair("tr", tracker);
        }
    }
    Ok(url.to_string())
}

fn select_video(files: &[(&str, u64)]) -> Option<usize> {
    files
        .iter()
        .enumerate()
        .filter(|(_, (name, _))| {
            std::path::Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| {
                    ["mkv", "mp4", "webm", "avi", "mov", "m4v", "ts"]
                        .contains(&e.to_ascii_lowercase().as_str())
                })
        })
        .max_by_key(|(_, (_, len))| *len)
        .map(|(i, _)| i)
}

fn select_torrent_file(files: &[(&str, u64)], index: Option<usize>) -> Result<usize> {
    if files.len() > MAX_TORRENT_FILES {
        bail!("Torrent contains too many files");
    }
    // The engine opens unselected files too. Reject Windows prefixes, alternate
    // streams, device aliases, and components Win32 can normalize into parents.
    for (name, _) in files {
        if name.split(['/', '\\']).any(|part| {
            part.is_empty()
                || part.contains(':')
                || part.ends_with(['.', ' '])
                || is_windows_device_name(part)
        }) {
            bail!("Torrent contains an unsafe file path");
        }
    }
    if index.is_some_and(|index| index >= files.len()) {
        bail!("Torrent file index is out of range");
    }
    index
        .or_else(|| select_video(files))
        .context("Torrent has no supported video file")
}

fn is_windows_device_name(component: &str) -> bool {
    // DOS aliases remain reserved with extensions, including COM/LPT with ¹²³.
    let stem = component
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ');
    ["CON", "PRN", "AUX", "NUL"]
        .iter()
        .any(|device| stem.eq_ignore_ascii_case(device))
        || (stem.get(..3).is_some_and(|prefix| {
            prefix.eq_ignore_ascii_case("COM") || prefix.eq_ignore_ascii_case("LPT")
        }) && matches!(
            stem.get(3..),
            Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
        ))
}

/// Inclusive single byte range. Multipart ranges are deliberately rejected.
fn byte_range(value: Option<&str>, len: u64) -> Result<(u64, u64, bool)> {
    if len == 0 {
        bail!("Empty file");
    }
    let Some(value) = value else {
        return Ok((0, len - 1, false));
    };
    let value = value.strip_prefix("bytes=").context("Invalid range unit")?;
    let (start, end) = value.split_once('-').context("Invalid range")?;
    if !start.bytes().all(|byte| byte.is_ascii_digit())
        || !end.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("Invalid range digits");
    }
    if start.is_empty() {
        let suffix = end.parse::<u64>()?;
        if suffix == 0 {
            bail!("Invalid suffix range");
        }
        return Ok((len.saturating_sub(suffix), len - 1, true));
    }
    let start = start.parse::<u64>()?;
    let end = if end.is_empty() {
        len - 1
    } else {
        end.parse::<u64>()?.min(len - 1)
    };
    if start >= len || start > end {
        bail!("Unsatisfiable range");
    }
    Ok((start, end, true))
}

async fn serve(
    State(state): State<Server>,
    Path((token, id, file)): Path<(String, usize, usize)>,
    headers: HeaderMap,
) -> Response {
    if token != state.token {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(mut stream) = state.api.api_stream(id.into(), file) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let len = stream.len();
    let range = headers.get(header::RANGE).map(|v| v.to_str()).transpose();
    let Ok((start, end, partial)) = range
        .ok()
        .and_then(|value| byte_range(value, len).ok())
        .ok_or(())
    else {
        return (
            StatusCode::RANGE_NOT_SATISFIABLE,
            [(header::CONTENT_RANGE, format!("bytes */{len}"))],
        )
            .into_response();
    };
    if stream.seek(SeekFrom::Start(start)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let mut response = Response::builder()
        .status(if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, (end - start + 1).to_string())
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CACHE_CONTROL, "no-store");
    if partial {
        response = response.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
    }
    response
        .body(Body::from_stream(ReaderStream::with_capacity(
            stream.take(end - start + 1),
            64 * 1024,
        )))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Torrent fixtures and local bridge test failures must fail the test immediately"
)]
mod tests {
    use super::*;
    #[test]
    fn ranges_support_seeking_and_suffixes() {
        assert_eq!(
            byte_range(Some("bytes=10-20"), 100).unwrap(),
            (10, 20, true)
        );
        assert_eq!(byte_range(Some("bytes=90-"), 100).unwrap(), (90, 99, true));
        assert_eq!(byte_range(Some("bytes=-10"), 100).unwrap(), (90, 99, true));
        assert_eq!(byte_range(Some("bytes=0-900"), 100).unwrap(), (0, 99, true));
        assert_eq!(byte_range(None, 100).unwrap(), (0, 99, false));
    }
    #[test]
    fn invalid_ranges_do_not_escape_the_file() {
        for value in [
            "bytes=100-",
            "bytes=20-10",
            "bytes=-0",
            "bytes=0-1,4-5",
            "items=0-1",
            "bytes=+1-2",
            "bytes=1-+2",
            "bytes=-+2",
        ] {
            assert!(byte_range(Some(value), 100).is_err(), "range {value}");
        }
        assert!(byte_range(None, 0).is_err());
    }
    #[test]
    fn largest_video_is_chosen_not_largest_archive() {
        assert_eq!(
            select_video(&[("a.zip", 9999), ("sample.mp4", 1), ("film.MKV", 900)]),
            Some(2)
        );
        assert_eq!(select_video(&[("a.txt", 100)]), None);
    }

    #[test]
    fn torrent_paths_cannot_escape_the_cache_or_name_windows_streams() {
        for name in [
            "C:escape.mp4",
            "movie.mp4:stream",
            "../escape.mp4",
            "../film/ok.mp4",
            "/escape.mp4",
            "folder./film.mp4",
            "folder /film.mp4",
        ] {
            for index in [None, Some(0)] {
                assert!(
                    select_torrent_file(&[(name, 100)], index).is_err(),
                    "path {name}, index {index:?}"
                );
            }
            assert!(
                select_torrent_file(&[("film.mp4", 100), (name, 1)], Some(0)).is_err(),
                "unselected path {name}"
            );
        }
        assert_eq!(
            select_torrent_file(&[("season/film.mp4", 100)], None).unwrap(),
            0
        );
        assert!(select_torrent_file(&[("film.mp4", 100)], Some(1)).is_err());
    }

    #[test]
    fn oversized_torrent_packs_are_rejected_before_file_initialization() {
        let files = vec![("film.mp4", 100); 1025];
        assert!(select_torrent_file(&files, Some(0)).is_err());
    }

    #[test]
    fn torrent_paths_reject_windows_device_components() {
        let devices = ["CON", "PRN", "AUX", "NUL"]
            .into_iter()
            .map(str::to_owned)
            .chain(["COM", "LPT"].into_iter().flat_map(|prefix| {
                ["1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"]
                    .into_iter()
                    .map(move |digit| format!("{prefix}{digit}"))
            }));
        for device in devices {
            for spelling in [device.clone(), device.to_ascii_lowercase()] {
                for suffix in ["", ".mp4", ".tar.mp4", " .mp4"] {
                    let component = format!("{spelling}{suffix}");
                    for name in [
                        component.clone(),
                        format!("season/{component}"),
                        format!("{component}/film.mp4"),
                        format!("season\\{component}\\film.mp4"),
                    ] {
                        assert!(
                            select_torrent_file(&[(&name, 100)], Some(0)).is_err(),
                            "selected device path {name}"
                        );
                        assert!(
                            select_torrent_file(&[("film.mp4", 100), (&name, 1)], Some(0)).is_err(),
                            "unselected device path {name}"
                        );
                    }
                }
            }
        }
        assert!(select_torrent_file(&[("cOm1.mp4", 100)], None).is_err());
    }

    #[test]
    fn torrent_paths_preserve_names_that_only_resemble_windows_devices() {
        for name in [
            "concert.mp4",
            "auxiliary/film.mp4",
            "COM0.mp4",
            "COM10.mp4",
            "LPT0.mp4",
            "LPT10.mp4",
            "COM¹0.mp4",
            "COM⁴.mp4",
            "film.CON.mp4",
            "season/film.mp4",
        ] {
            assert_eq!(
                select_torrent_file(&[(name, 100)], None).unwrap(),
                0,
                "path {name}"
            );
        }
    }

    #[test]
    fn magnet_only_forwards_supported_tracker_sources() {
        let stream: Stream = serde_json::from_value(serde_json::json!({
            "infoHash":"0123456789abcdef0123456789abcdef01234567",
            "sources":["tracker:udp://tracker.example:80/announce", "tracker:file:///private", "dht:router.example:6881"]
        })).unwrap();
        let url = url::Url::parse(&magnet(&stream).unwrap()).unwrap();
        let pairs: Vec<_> = url.query_pairs().collect();
        assert_eq!(pairs.len(), 2);
        assert_eq!(
            pairs[0].1,
            "urn:btih:0123456789abcdef0123456789abcdef01234567"
        );
        assert_eq!(pairs[1].1, "udp://tracker.example:80/announce");
    }

    #[test]
    fn magnet_bounds_tracker_url_size() {
        let stream: Stream = serde_json::from_value(serde_json::json!({
            "infoHash":"0123456789abcdef0123456789abcdef01234567",
            "sources":[
                format!("tracker:https://tracker.example/{}", "a".repeat(8192)),
                "tracker:https://tracker.example/announce"
            ]
        }))
        .unwrap();
        let url = url::Url::parse(&magnet(&stream).unwrap()).unwrap();
        let trackers: Vec<_> = url.query_pairs().filter(|(key, _)| key == "tr").collect();
        assert_eq!(trackers.len(), 1);
        assert_eq!(trackers[0].1, "https://tracker.example/announce");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn local_bridge_requires_token_and_returns_only_requested_bytes() {
        let temp = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("Temp/opencode"))
            .filter(|p| p.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let root = tempfile::tempdir_in(temp).unwrap();
        let media = root.path().join("fixture.mp4");
        std::fs::write(&media, b"0123456789abcdefghijklmnopqrstuvwxyz").unwrap();
        let torrent = librqbit::create_torrent(&media, librqbit::CreateTorrentOptions::default())
            .await
            .unwrap();
        let session = Session::new_with_opts(
            root.path().to_owned(),
            SessionOptions {
                disable_dht: true,
                disable_dht_persistence: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let api = Api::new(session.clone(), None);
        let added = api
            .api_add_torrent(
                AddTorrent::from_bytes(torrent.as_bytes().unwrap()),
                Some(AddTorrentOptions {
                    overwrite: true,
                    paused: true,
                    ..Default::default()
                }),
            )
            .await
            .unwrap();
        let id = added.id.unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            api.mgr_handle(id.into()).unwrap().wait_until_initialized(),
        )
        .await
        .unwrap()
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new()
            .route("/:token/:id/:file", get(serve))
            .with_state(Server {
                api,
                token: "test-token".into(),
            });
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let url = format!("http://{address}/test-token/{id}/0");
        let response = client
            .get(&url)
            .header(header::RANGE, "bytes=3-7")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 3-7/36");
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"34567");
        let denied = client
            .get(format!("http://{address}/wrong-token/{id}/0"))
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::NOT_FOUND);
        let invalid = client
            .get(&url)
            .header(header::RANGE, "bytes=100-")
            .send()
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(invalid.headers()[header::CONTENT_RANGE], "bytes */36");
        server.abort();
        session.stop().await;
    }
}
