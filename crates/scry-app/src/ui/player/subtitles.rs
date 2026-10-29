use super::super::{
    Page, State,
    design::icons::{ADD, CAPTIONS, CLOSE},
    primitives::{
        button::{Button, Variant, tab},
        hover_overlay::{self, HoverOverlay},
        surface::scroll,
        text::{heading, label},
    },
};
use rune::{El, col, row};
use scry_app::platform::VideoHost;
use scry_playback::subtitles::{
    Preferences, SIZES, Snapshot, Track, language_code, language_label,
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::ui) enum Slot {
    #[default]
    Primary,
    Secondary,
}

impl Slot {
    pub fn index(self) -> usize {
        usize::from(self == Self::Secondary)
    }
    fn property(self) -> &'static str {
        if self == Self::Primary {
            "sid"
        } else {
            "secondary-sid"
        }
    }
}

#[derive(Default)]
pub(in crate::ui) struct SettingsUi {
    pub busy: bool,
    pub error: Option<String>,
    pub language_menu: Option<Slot>,
}

pub(in crate::ui) struct Subtitles {
    pub overlay: HoverOverlay,
    pub tracks: Vec<Track>,
    pub selected: [Option<i64>; 2],
    pub slot: Slot,
    pub defaults: Preferences,
    pub sources: Vec<crate::services::ExternalSubtitle>,
    pub searching: bool,
    pub source_error: bool,
    pub error: Option<String>,
    pending: Option<(Slot, u64, String, String)>,
    files: Vec<std::sync::Arc<crate::services::SubtitleFile>>,
    request: u64,
    manual: [bool; 2],
    previous: [Option<i64>; 2],
    delay: [i32; 2],
    size: u16,
    position: [u8; 2],
}

impl Default for Subtitles {
    fn default() -> Self {
        Self {
            overlay: HoverOverlay::default(),
            tracks: Vec::new(),
            selected: [None; 2],
            slot: Slot::Primary,
            defaults: Preferences::default(),
            sources: Vec::new(),
            searching: false,
            source_error: false,
            error: None,
            pending: None,
            files: Vec::new(),
            request: 0,
            manual: [false; 2],
            previous: [None; 2],
            delay: [0; 2],
            size: 100,
            position: [95, 5],
        }
    }
}

fn overlay(s: &mut State) -> &mut HoverOverlay {
    &mut s.player.subtitles.overlay
}

pub(in crate::ui) fn scene(s: &mut State) {
    s.navigation.page = Page::Player;
    s.config_busy = false;
    s.player.playing = true;
    s.player.ready = true;
    s.player.title = "Subtitle controls".into();
    s.apply_subtitle_snapshot(
        s.player.generation,
        Snapshot {
            tracks: [
                (1, "eng", ""),
                (2, "eng", "SDH"),
                (3, "ara", ""),
                (4, "spa", ""),
            ]
            .into_iter()
            .map(|(id, language, title)| Track {
                id,
                language: language.into(),
                title: title.into(),
                codec: "subrip".into(),
                external: false,
            })
            .collect(),
            ..Snapshot::default()
        },
    );
    s.show_subtitles();
}

pub(super) fn trigger(s: &State) -> El {
    let trigger = Button::new("subtitles", "Subtitles (S) · Toggle (C)")
        .variant(Variant::Ghost)
        .icon(&CAPTIONS)
        .selected(s.player.subtitles.overlay.is_open())
        .enabled(s.player.playing)
        .on_press_tx(|s: &mut State, tx| {
            s.show_subtitles();
            tx.instant();
        });
    hover_overlay::trigger(trigger, s.player.playing, overlay)
}

