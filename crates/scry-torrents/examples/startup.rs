//! Controlled loopback torrent/decode benchmark. This does not measure visible presentation.
use anyhow::{Context, Result, bail, ensure};
use librqbit::{AddTorrent, AddTorrentOptions, CreateTorrentOptions, Session, SessionOptions};
use scry_addons::Stream;
use scry_playback::{PlaybackEvent, PlaybackTarget, Player, telemetry::Tracker};
use scry_torrents::Torrents;
use serde_json::json;
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    num::NonZeroU32,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

struct Options {
    media: PathBuf,
    report: PathBuf,
    runs: u32,
    piece_bytes: u32,
    upload_bps: NonZeroU32,
    stremio: Option<url::Url>,
}

#[derive(Clone, Copy)]
enum CacheState {
    Cold,
    Complete,
}

impl CacheState {
    fn label(self) -> &'static str {
        match self {
            Self::Cold => "cold",
            Self::Complete => "complete",
        }
    }
}

impl Options {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self> {
        let mut media = None;
        let mut report = None;
        let mut runs = 5;
        let mut piece_bytes: u32 = 1024 * 1024;
        let mut upload_bps = NonZeroU32::new(1024 * 1024).context("Invalid upload limit")?;
        let mut stremio = None;
        while let Some(flag) = args.next() {
            let value = args.next().context("Each benchmark option needs a value")?;
            match flag.as_str() {
                "--media" => media = Some(PathBuf::from(value)),
                "--report" => report = Some(PathBuf::from(value)),
                "--runs" => runs = value.parse()?,
                "--piece-bytes" => piece_bytes = value.parse()?,
                "--upload-bps" => upload_bps = value.parse()?,
                "--stremio" => {
                    let parsed = url::Url::parse(&value)?;
                    let loopback = match parsed.host() {
                        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                        _ => false,
                    };
                    ensure!(
                        parsed.scheme() == "http"
                            && parsed.username().is_empty()
                            && parsed.password().is_none()
                            && parsed.query().is_none()
                            && parsed.fragment().is_none()
                            && parsed.path() == "/"
                            && loopback,
                        "Use a dedicated Stremio test server at a loopback HTTP origin"
                    );
                    stremio = Some(parsed);
                }
                _ => bail!("Unknown benchmark option"),
            }
        }
        ensure!((1..=20).contains(&runs), "Runs must be between 1 and 20");
        ensure!(
            (16384..=100 * 1024 * 1024).contains(&upload_bps.get()),
            "Upload limit must be between 16 KiB/s and 100 MiB/s"
        );
        ensure!(
            (65536..=16 * 1024 * 1024).contains(&piece_bytes) && piece_bytes.is_power_of_two(),
            "Piece size must be a power of two between 64 KiB and 16 MiB"
        );
        let media = media
            .context("Pass --media PATH to a synthetic video fixture")?
            .canonicalize()?;
        ensure!(
            media.is_file() && fs::metadata(&media)?.len() <= 512 * 1024 * 1024,
            "Fixture must be a file no larger than 512 MiB"
        );
        let report = report.context("Pass --report PATH to a new JSONL file")?;
        Ok(Self {
            media,
            report,
            runs,
            piece_bytes,
            upload_bps,
            stremio,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let options = Options::parse(std::env::args().skip(1))?;
    let mut output = BufWriter::new(
        File::options()
            .write(true)
            .create_new(true)
            .open(&options.report)?,
    );
    writeln!(
        output,
        "{}",
        json!({"event":"benchmark", "schema":1,
            "mode":"decode-only-loopback", "runs":options.runs,
            "scry_version":env!("CARGO_PKG_VERSION"), "os":std::env::consts::OS,
            "profile":if cfg!(debug_assertions) {"debug"} else {"release"},
            "seed_reset":"fresh-session-per-trial",
            "piece_bytes":options.piece_bytes, "upload_bps":options.upload_bps.get(),
            "media_bytes":fs::metadata(&options.media)?.len(), "limiter_start":"empty",
            "limitations":"Readiness and advancing decode, not visible frames. One rate-limited local seed; no public discovery or mixed-speed peers. External Stremio disk cache is uncontrolled."
        })
    )?;
    let root = tempfile::tempdir()?;
    let torrent = librqbit::create_torrent(
        &options.media,
        CreateTorrentOptions {
            piece_length: Some(options.piece_bytes),
            ..Default::default()
        },
    )
    .await?;
    let hash = torrent.info_hash().as_string();
    let metadata = torrent.as_bytes()?;
    let mut seed = start_seed(&options, &metadata, None).await?;
    let seed_port = seed.tcp_listen_port().context("Seed has no listener")?;
    let peer = format!("127.0.0.1:{seed_port}").parse()?;
    let stream: Stream = serde_json::from_value(json!({"infoHash":hash, "fileIdx":0}))?;
    let mut reused = None;
    let result = async {
        for run in 0..options.runs {
            // Alternate order to reduce systematic warmup bias.
            let strategies = if run % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            };
            for state in [CacheState::Cold, CacheState::Complete] {
                for reuse in strategies {
                    reset_seed(&mut seed, &options, &metadata, seed_port).await?;
                    let strategy = if reuse {
                        "reused-session"
                    } else {
                        "fresh-session"
                    };
                    let cache = root.path().join(strategy);
                    prepare_cache(&options.media, &cache, &hash, &metadata, state)?;
                    let started = Instant::now();
                    let mut fresh = None;
                    let slot = if reuse { &mut reused } else { &mut fresh };
                    if slot.is_none() {
                        *slot =
                            Some(Torrents::with_local_peer(cache, 1024 * 1024 * 1024, peer).await?);
                    }
                    let engine = slot.as_ref().context("Missing client session")?;
                    let source =
                        tokio::time::timeout(Duration::from_secs(45), engine.play(&stream))
                            .await??;
                    let prepared_s = started.elapsed().as_secs_f64();
                    let playback = decode(&source, started, Some(engine)).await?;
                    let stats = engine.stats();
                    engine.stop().await;
                    writeln!(
                        output,
                        "{}",
                        json!({"event":"trial", "engine":"scry", "strategy":strategy,
                            "run":run, "cache":state.label(),
                            "prepared_s":prepared_s, "playback":playback,
                            "available_file_bytes":stats.and_then(|stats| stats.downloaded_bytes)
                        })
                    )?;
                    output.flush()?;
                }
            }
            if let Some(server) = &options.stremio {
                reset_seed(&mut seed, &options, &metadata, seed_port).await?;
                stremio_trial(server, peer, &hash, run, &mut output).await?;
            }
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    drop(reused);
    seed.stop().await;
    if result.is_err() {
        writeln!(
            output,
            "{}",
            json!({"event":"failure", "reason":"Benchmark trial failed"})
        )?;
    }
    output.flush()?;
    result
}

fn prepare_cache(
    media: &Path,
    cache: &Path,
    hash: &str,
    metadata: &[u8],
    state: CacheState,
) -> Result<()> {
    let entry = cache.join("v1").join(hash);
    let file = entry
        .join("data")
        .join(media.file_name().context("Missing filename")?);
    match state {
        CacheState::Cold => {
            // These exact files belong to this benchmark's private temporary directory.
            for path in [file, entry.join("metadata.torrent")] {
                if path.try_exists()? {
                    fs::remove_file(path)?;
                }
            }
        }
        CacheState::Complete => {
            fs::create_dir_all(entry.join("data"))?;
            fs::copy(media, file)?;
            fs::write(entry.join("metadata.torrent"), metadata)?;
        }
    }
    Ok(())
}

async fn reset_seed(
    seed: &mut std::sync::Arc<Session>,
    options: &Options,
    metadata: &[u8],
    port: u16,
) -> Result<()> {
    // Recreate the seed outside the timed region. Resetting only its token bucket
    // leaves outstanding upload work from the previous client in the same session.
    seed.stop().await;
    *seed = start_seed(options, metadata, Some(port)).await?;
    seed.ratelimits
        .prepare_for_upload(options.upload_bps)
        .await?;
    Ok(())
}

async fn start_seed(
    options: &Options,
    metadata: &[u8],
    port: Option<u16>,
) -> Result<std::sync::Arc<Session>> {
    let ports = if let Some(port) = port {
        port..port.checked_add(1).context("Invalid seed port")?
    } else {
        40000..40100
    };
    let seed = Session::new_with_opts(
        options
            .media
            .parent()
            .context("Fixture has no parent")?
            .into(),
        SessionOptions {
            disable_dht: true,
            disable_dht_persistence: true,
            listen_port_range: Some(ports),
            ratelimits: librqbit::limits::LimitsConfig {
                upload_bps: Some(options.upload_bps),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .await?;
    let added = seed
        .add_torrent(
            AddTorrent::from_bytes(metadata.to_owned()),
            Some(AddTorrentOptions {
                overwrite: true,
                disable_trackers: true,
                ..Default::default()
            }),
        )
        .await?;
    let handle = added.into_handle().context("Seed has no torrent handle")?;
    tokio::time::timeout(Duration::from_secs(30), handle.wait_until_initialized()).await??;
    Ok(seed)
}

async fn decode(
    source: &str,
    started: Instant,
    torrents: Option<&Torrents>,
) -> Result<serde_json::Value> {
    let player = Player::open(PlaybackTarget::DecodeOnly)?;
    player
        .load(source)
        .map_err(|_| anyhow::anyhow!("Benchmark media load failed"))?;
    let mut ready_s = None;
    let mut advancing_s = None;
    let mut available_at_ready = None;
    let mut available_at_advance = None;
    let mut tracker = Tracker::default();
    let mut next_sample = Instant::now();
    let mut complete = false;
    while started.elapsed() < Duration::from_secs(45) {
        match player.event(0.0) {
            Some(PlaybackEvent::Ready) => {
                if ready_s.is_none() {
                    ready_s = Some(started.elapsed().as_secs_f64());
                    available_at_ready = torrents
                        .and_then(Torrents::stats)
                        .and_then(|stats| stats.downloaded_bytes);
                }
            }
            Some(PlaybackEvent::Failed | PlaybackEvent::Shutdown) => {
                bail!("Benchmark playback failed")
            }
            Some(PlaybackEvent::Ended) => {
                complete = true;
            }
            _ => {}
        }
        if Instant::now() >= next_sample || complete {
            let sample = player.sample(started.elapsed().as_secs_f64());
            tracker.observe(&sample);
            if advancing_s.is_none() && tracker.summary.playback_advanced_s > 0.05 {
                advancing_s = Some(started.elapsed().as_secs_f64());
                available_at_advance = torrents
                    .and_then(Torrents::stats)
                    .and_then(|stats| stats.downloaded_bytes);
            }
            next_sample = Instant::now() + Duration::from_millis(100);
        }
        if complete
            || advancing_s.is_some_and(|first| started.elapsed().as_secs_f64() - first >= 5.0)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    ensure!(
        advancing_s.is_some(),
        "No advancing playback before the benchmark deadline"
    );
    Ok(json!({"ready_s":ready_s, "advancing_s":advancing_s,
        "available_file_bytes_at_ready":available_at_ready, "available_file_bytes_at_advance":available_at_advance,
        "summary":tracker.summary, "mpv_version":player.property("mpv-version")}))
}

async fn stremio_trial(
    server: &url::Url,
    peer: std::net::SocketAddr,
    hash: &str,
    run: u32,
    output: &mut BufWriter<File>,
) -> Result<()> {
    use axum::{Router, routing::get};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let peers_url = format!("http://{}/peers", listener.local_addr()?);
    let router = Router::new().route("/peers", get(move || async move { format!("{peer}\n") }));
    let peers = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let started = Instant::now();
    let result =
        async {
            let response = client.post(server.join(&format!("{hash}/create"))?).json(&json!({
            "torrent":{"infoHash":hash}, "peerSearch":{"sources":[peers_url], "min":1, "max":1}
        })).send().await.map_err(|_| anyhow::anyhow!("Stremio test server request failed"))?;
            ensure!(
                response.status().is_success(),
                "Stremio test server could not create the fixture torrent"
            );
            let prepared_s = started.elapsed().as_secs_f64();
            let source = server.join(&format!("{hash}/0"))?;
            let playback = decode(source.as_str(), started, None).await?;
            writeln!(
                output,
                "{}",
                json!({"event":"trial", "engine":"stremio", "run":run,
                    "cache":"external-uncontrolled", "prepared_s":prepared_s, "playback":playback
                })
            )?;
            output.flush()?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
    // Remove only this fixture engine, never other engines or cache directories.
    let removed = client
        .get(server.join(&format!("{hash}/remove"))?)
        .send()
        .await;
    peers.abort();
    result?;
    ensure!(
        removed.is_ok_and(|response| response.status().is_success()),
        "Stremio fixture engine cleanup failed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_states_reset_only_the_fixture_and_restore_identical_warm_bytes() -> Result<()> {
        let root = tempfile::tempdir()?;
        let media = root.path().join("fixture.mp4");
        fs::write(&media, b"synthetic media")?;
        let cache = root.path().join("cache");
        let hash = "0123456789abcdef0123456789abcdef01234567";
        prepare_cache(&media, &cache, hash, b"metadata", CacheState::Complete)?;
        let entry = cache.join("v1").join(hash);
        fs::write(entry.join("unrelated"), b"keep")?;
        fs::write(entry.join("data/fixture.mp4"), b"partial")?;
        prepare_cache(&media, &cache, hash, b"metadata", CacheState::Complete)?;
        assert_eq!(
            fs::read(entry.join("data/fixture.mp4"))?,
            b"synthetic media"
        );
        assert_eq!(fs::read(entry.join("metadata.torrent"))?, b"metadata");
        prepare_cache(&media, &cache, hash, b"metadata", CacheState::Cold)?;
        assert!(!entry.join("data/fixture.mp4").exists());
        assert!(!entry.join("metadata.torrent").exists());
        assert_eq!(fs::read(entry.join("unrelated"))?, b"keep");
        assert_eq!(fs::read(media)?, b"synthetic media");
        Ok(())
    }

    #[test]
    fn options_bound_work_and_restrict_stremio_to_an_uncredentialed_loopback_origin() -> Result<()>
    {
        let root = tempfile::tempdir()?;
        let media = root.path().join("fixture.mp4");
        fs::write(&media, [])?;
        let parse = |flag: &str, value: &str| {
            Options::parse(
                [
                    "--media".into(),
                    media.to_string_lossy().into_owned(),
                    "--report".into(),
                    root.path()
                        .join("report.jsonl")
                        .to_string_lossy()
                        .into_owned(),
                    flag.into(),
                    value.into(),
                ]
                .into_iter(),
            )
        };
        for (flag, value) in [
            ("--runs", "0"),
            ("--runs", "21"),
            ("--piece-bytes", "100000"),
            ("--upload-bps", "0"),
            ("--upload-bps", "1"),
            ("--stremio", "http://192.0.2.1:11470/"),
            ("--stremio", "http://127.0.0.1:11470/private?token=secret"),
            ("--stremio", "http://user:secret@127.0.0.1:11470/"),
        ] {
            let error = parse(flag, value)
                .err()
                .context("Invalid benchmark options were accepted")?;
            assert!(!error.to_string().contains("secret"));
        }
        assert!(
            parse("--stremio", "http://127.0.0.1:11470/")?
                .stremio
                .is_some()
        );
        assert!(parse("--stremio", "http://[::1]:11470/")?.stremio.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn stremio_failure_still_removes_only_the_requested_fixture_engine() -> Result<()> {
        use axum::{
            Router,
            http::StatusCode,
            routing::{get, post},
        };
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let removed = Arc::new(AtomicUsize::new(0));
        let count = removed.clone();
        let hash = "0123456789abcdef0123456789abcdef01234567";
        let router = Router::new()
            .route(
                &format!("/{hash}/create"),
                post(|| async { StatusCode::BAD_REQUEST }),
            )
            .route(
                &format!("/{hash}/remove"),
                get(move || async move {
                    count.fetch_add(1, Ordering::Relaxed);
                    StatusCode::OK
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = url::Url::parse(&format!("http://{}/", listener.local_addr()?))?;
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let root = tempfile::tempdir()?;
        let mut output = BufWriter::new(File::create(root.path().join("trial.jsonl"))?);
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            stremio_trial(&address, "127.0.0.1:1234".parse()?, hash, 0, &mut output),
        )
        .await?;
        server.abort();
        assert!(result.is_err(), "A rejected fixture must fail the trial");
        assert_eq!(
            removed.load(Ordering::Relaxed),
            1,
            "A failed create must still clean up its fixture engine"
        );
        assert_eq!(
            fs::metadata(root.path().join("trial.jsonl"))?.len(),
            0,
            "A failed trial must not record successful playback"
        );
        Ok(())
    }
}
