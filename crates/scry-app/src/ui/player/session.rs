//! Playback commands and captured media identity.
use super::super::{Page, State};
use crate::services::{Control, Source};
use scry_addons::Stream;
use scry_app::platform::VideoHost;
use scry_playback::telemetry::Sample;

impl State {
    pub(in crate::ui) fn stop(&mut self) {
        self.set_fullscreen(false);
        self.save_library_progress();
        self.library.playing = None;
        self.refresh_library();
        self.player.generation += 1;
        self.player.playing = false;
        self.player.ready = false;
        self.player.seek_preview = None;
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
        self.save_library_progress();
        self.library.playing = self.library_media();
        self.library.last_sample = None;
        self.library.saved_sample = None;
        self.library.last_save = std::time::Instant::now();
        self.player.meta_id = self
            .detail
            .as_ref()
            .map(|meta| (meta.kind.clone(), meta.id.clone()));
        self.player.video_id = self.selected_video_id.clone();
        self.play_source(Source::Stream(stream), title);
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
                self.remember();
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
                self.navigation.page = Page::Player;
                self.player.title = title;
                self.player.sample = Sample::default();
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
        self.player.meta_id = None;
        self.player.video_id = None;
        self.play_source(Source::Stream(stream), "Direct stream".into());
    }
}
