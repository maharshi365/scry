use super::*;
use scry_library::{MediaKey, MediaKind};

fn pending() -> State {
    let mut state = crate::ui::app::init();
    state.navigation.page = Page::Detail;
    state.detail.namespace = "provider".into();
    state.detail.meta = serde_json::from_value(serde_json::json!({
        "id": "show", "type": "series", "name": "Show"
    }))
    .ok();
    state.library.resume_request = Some(ResumeRequest {
        generation: state.detail.generation,
        media: Media {
            key: MediaKey::new("provider", MediaKind::Episode, "show:1:2"),
            title: "Episode two".into(),
            parent: Some(MediaKey::new("provider", MediaKind::Series, "show")),
        },
        progress: Progress {
            position_seconds: 45.0,
            duration_seconds: 100.0,
        },
        stream: StreamLookup::Pending,
    });
    state
}

#[test]
fn remembered_stream_waits_for_metadata_then_targets_the_exact_episode() {
    let mut state = pending();
    state.detail.request.loading = true;
    state.apply_saved_stream(Event::SavedStream {
        generation: state.detail.generation,
        stream: Some(
            r#"{"infoHash":"0123456789abcdef0123456789abcdef01234567","fileIdx":2}"#.into(),
        ),
    });
    assert!(state.library.playing.is_none());
    assert!(
        matches!(state.library.resume_request.as_ref().map(|request| &request.stream), Some(StreamLookup::Ready(Some(stream))) if stream.file_idx == Some(2))
    );
    state.detail.request.loading = false;
    state.try_resume_stream();
    assert_eq!(state.detail.episodes.selected.as_deref(), Some("show:1:2"));
    assert_eq!(
        state
            .library
            .playing
            .as_ref()
            .map(|media| media.key.id.as_str()),
        Some("show:1:2")
    );
    assert!(state.library.resume_request.is_none());
}

#[test]
fn missing_invalid_and_stale_streams_never_start_playback() {
    for stream in [
        None,
        Some("not json".into()),
        Some(r#"{"url":"file:///private"}"#.into()),
    ] {
        let mut state = pending();
        state.apply_saved_stream(Event::SavedStream {
            generation: state.detail.generation,
            stream,
        });
        assert!(state.library.playing.is_none());
        assert!(state.library.resume_request.is_none());
        assert_eq!(state.navigation.page, Page::Detail);
    }
    let mut state = pending();
    state.detail.generation += 1;
    state.apply_saved_stream(Event::SavedStream {
        generation: state.detail.generation - 1,
        stream: Some(r#"{"url":"https://example.com/video"}"#.into()),
    });
    assert!(state.library.playing.is_none());
    assert!(state.library.resume_request.is_none());
}
