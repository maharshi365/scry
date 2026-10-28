use super::super::{
    Page, State,
    design::{
        icons::{AUDIO, CLOSE},
        tokens,
    },
    primitives::{
        button::{Button, Variant},
        hover_overlay::{self, HoverOverlay},
        surface::scroll,
        text::{heading, label},
    },
};
use rune::{El, col, row};
use scry_playback::{
    audio::{Snapshot, Track},
    subtitles::language_label,
};

#[derive(Default)]
pub(in crate::ui) struct Audio {
    pub overlay: HoverOverlay,
    pub tracks: Vec<Track>,
    pub selected: Option<i64>,
}

impl Audio {
    fn selection_command(&self, id: i64) -> Option<[String; 3]> {
        self.tracks
            .iter()
            .any(|track| track.id == id)
            .then(|| ["set".into(), "aid".into(), id.to_string()])
    }
}

fn overlay(s: &mut State) -> &mut HoverOverlay {
    &mut s.player.audio.overlay
}

pub(super) fn trigger(s: &State) -> El {
    let trigger = Button::new("audio", "Audio tracks")
        .variant(Variant::Ghost)
        .icon(&AUDIO)
        .selected(s.player.audio.overlay.is_open())
        .enabled(s.player.playing)
        .on_press_tx(|s: &mut State, tx| {
            s.show_audio();
            tx.instant();
        });
    hover_overlay::trigger(trigger, s.player.playing, overlay)
}

pub(in crate::ui) fn panel(s: &State) -> Option<El> {
    if !s.player.playing
        || !s.player.audio.overlay.is_open()
        || !(s.navigation.page == Page::Player
            || s.navigation.page == Page::Guide && s.player.fullscreen)
    {
        return None;
    }
    let mut tracks = col().w_full().gap(4.0);
    for track in &s.player.audio.tracks {
        let id = track.id;
        tracks = tracks.child(
            Button::new(format!("audio-track-{id}"), track_label(track))
                .variant(Variant::Ghost)
                .selected(s.player.audio.selected == Some(id))
                .on_press(move |s: &mut State| s.select_audio(id))
                .w_full()
                .min_h(38.0)
                .border(0.0, tokens::transparent_edge())
                .justify_start(),
        );
    }
    if s.player.audio.tracks.is_empty() {
        tracks = tracks.child(label("No audio tracks found."));
    }
    let content = col()
        .w_full()
        .gap(10.0)
        .child(
            row()
                .w_full()
                .center_y()
                .child(heading("Audio tracks").grow(1.0))
                .child(
                    Button::new("audio-close", "Close")
                        .variant(Variant::Ghost)
                        .icon(&CLOSE)
                        .on_press(|s: &mut State| s.player.audio.overlay.dismiss()),
                ),
        )
        .child(
            col()
                .w_full()
                .padding(4.0)
                .radius(8.0)
                .border(1.0, tokens::edge())
                .child(
                    scroll("audio-tracks", tracks)
                        .h(168.0)
                        .min_h(168.0)
                        .grow(0.0),
                ),
        );
    Some(
        hover_overlay::panel("player-audio-overlay", "audio", 420.0, content, overlay)
            .focus_group()
            .focus_return("audio")
            .on_layout(|s: &mut State, rect| {
                s.player.audio.overlay.rect = Some(rect);
                s.cover_player();
            }),
    )
}

fn track_label(track: &Track) -> String {
    let language = if track.language.trim().is_empty() {
        format!("Track {}", track.id)
    } else {
        language_label(&track.language)
    };
    if track.title.trim().is_empty() {
        language
    } else {
        format!("{language} · {}", track.title)
    }
}

impl State {
    pub(in crate::ui) fn show_audio(&mut self) {
        self.player.subtitles.overlay.dismiss();
        self.player.stats_overlay.dismiss();
        self.player.audio.overlay.pin();
        self.player.interaction.reveal(std::time::Instant::now());
    }

    pub(in crate::ui) fn apply_audio_snapshot(&mut self, generation: u64, snapshot: Snapshot) {
        if generation != self.player.generation || !self.player.playing {
            return;
        }
        self.player.audio.selected = snapshot
            .selected
            .filter(|id| snapshot.tracks.iter().any(|t| t.id == *id));
        self.player.audio.tracks = snapshot.tracks;
    }

    pub(in crate::ui) fn select_audio(&mut self, id: i64) {
        if !self.player.playing {
            return;
        }
        if let Some(args) = self.player.audio.selection_command(id) {
            self.command(&args.each_ref().map(String::as_str));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selecting_the_only_track_sets_its_id_and_never_cycles_or_disables_audio() {
        let audio = Audio {
            tracks: vec![Track {
                id: 7,
                language: String::new(),
                title: String::new(),
            }],
            selected: Some(7),
            ..Audio::default()
        };
        assert_eq!(
            audio.selection_command(7),
            Some(["set".into(), "aid".into(), "7".into()])
        );
        for id in [-1, 0, 8] {
            assert_eq!(audio.selection_command(id), None);
        }
    }

    #[test]
    fn track_labels_preserve_language_and_title_with_a_fallback_for_missing_metadata() {
        let mut track = Track {
            id: 7,
            language: String::new(),
            title: String::new(),
        };
        assert_eq!(track_label(&track), "Track 7");
        track.title = "Commentary".into();
        assert_eq!(track_label(&track), "Track 7 · Commentary");
        track.language = "eng".into();
        assert_eq!(track_label(&track), "English · Commentary");
    }
}