pub(in crate::ui) fn panel(s: &State) -> Option<El> {
    if !s.player.playing
        || !s.player.subtitles.overlay.is_open()
        || !(s.navigation.page == Page::Player
            || s.navigation.page == Page::Guide && s.player.fullscreen)
    {
        return None;
    }
    let subtitles = &s.player.subtitles;
    let slot = subtitles.slot;
    let selected = subtitles.selected[slot.index()];
    let active = subtitles.tracks.iter().find(|t| Some(t.id) == selected);
    let appearance =
        active.is_some_and(|t| !(t.bitmap() || t.styled() && subtitles.defaults.ass_styles));
    let content = col()
        .w_full()
        .gap(10.0)
        .child(panel_header())
        .child(
            row()
                .key("subtitle-tabs")
                .role("tablist")
                .label("Subtitle track")
                .gap(4.0)
                .child(tab(
                    "subtitle-slot-primary",
                    "Primary",
                    slot == Slot::Primary,
                    |s: &mut State| s.player.subtitles.slot = Slot::Primary,
                ))
                .child(tab(
                    "subtitle-slot-secondary",
                    "Secondary",
                    slot == Slot::Secondary,
                    |s: &mut State| s.player.subtitles.slot = Slot::Secondary,
                )),
        )
        .child(
            super::super::primitives::surface::list()
                .key("subtitle-languages")
                .child(
                    scroll("subtitle-tracks", track_list(s))
                        .h(168.0)
                        .min_h(168.0)
                        .grow(0.0),
                ),
        )
        .child(stepper(
            "delay",
            "Timing",
            format!("{:+.1} s", f64::from(subtitles.delay[slot.index()]) / 10.0),
            selected.is_some(),
            State::adjust_subtitle_delay,
        ))
        .child(stepper(
            "size",
            "Size",
            format!("{}%", subtitles.size),
            appearance,
            State::adjust_subtitle_size,
        ))
        .child(stepper(
            "position",
            "Position",
            format!("{}%", subtitles.position[slot.index()]),
            appearance,
            State::adjust_subtitle_position,
        ))
        .child(label(if active.is_some_and(Track::bitmap) {
            "Bitmap subtitles have a fixed appearance."
        } else if !appearance && active.is_some() {
            "ASS styles control this track’s appearance."
        } else {
            "Size applies to both tracks."
        }))
        .child(panel_actions());
    Some(
        hover_overlay::panel(
            "player-subtitles-overlay",
            "subtitles",
            420.0,
            content,
            overlay,
        )
        .focus_group()
        .focus_return("subtitles")
        .on_layout(|s: &mut State, rect| {
            s.player.subtitles.overlay.rect = Some(rect);
            s.cover_player();
        }),
    )
}

fn panel_header() -> El {
    row()
        .w_full()
        .center_y()
        .child(heading("Subtitles").grow(1.0))
        .child(
            Button::new("subtitle-close", "Close")
                .variant(Variant::Ghost)
                .icon(&CLOSE)
                .on_press(|s: &mut State| s.player.subtitles.overlay.dismiss()),
        )
}

fn panel_actions() -> El {
    row()
        .w_full()
        .justify_end()
        .center_y()
        .gap(8.0)
        .child(
            Button::new("subtitle-reset", "Reset")
                .variant(Variant::Ghost)
                .on_press(State::reset_subtitle_adjustments),
        )
        .child(
            Button::new("subtitle-load-local", "Load subtitle file")
                .variant(Variant::Ghost)
                .icon(&ADD)
                .on_press(State::load_local_subtitle),
        )
}

fn track_list(s: &State) -> El {
    let subtitles = &s.player.subtitles;
    let selected = subtitles.selected[subtitles.slot.index()];
    let mut tracks = col().w_full().gap(4.0).child(
        Button::new("subtitle-track-off", "Off")
            .list_item()
            .selected(selected.is_none())
            .on_press(|s: &mut State| s.select_subtitle(None)),
    );
    for track in &subtitles.tracks {
        let id = track.id;
        let language = language_label(&track.language);
        let source = if track.external {
            "External"
        } else {
            "Embedded"
        };
        let title = if track.title.is_empty() {
            language
        } else {
            format!("{language} · {}", track.title)
        };
        let badge = if track.styled() {
            " · ASS"
        } else if track.bitmap() {
            " · Bitmap"
        } else {
            ""
        };
        tracks = tracks.child(
            Button::new(
                format!("subtitle-track-{id}"),
                format!("{title} · {source}{badge}"),
            )
            .list_item()
            .selected(selected == Some(id))
            .on_press(move |s: &mut State| s.select_subtitle(Some(id))),
        );
    }
    external_tracks(s, tracks)
}

