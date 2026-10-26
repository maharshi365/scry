//! Playback commands and captured media identity.
use super::super::{Page, State};
use crate::services::{Control, Source};
use scry_addons::Stream;
use scry_app::platform::VideoHost;
use scry_playback::telemetry::Sample;

impl State {
    pub(in crate::ui) fn stop(&mut self) {
        self.set_fullscreen(false);
        self.finish_library_progress(false);
        self.library.playing = None;
        self.refresh_library();
        self.player.generation += 1;
        self.player.playing = false;
        self.player.ready = false;
        self.player.seek_preview = None;
        self.player.interaction.reset();
        self.player.sample.cache_s = None;
        self.host.hide();
        self.send(Control::Stop(self.player.generation));
    }

    pub(super) fn send(&mut self, control: Control) {
        if let Some(services) = &self.services
            && let Err(error) = services.control(control)
        {
            self.error = Some(error.to_string());
        }
    }

    pub(in crate::ui) fn command(&mut self, args: &[&str]) {
        self.send(Control::Command(
            self.player.generation,
            args.iter().map(|s| (*s).to_owned()).collect(),
        ));
    }

    pub(in crate::ui) fn play(&mut self, stream: Stream, title: String) {
        if let Err(error) = stream.validate_playback() {
            self.error = Some(error.to_string());
            return;
        }
        self.play_media_source(Source::Stream(stream), title);
    }

    pub(in crate::ui) fn play_media_source(&mut self, source: Source, title: String) {
        self.save_library_progress();
        self.library.playing = self.library_media();
        self.library.watched_pending = 0.0;
        self.library.observed = None;
        self.library.resume_position = self
            .library
            .snapshot
            .as_ref()
            .and_then(|v| v.selected_state.as_ref())
            .filter(|v| {
                self.library
                    .playing
                    .as_ref()
                    .is_some_and(|m| m.key == v.key)
            })
            .and_then(|v| v.progress)
            .map(|p| p.position_seconds)
            .filter(|p| *p > 0.0);
        self.library.next_episode = self.next_released_episode();
        self.library.last_sample = None;
        self.library.saved_sample = None;
        self.library.last_save = std::time::Instant::now();
        self.player.meta_id = self
            .detail
            .as_ref()
            .map(|meta| (meta.kind.clone(), meta.id.clone()));
        self.player.video_id = self.selected_video_id.clone();
        self.play_source(source, title);
    }

    pub(in crate::ui) fn play_source(&mut self, source: Source, title: String) {
        self.error = None;
        if let Some(error) = self.host_error.lock().ok().and_then(|e| e.clone()) {
            self.error = Some(error);
            return;
        }
        match self.host.target() {
            Ok(target) => {
                self.host.hide();
                if self.navigation.page != Page::Guide {
                    self.remember();
                }
                self.player.generation += 1;
                self.player.playing = true;
                self.player.ready = false;
                let meta = self.detail.as_ref().filter(|meta| {
                    self.player
                        .meta_id
                        .as_ref()
                        .is_some_and(|(kind, id)| kind == &meta.kind && id == &meta.id)
                });
                self.player.background = meta.and_then(|meta| meta.background.clone());
                self.player.logo = meta.and_then(|meta| meta.logo.clone());
                if self.navigation.page != Page::Guide {
                    self.navigation.page = Page::Player;
                }
                self.player.title = title;
                self.player.sample = Sample::default();
                self.player.interaction.reset();
                self.player.seek_preview = None;
                self.player.muted = false;
                self.player.report = None;
                self.status = "Loading…".into();
                self.send(Control::Play(self.player.generation, target, source));
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(in crate::ui) fn play_direct(&mut self) {
        let stream =
            serde_json::from_value::<Stream>(serde_json::json!({"url":self.direct.text.trim()}));
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        if let Err(error) = stream.validate_playback() {
            self.error = Some(error.to_string());
            return;
        }
        self.save_library_progress();
        self.library.playing = None;
        self.library.next_episode = None;
        self.library.resume_position = None;
        self.player.meta_id = None;
        self.player.video_id = None;
        self.play_source(Source::Stream(stream), "Direct stream".into());
    }
}

impl State {
    fn next_released_episode(&self) -> Option<scry_library::Media> {
        let meta = self.detail.as_ref().filter(|m| m.kind == "series")?;
        let current = self.selected_video_id.as_ref()?;
        let mut videos: Vec<_> = meta
            .videos
            .iter()
            .filter(|v| v.season.is_some_and(|s| s > 0) && v.episode.is_some())
            .collect();
        videos.sort_by_key(|v| (v.season, v.episode));
        let index = videos.iter().position(|v| &v.id == current)?;
        let next = *videos.get(index + 1)?;
        let released = next.released.as_deref()?;
        let is_released = chrono::DateTime::parse_from_rfc3339(released).map_or_else(
            |_| {
                super::super::calendar::release_date(released)
                    .is_some_and(|d| d <= chrono::Local::now().date_naive())
            },
            |d| d <= chrono::Utc::now(),
        );
        if !is_released {
            return None;
        }
        let parent = super::super::library_view::key(
            &self.detail_namespace,
            scry_library::MediaKind::Series,
            &meta.id,
        );
        Some(scry_library::Media {
            key: super::super::library_view::key(
                &parent.namespace,
                scry_library::MediaKind::Episode,
                &next.id,
            ),
            title: format!("{} · {}", meta.name, next.title),
            parent: Some(parent),
        })
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Episode fixtures and selected next episode must exist"
)]
mod tests {
    #[test]
    fn resume_uses_episode_order_and_requires_a_known_released_next_episode() {
        let mut s = super::super::super::app::init();
        s.detail_namespace = "provider".into();
        s.detail = Some(
            serde_json::from_value(serde_json::json!({
                "id":"tt123","type":"series","name":"Show","videos":[
                    {"id":"show:1:2","season":1,"episode":2,"released":"2000-01-02T00:00:00Z"},
                    {"id":"show:1:1","season":1,"episode":1,"released":"2000-01-01T00:00:00Z"}
                ]
            }))
            .unwrap(),
        );
        s.selected_video_id = Some("show:1:1".into());
        assert_eq!(s.next_released_episode().unwrap().key.id, "show:1:2");
        assert_eq!(s.next_released_episode().unwrap().key.namespace, "imdb");
        s.detail.as_mut().unwrap().videos[0].released = None;
        assert!(s.next_released_episode().is_none());
        s.detail.as_mut().unwrap().videos[0].released = Some("2100-01-02T00:00:00Z".into());
        assert!(s.next_released_episode().is_none());
    }
}
