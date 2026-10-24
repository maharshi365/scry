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

/// Created only when a torrent is selected. Exactly one torrent runs at a time.
pub struct Torrents {
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
        // Each engine owns a new cache directory. Never remove files from a caller's
        // existing directory, or from another Scry session.
        let folder = folder.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&folder)?;
        let session = Session::new_with_opts(
            folder.clone(),
            SessionOptions {
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
        let (source, index) = if let Some(index) = stream.file_idx {
            (AddTorrent::from_url(magnet), index)
        } else {
            // Resolve metadata without opening or allocating every file in a season pack.
            let listed = tokio::time::timeout(
                std::time::Duration::from_secs(90),
                self.session.add_torrent(
                    AddTorrent::from_url(magnet),
                    Some(AddTorrentOptions {
                        list_only: true,
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
                .map(|file| Ok((file.filename.to_string()?, file.len)))
                .collect::<Result<Vec<_>>>()?;
            let index = select_video(
                &files
                    .iter()
                    .map(|(name, len)| (name.as_str(), *len))
                    .collect::<Vec<_>>(),
            )
            .context("Torrent has no supported video file")?;
            (AddTorrent::from_bytes(listed.torrent_bytes), index)
        };
        let added = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            self.api.api_add_torrent(
                source,
                Some(AddTorrentOptions {
                    paused: true,
                    only_files: Some(vec![index]),
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
        ] {
            assert!(byte_range(Some(value), 100).is_err());
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