fn external_tracks(s: &State, mut tracks: El) -> El {
    let subtitles = &s.player.subtitles;
    for (index, source) in subtitles.sources.iter().enumerate() {
        let name = if source.subtitle.label.is_empty() {
            language_label(&source.subtitle.lang)
        } else {
            format!(
                "{} · {}",
                language_label(&source.subtitle.lang),
                source.subtitle.label
            )
        };
        tracks = tracks.child(
            Button::new(
                format!("subtitle-source-{index}"),
                format!("{name} · {}", source.provider),
            )
            .list_item()
            .enabled(subtitles.pending.is_none() && subtitles.files.len() < 16)
            .on_press(move |s: &mut State| s.load_external_subtitle(index)),
        );
    }
    if subtitles.searching {
        tracks = tracks.child(label("Searching subtitle providers…"));
    }
    if subtitles.pending.is_some() {
        tracks = tracks.child(label("Loading subtitle…"));
    }
    if subtitles.tracks.is_empty() && subtitles.sources.is_empty() && !subtitles.searching {
        tracks = tracks.child(label(
            "No subtitles found. Add a subtitle provider in Add-ons.",
        ));
    }
    if subtitles.source_error {
        tracks = tracks.child(label("A subtitle provider did not respond."));
    }
    if let Some(error) = &subtitles.error {
        tracks = tracks.child(label(error.clone()));
    }
    tracks
}

fn stepper(
    key: &str,
    title: &str,
    value: String,
    enabled: bool,
    change: fn(&mut State, i32),
) -> El {
    super::super::primitives::stepper::stepper(
        &format!("subtitle-{key}"),
        title,
        value,
        enabled,
        change,
    )
}

