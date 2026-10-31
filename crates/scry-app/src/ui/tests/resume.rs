use super::*;
use crate::library_worker::{Command, Worker};
use scry_library::{Library, Media, MediaKey, MediaKind};

#[test]
fn only_ready_playback_replaces_the_last_working_stream() {
    for (ready, current) in [(false, true), (true, true), (true, false)] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.sqlite3");
        let media = Media {
            key: MediaKey::new("provider", MediaKind::Movie, "movie"),
            title: "Movie".into(),
            parent: None,
        };
        let mut db = Library::open(&path).unwrap();
        db.upsert_media(&media).unwrap();
        db.save_stream(&media.key, r#"{"url":"https://example.com/old"}"#)
            .unwrap();
        let (mut state, tx) = state_with_events();
        state.library.worker = Some(Worker::start(path, Arc::new(|| {})).unwrap());
        state.library.playing = Some(media.clone());
        state.library.stream_candidate = Some(super::super::library_view::StreamCandidate {
            generation: state.player.generation + u64::from(!current),
            media: media.clone(),
            stream: serde_json::from_value(serde_json::json!({"url":"https://example.com/new"}))
                .unwrap(),
        });
        state.player.playing = true;
        tx.try_send(if ready {
            Event::Ready(state.player.generation)
        } else {
            Event::Stopped(state.player.generation, Some("Failed".into()))
        })
        .unwrap();
        poll(&mut state, 0.0, &mut Tx::default());
        state
            .library
            .worker
            .as_ref()
            .unwrap()
            .send(Command::Flush(99))
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            assert!(std::time::Instant::now() < deadline);
            if matches!(
                state.library.worker.as_ref().unwrap().try_recv(),
                Ok(crate::library_worker::Event::Flushed(99))
            ) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let saved: Stream =
            serde_json::from_str(&db.saved_stream(&media.key).unwrap().unwrap()).unwrap();
        assert_eq!(
            saved.url.as_deref(),
            Some(if ready && current {
                "https://example.com/new"
            } else {
                "https://example.com/old"
            })
        );
    }
}

#[test]
fn failed_resume_returns_to_the_same_episode_picker() {
    let (mut state, tx) = state_with_events();
    let media = Media {
        key: MediaKey::new("provider", MediaKind::Episode, "show:2:3"),
        title: "Episode".into(),
        parent: Some(MediaKey::new("provider", MediaKind::Series, "show")),
    };
    state.navigation.page = Page::Player;
    state.library.replaying = Some(media.clone());
    state.library.playing = Some(media);
    state.player.playing = true;
    tx.try_send(Event::Stopped(
        state.player.generation,
        Some("Failed".into()),
    ))
    .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert_eq!(state.navigation.page, Page::Detail);
    assert_eq!(state.detail.pending_episode.as_deref(), Some("show:2:3"));
    assert_eq!(state.detail.meta.as_ref().unwrap().id, "show");
    assert!(
        state
            .detail
            .request
            .error
            .as_ref()
            .unwrap()
            .contains("Choose another stream")
    );
    assert!(state.library.stream_candidate.is_none());
}
