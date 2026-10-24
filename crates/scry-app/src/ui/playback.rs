//! Playback commands and captured media identity.
use super::{Page, State};
use crate::services::{Control, Source};
use scry_addons::Stream;
use scry_app::platform::VideoHost;
use scry_playback::telemetry::Sample;

impl State {
    pub(super) fn stop(&mut self) {
        self.save_library_progress();
        self.library.playing = None;
        self.refresh_library();
        self.play_id += 1;
        self.playing = false;
        self.host.hide();
        self.send(Control::Stop(self.play_id));
    }

    pub(super) fn send(&mut self, control: Control) {
        if let Some(services) = &self.services
            && let Err(error) = services.control(control)
        {
            self.error = Some(error.to_string());
        }
    }

    pub(super) fn command(&mut self, args: &[&str]) {
        self.send(Control::Command(
            self.play_id,
            args.iter().map(|s| (*s).to_owned()).collect(),
        ));
    }

    pub(super) fn play(&mut self, stream: Stream, title: String) {
        if let Err(error) = stream.validate_playback() {
            self.error = Some(error.to_string());
            return;
        }
        self.save_library_progress();
        self.library.playing = self.library_media();
        self.library.last_sample = None;
        self.library.saved_sample = None;
        self.library.last_save = std::time::Instant::now();
        self.playing_meta_id = self
            .detail
            .as_ref()
            .map(|meta| (meta.kind.clone(), meta.id.clone()));
        self.playing_video_id = self.selected_video_id.clone();
        self.play_source(Source::Stream(stream), title);
    }

    pub(super) fn play_source(&mut self, source: Source, title: String) {
        self.error = None;
        if let Some(error) = self.host_error.lock().ok().and_then(|e| e.clone()) {
            self.error = Some(error);
            return;
        }
        match self.host.target() {
            Ok(target) => {
                self.remember();
                self.play_id += 1;
                self.playing = true;
                self.page = Page::Player;
                self.playing_title = title;
                self.sample = Sample::default();
                self.report = None;
                self.status = "Opening stream · torrent metadata can take up to 90 seconds…".into();
                self.send(Control::Play(self.play_id, target, source));
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(super) fn play_direct(&mut self) {
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
        self.playing_meta_id = None;
        self.playing_video_id = None;
        self.play_source(Source::Stream(stream), "Direct stream".into());
    }
}