impl State {
    fn load_local_subtitle(&mut self) {
        if !self.player.playing {
            return;
        }
        let path = match self.host.pick_subtitle() {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(_) => {
                self.player.subtitles.error = Some("Cannot open the subtitle file picker".into());
                return;
            }
        };
        let supported = path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
            ["srt", "vtt", "ass", "ssa"]
                .iter()
                .any(|extension| s.eq_ignore_ascii_case(extension))
        });
        if !supported {
            self.player.subtitles.error = Some("Choose an SRT, VTT, ASS, or SSA file".into());
            return;
        }
        self.player.subtitles.request += 1;
        let request = self.player.subtitles.request;
        let title = format!("Local · #{request}");
        self.player.subtitles.manual[self.player.subtitles.slot.index()] = true;
        self.player.subtitles.pending =
            Some((self.player.subtitles.slot, request, title, String::new()));
        self.send_subtitle_load(self.player.generation, request, path);
    }

    fn send_subtitle_load(
        &mut self,
        generation: u64,
        request: u64,
        path: std::path::PathBuf,
    ) -> bool {
        if generation != self.player.generation || !self.player.playing {
            return false;
        }
        let Some((_, pending, title, language)) = &self.player.subtitles.pending else {
            return false;
        };
        if request != *pending {
            return false;
        }
        let control = crate::services::Control::LoadSubtitle {
            generation,
            request,
            path,
            title: title.clone(),
            language: language.clone(),
        };
        let error = match self
            .services
            .as_ref()
            .map(|services| services.control(control))
        {
            Some(Ok(())) => return true,
            Some(Err(error)) => error.to_string(),
            None => "Player is closed".into(),
        };
        self.apply_subtitle_load_failed(generation, request, error);
        false
    }

    pub(in crate::ui) fn apply_subtitle_load_failed(
        &mut self,
        generation: u64,
        request: u64,
        error: String,
    ) {
        if generation == self.player.generation
            && self.player.playing
            && self
                .player
                .subtitles
                .pending
                .as_ref()
                .is_some_and(|(_, pending, ..)| *pending == request)
        {
            self.player.subtitles.pending = None;
            self.player.subtitles.error = Some(error);
        }
    }

    pub(in crate::ui) fn show_subtitles(&mut self) {
        self.player.audio.overlay.dismiss();
        self.player.stats_overlay.dismiss();
        self.player.subtitles.overlay.pin();
        self.player.interaction.reveal(std::time::Instant::now());
    }

    pub(in crate::ui) fn apply_subtitle_snapshot(&mut self, generation: u64, snapshot: Snapshot) {
        if generation != self.player.generation || !self.player.playing {
            return;
        }
        self.player.subtitles.tracks = snapshot.tracks;
        if let Some((slot, _, title, _)) = &self.player.subtitles.pending
            && let Some(id) = self
                .player
                .subtitles
                .tracks
                .iter()
                .find(|t| t.title == *title)
                .map(|t| t.id)
        {
            let slot = *slot;
            self.player.subtitles.pending = None;
            self.set_subtitle(slot, Some(id));
        }
        if self.subtitle_settings.busy {
            return;
        }
        for slot in [Slot::Primary, Slot::Secondary] {
            let index = slot.index();
            if self.player.subtitles.manual[index] {
                continue;
            }
            let language = if slot == Slot::Primary {
                &self.player.subtitles.defaults.primary
            } else {
                &self.player.subtitles.defaults.secondary
            };
            let other = self.player.subtitles.selected[1 - index];
            let track = language
                .as_ref()
                .and_then(|lang| {
                    self.player.subtitles.tracks.iter().find(|track| {
                        language_code(&track.language) == *lang && Some(track.id) != other
                    })
                })
                .map(|track| track.id);
            if self.player.subtitles.selected[index] != track {
                self.set_subtitle(slot, track);
            }
        }
        self.auto_external_subtitle();
    }

    fn set_subtitle(&mut self, slot: Slot, id: Option<i64>) {
        self.player.subtitles.selected[slot.index()] = id;
        self.player.subtitles.delay[slot.index()] = 0;
        self.command(&[
            "set",
            slot.property(),
            &id.map_or_else(|| "no".into(), |id| id.to_string()),
        ]);
        let delay = if slot == Slot::Primary {
            "sub-delay"
        } else {
            "secondary-sub-delay"
        };
        self.command(&["set", delay, "0"]);
    }

    pub(in crate::ui) fn select_subtitle(&mut self, id: Option<i64>) {
        if !self.player.playing
            || id.is_some_and(|id| !self.player.subtitles.tracks.iter().any(|t| t.id == id))
        {
            return;
        }
        let slot = self.player.subtitles.slot;
        if self
            .player
            .subtitles
            .pending
            .as_ref()
            .is_some_and(|(pending_slot, ..)| *pending_slot == slot)
        {
            self.player.subtitles.pending = None;
        }
        let index = slot.index();
        let other = if slot == Slot::Primary {
            Slot::Secondary
        } else {
            Slot::Primary
        };
        if id.is_some() && self.player.subtitles.selected[other.index()] == id {
            self.player.subtitles.manual[other.index()] = true;
            self.set_subtitle(other, None);
        }
        self.player.subtitles.manual[index] = true;
        self.set_subtitle(slot, id);
    }

    pub(in crate::ui) fn toggle_subtitles(&mut self) {
        self.player.subtitles.pending = None;
        if self.player.subtitles.selected.iter().any(Option::is_some) {
            self.player.subtitles.previous = self.player.subtitles.selected;
            self.player.subtitles.manual = [true; 2];
            self.set_subtitle(Slot::Primary, None);
            self.set_subtitle(Slot::Secondary, None);
        } else {
            let previous = self.player.subtitles.previous;
            for slot in [Slot::Primary, Slot::Secondary] {
                let id = previous[slot.index()]
                    .filter(|id| self.player.subtitles.tracks.iter().any(|t| t.id == *id));
                self.player.subtitles.manual[slot.index()] = true;
                self.set_subtitle(slot, id);
            }
            if self.player.subtitles.selected == [None; 2] {
                let id = self.player.subtitles.tracks.first().map(|t| t.id);
                self.set_subtitle(Slot::Primary, id);
                self.player.subtitles.manual[0] = true;
            }
        }
    }

    pub(in crate::ui) fn adjust_subtitle_delay(&mut self, direction: i32) {
        let index = self.player.subtitles.slot.index();
        if self.player.subtitles.selected[index].is_none() {
            return;
        }
        self.player.subtitles.delay[index] = self.player.subtitles.delay[index]
            .saturating_add(direction)
            .clamp(-36000, 36000);
        let key = if index == 0 {
            "sub-delay"
        } else {
            "secondary-sub-delay"
        };
        self.command(&[
            "set",
            key,
            &(f64::from(self.player.subtitles.delay[index]) / 10.0).to_string(),
        ]);
    }

    pub(in crate::ui) fn adjust_subtitle_size(&mut self, direction: i32) {
        let current = self.player.subtitles.size;
        let next = if direction > 0 {
            SIZES.iter().copied().find(|v| *v > current)
        } else {
            SIZES.iter().rev().copied().find(|v| *v < current)
        };
        if let Some(next) = next {
            self.player.subtitles.size = next;
            self.command(&["set", "sub-scale", &(f64::from(next) / 100.0).to_string()]);
        }
    }

    fn adjust_subtitle_position(&mut self, direction: i32) {
        let index = self.player.subtitles.slot.index();
        self.player.subtitles.position[index] = self.player.subtitles.position[index]
            .saturating_add_signed(if direction > 0 { 1 } else { -1 })
            .min(100);
        let key = if index == 0 {
            "sub-pos"
        } else {
            "secondary-sub-pos"
        };
        self.command(&[
            "set",
            key,
            &self.player.subtitles.position[index].to_string(),
        ]);
    }

    pub(in crate::ui) fn reset_subtitle_adjustments(&mut self) {
        self.player.subtitles.delay = [0; 2];
        self.command(&["set", "sub-delay", "0"]);
        self.command(&["set", "secondary-sub-delay", "0"]);
        self.apply_subtitle_appearance();
    }

    pub(in crate::ui) fn apply_subtitle_appearance(&mut self) {
        self.player.subtitles.size = self.player.subtitles.defaults.size;
        self.player.subtitles.position = [self.player.subtitles.defaults.position, 5];
        self.send(crate::services::Control::SubtitleAppearance(
            self.player.generation,
            self.player.subtitles.defaults.clone(),
        ));
    }

    pub(in crate::ui) fn save_subtitle_preferences(&mut self, preferences: Preferences) {
        if self.subtitle_settings.busy || preferences.validate().is_err() {
            return;
        }
        self.subtitle_preferences = preferences.clone();
        self.subtitle_settings.error = None;
        if let Some(services) = &self.services {
            match services.save_subtitle_preferences(preferences) {
                Ok(()) => self.subtitle_settings.busy = true,
                Err(error) => self.subtitle_settings.error = Some(error.to_string()),
            }
        }
    }

    pub(in crate::ui) fn apply_subtitle_sources(
        &mut self,
        generation: u64,
        sources: Vec<crate::services::ExternalSubtitle>,
        done: bool,
        failed: bool,
    ) {
        if generation != self.player.generation || !self.player.playing {
            return;
        }
        let remaining = 100_usize.saturating_sub(self.player.subtitles.sources.len());
        self.player
            .subtitles
            .sources
            .extend(sources.into_iter().take(remaining));
        self.player.subtitles.searching = !done;
        self.player.subtitles.source_error |= failed;
    }

    pub(in crate::ui) fn load_external_subtitle(&mut self, index: usize) {
        if !self.player.playing {
            return;
        }
        let Some(source) = self.player.subtitles.sources.get(index) else {
            return;
        };
        if self.player.subtitles.files.len() >= 16 {
            return;
        }
        let source = source.clone();
        self.player.subtitles.request += 1;
        let request = self.player.subtitles.request;
        let title = format!(
            "{} · {} · #{request}",
            source.provider,
            if source.subtitle.label.is_empty() {
                language_label(&source.subtitle.lang)
            } else {
                source.subtitle.label.clone()
            }
        );
        self.player.subtitles.manual[self.player.subtitles.slot.index()] = true;
        self.player.subtitles.pending = Some((
            self.player.subtitles.slot,
            request,
            title,
            source.subtitle.lang.clone(),
        ));
        self.player.subtitles.error = None;
        if let Some(services) = &mut self.services {
            services.subtitle_file(self.player.generation, request, source.subtitle);
        } else {
            self.apply_subtitle_load_failed(
                self.player.generation,
                request,
                "Player is closed".into(),
            );
        }
    }

    fn auto_external_subtitle(&mut self) {
        if !self.player.ready
            || self.player.subtitles.pending.is_some()
            || self.player.subtitles.searching
        {
            return;
        }
        for slot in [Slot::Primary, Slot::Secondary] {
            if self.player.subtitles.manual[slot.index()]
                || self.player.subtitles.selected[slot.index()].is_some()
            {
                continue;
            }
            let language = if slot == Slot::Primary {
                &self.player.subtitles.defaults.primary
            } else {
                &self.player.subtitles.defaults.secondary
            };
            if let Some(index) = language.as_ref().and_then(|language| {
                self.player
                    .subtitles
                    .sources
                    .iter()
                    .position(|s| language_code(&s.subtitle.lang) == *language)
            }) {
                let active = self.player.subtitles.slot;
                self.player.subtitles.slot = slot;
                self.load_external_subtitle(index);
                self.player.subtitles.slot = active;
                break;
            }
        }
    }

    pub(in crate::ui) fn apply_subtitle_file(
        &mut self,
        generation: u64,
        request: u64,
        file: Result<std::sync::Arc<crate::services::SubtitleFile>, String>,
    ) {
        if generation != self.player.generation || !self.player.playing {
            return;
        }
        let Some((_, pending, ..)) = &self.player.subtitles.pending else {
            return;
        };
        if request != *pending {
            return;
        }
        match file {
            Ok(file) => {
                if self.send_subtitle_load(generation, request, file.path.clone()) {
                    self.player.subtitles.files.push(file);
                }
            }
            Err(error) => {
                self.apply_subtitle_load_failed(generation, request, error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, language: &str) -> Track {
        Track {
            id,
            language: language.into(),
            title: format!("Track {id}"),
            codec: "subrip".into(),
            external: false,
        }
    }

    #[test]
    fn unavailable_player_does_not_leave_a_downloaded_subtitle_pending() -> anyhow::Result<()> {
        for closed in [false, true] {
            let mut state = crate::ui::app::init();
            state.player.playing = true;
            state.player.subtitles.pending =
                Some((Slot::Primary, 1, "Download".into(), "eng".into()));
            if closed {
                state.services = Some(crate::services::Services::install_fixture()?.0);
            }
            state.apply_subtitle_file(
                0,
                1,
                Ok(std::sync::Arc::new(crate::services::SubtitleFile {
                    path: std::path::PathBuf::new(),
                })),
            );
            assert!(state.player.subtitles.pending.is_none(), "closed: {closed}");
            assert_eq!(
                state.player.subtitles.error.as_deref(),
                Some("Player is closed")
            );
            assert!(state.player.subtitles.files.is_empty());
        }
        Ok(())
    }

    #[test]
    fn subtitle_load_failures_only_clear_the_matching_live_request() {
        let mut state = crate::ui::app::init();
        state.player.playing = true;
        state.player.generation = 2;
        state.player.subtitles.pending = Some((Slot::Primary, 7, "Download".into(), "eng".into()));
        for (generation, request) in [(1, 7), (2, 6)] {
            state.apply_subtitle_load_failed(generation, request, "stale".into());
            assert!(state.player.subtitles.pending.is_some());
            assert!(state.player.subtitles.error.is_none());
            assert!(!state.send_subtitle_load(generation, request, std::path::PathBuf::new()));
            assert!(state.player.subtitles.pending.is_some());
            assert!(state.player.subtitles.error.is_none());
        }
        state.apply_subtitle_load_failed(2, 7, "Cannot load this subtitle".into());
        assert!(state.player.subtitles.pending.is_none());
        assert_eq!(
            state.player.subtitles.error.as_deref(),
            Some("Cannot load this subtitle")
        );
        state.apply_subtitle_load_failed(2, 7, "duplicate".into());
        assert_eq!(
            state.player.subtitles.error.as_deref(),
            Some("Cannot load this subtitle")
        );
    }

    #[test]
    fn external_subtitle_without_services_does_not_leave_a_download_pending() {
        let mut state = crate::ui::app::init();
        state.player.playing = true;
        state
            .player
            .subtitles
            .sources
            .push(crate::services::ExternalSubtitle {
                provider: "Fixture".into(),
                subtitle: scry_addons::Subtitle {
                    id: "fixture".into(),
                    lang: "eng".into(),
                    url: "https://example.invalid/subtitle.srt".into(),
                    label: String::new(),
                },
            });
        state.load_external_subtitle(0);
        assert!(state.player.subtitles.pending.is_none());
        assert_eq!(
            state.player.subtitles.error.as_deref(),
            Some("Player is closed")
        );
    }

    #[test]
    fn toggling_restores_an_explicitly_disabled_secondary_slot() {
        let mut state = crate::ui::app::init();
        state.player.playing = true;
        state.player.subtitles.defaults.secondary = Some("spa".into());
        let snapshot = Snapshot {
            tracks: vec![track(1, "eng"), track(2, "spa")],
            ..Snapshot::default()
        };
        state.apply_subtitle_snapshot(0, snapshot.clone());
        state.player.subtitles.slot = Slot::Secondary;
        state.select_subtitle(None);
        state.toggle_subtitles();
        state.toggle_subtitles();
        state.apply_subtitle_snapshot(0, snapshot);
        assert_eq!(state.player.subtitles.selected, [Some(1), None]);
    }

    #[test]
    fn selecting_the_other_slot_does_not_cancel_a_pending_subtitle() {
        let mut state = crate::ui::app::init();
        state.player.playing = true;
        state.player.subtitles.pending = Some((Slot::Primary, 1, "Download".into(), "eng".into()));
        state.player.subtitles.slot = Slot::Secondary;
        state.select_subtitle(None);
        assert!(state.player.subtitles.pending.is_some());
        state.apply_subtitle_snapshot(
            0,
            Snapshot {
                tracks: vec![Track {
                    title: "Download".into(),
                    ..track(3, "eng")
                }],
                ..Snapshot::default()
            },
        );
        assert_eq!(state.player.subtitles.selected, [Some(3), None]);
        assert!(state.player.subtitles.pending.is_none());
    }

    #[test]
    fn subtitle_delay_saturates_before_clamping() {
        let mut state = crate::ui::app::init();
        state.player.playing = true;
        state.player.subtitles.selected[0] = Some(1);
        state.adjust_subtitle_delay(1);
        state.adjust_subtitle_delay(i32::MAX);
        assert_eq!(state.player.subtitles.delay[0], 36000);
        state.adjust_subtitle_delay(i32::MIN);
        assert_eq!(state.player.subtitles.delay[0], -36000);
    }
}
