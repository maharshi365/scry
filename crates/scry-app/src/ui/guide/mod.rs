//! Guide-owned state, tuning, and clock-driven program transitions.
mod settings;
mod sources;
mod tuning;
mod view;

use super::{Page, State};
use crate::{library_worker::Command, services::Source};
use rune::{TextEdit, Tx};
use scry_addons::{Meta, Stream};
use scry_library::{
    Media, MediaKey, MediaKind,
    guide::{GuideAction, GuideMetadata, Slot, Snapshot, SourcePolicy},
};
#[cfg(test)]
use sources::select_sources;
use std::collections::HashMap;
pub(super) use view::view;

pub(super) struct GuideUi {
    pub data: Snapshot,
    pub settings: bool,
    pub adding: Option<Media>,
    pub collection: Option<i64>,
    pub name: TextEdit,
    pub selected: Option<String>,
    pub kind: MediaKind,
    pub error: Option<String>,
    pub busy: bool,
    pub tuned: Option<(String, Slot)>,
    pub resolving: bool,
    pub alternatives: Vec<Stream>,
    tuning: tuning::Tuning,
    generation: u64,
    tick: i64,
    refreshed: i64,
    details_pending: Option<u64>,
    details_generation: u64,
    details_after: i64,
    details_checked: HashMap<MediaKey, std::time::Instant>,
    metadata: HashMap<MediaKey, (Media, GuideMetadata, Vec<scry_library::EpisodeRelease>)>,
}

impl Default for GuideUi {
    fn default() -> Self {
        Self {
            data: Snapshot::default(),
            settings: false,
            adding: None,
            collection: None,
            name: TextEdit::new(""),
            selected: None,
            kind: MediaKind::Series,
            error: None,
            busy: false,
            tuned: None,
            resolving: false,
            alternatives: Vec::new(),
            tuning: tuning::Tuning::default(),
            generation: 0,
            tick: 0,
            refreshed: 0,
            details_pending: None,
            details_generation: 0,
            details_after: 0,
            details_checked: HashMap::new(),
            metadata: HashMap::new(),
        }
    }
}

impl State {
    pub(in crate::ui) fn register_guide_metadata(&mut self, key: &MediaKey, meta: &Meta) {
        if !matches!(key.kind, MediaKind::Series | MediaKind::Movie)
            || self.guide.metadata.len() >= 200
        {
            return;
        }
        let media = Media {
            key: key.clone(),
            title: meta.name.clone(),
            parent: None,
        };
        let metadata = GuideMetadata {
            released_at: crate::media_metadata::meta_release_timestamp(meta),
            genres: meta.genres.iter().take(64).cloned().collect(),
            description: meta.description.chars().take(8192).collect(),
            artwork: meta.background.clone().or_else(|| meta.poster.clone()),
        };
        let inventory = if key.kind == MediaKind::Series {
            crate::media_metadata::episode_inventory(key, meta)
        } else {
            Vec::new()
        };
        self.guide
            .metadata
            .insert(key.clone(), (media, metadata, inventory));
    }

    pub(in crate::ui) fn guide_action(&mut self, action: GuideAction) {
        let Some(worker) = &self.library.worker else {
            self.guide.error = Some("Local library is unavailable".into());
            return;
        };
        let generation = self.guide.generation.wrapping_add(1);
        match worker.send(Command::Guide { generation, action }) {
            Ok(()) => {
                self.guide.generation = generation;
                self.guide.busy = true;
                self.guide.error = None;
            }
            Err(_) => self.guide.error = Some("Guide is busy; try again".into()),
        }
    }

    pub(in crate::ui) fn refresh_guide(&mut self) {
        self.guide.refreshed = chrono::Utc::now().timestamp();
        self.guide_action(GuideAction::Refresh);
    }

    pub(in crate::ui) fn apply_guide(&mut self, generation: u64, data: Snapshot) {
        if generation != self.guide.generation {
            return;
        }
        self.guide.data = data;
        self.guide.busy = false;
        if self.guide.selected.as_ref().is_none_or(|id| {
            !self.guide.data.channels.iter().any(|c| {
                &c.id == id
                    && c.kind == self.guide.kind
                    && c.current(chrono::Utc::now().timestamp()).is_some()
            })
        }) {
            self.guide.selected = self
                .guide
                .data
                .channels
                .iter()
                .find(|c| {
                    c.kind == self.guide.kind && c.current(chrono::Utc::now().timestamp()).is_some()
                })
                .map(|c| c.id.clone());
        }
        if let Some((id, _)) = &self.guide.tuned
            && !self.guide.data.channels.iter().any(|c| &c.id == id)
        {
            self.leave_guide();
            self.stop();
        }
        self.request_guide_details();
        self.autoplay_guide();
    }

    pub(in crate::ui) fn autoplay_guide(&mut self) {
        if self.navigation.page != Page::Guide
            || self.config_busy
            || self.guide.settings
            || self.guide.busy
            || self.guide.tuned.is_some()
            || self.guide.resolving
            || self.guide.error.is_some()
        {
            return;
        }
        let now = chrono::Utc::now().timestamp();
        let id = self
            .guide
            .data
            .channels
            .iter()
            .find(|channel| {
                Some(&channel.id) == self.guide.selected.as_ref() && channel.current(now).is_some()
            })
            .or_else(|| {
                self.guide.data.channels.iter().find(|channel| {
                    channel.kind == self.guide.kind && channel.current(now).is_some()
                })
            })
            .map(|channel| channel.id.clone());
        if let Some(id) = id {
            self.tune_guide(id);
        }
    }

    pub(in crate::ui) fn select_guide_kind(&mut self, kind: MediaKind) {
        self.guide.kind = kind;
        let now = chrono::Utc::now().timestamp();
        self.guide.selected = self
            .guide
            .data
            .channels
            .iter()
            .find(|channel| channel.kind == kind && channel.current(now).is_some())
            .map(|channel| channel.id.clone());
        if let Some(id) = self.guide.selected.clone() {
            self.tune_guide(id);
        }
    }

    pub(in crate::ui) fn update_guide_policy(&mut self, policy: SourcePolicy) {
        if self.guide.busy {
            return;
        }
        let previous = &self.guide.data.policy;
        if previous.provider != policy.provider
            || previous.quality != policy.quality
            || previous.allow_torrents != policy.allow_torrents
        {
            self.leave_guide();
            self.stop();
        }
        self.guide_action(GuideAction::Policy(policy));
    }

    fn request_guide_details(&mut self) {
        if self.navigation.page != Page::Guide
            || self.guide.details_pending.is_some()
            || self.addons.is_empty()
            || chrono::Utc::now().timestamp() < self.guide.details_after
        {
            return;
        }
        self.guide
            .details_checked
            .retain(|_, checked| checked.elapsed() < std::time::Duration::from_mins(10));
        let mut titles = Vec::new();
        for media in self
            .guide
            .data
            .collections
            .iter()
            .flat_map(|group| &group.members)
            .chain(&self.calendar.series)
            .take(2000)
        {
            if !self.guide.details_checked.contains_key(&media.key)
                && !titles.iter().any(|title: &Media| title.key == media.key)
            {
                titles.push(media.clone());
                if titles.len() == 12 {
                    break;
                }
            }
        }
        if titles.is_empty() {
            return;
        }
        if let Some(services) = &mut self.services {
            for title in &titles {
                self.guide
                    .details_checked
                    .insert(title.key.clone(), std::time::Instant::now());
            }
            self.guide.details_generation = self.guide.details_generation.wrapping_add(1);
            let generation = self.guide.details_generation;
            self.guide.details_pending = Some(generation);
            self.guide.details_after = chrono::Utc::now().timestamp().saturating_add(60);
            services.guide_details(generation, self.addons.clone(), titles);
        }
    }

    pub(in crate::ui) fn apply_guide_details(
        &mut self,
        generation: u64,
        found: Vec<(Media, Meta)>,
    ) {
        if self.guide.details_pending != Some(generation) {
            return;
        }
        self.guide.details_pending = None;
        for (media, meta) in found {
            self.register_known_runtimes(&media.key, &meta);
        }
        // Flush projections before requesting the next local schedule snapshot.
        self.guide.refreshed = 0;
    }

    pub(in crate::ui) fn guide_failed(&mut self, generation: u64) {
        if generation == self.guide.generation {
            self.guide.busy = false;
            self.guide.error = Some("Cannot update Guide. Check the collection name, membership, and channel limits, then retry.".into());
        }
    }

    pub(in crate::ui) fn leave_guide(&mut self) {
        if self.navigation.page != Page::Guide
            && self.guide.tuned.is_none()
            && !self.guide.resolving
        {
            return;
        }
        self.guide.tuned = None;
        self.guide.resolving = false;
        self.guide.alternatives.clear();
        self.guide.tuning = tuning::Tuning::default();
        self.stream_id = self.stream_id.wrapping_add(1);
        if let Some(services) = &mut self.services {
            services.cancel_streams();
        }
    }

    pub(in crate::ui) fn tune_guide(&mut self, id: String) {
        if self.navigation.page != Page::Guide {
            return;
        }
        let now = chrono::Utc::now().timestamp();
        let slot = self
            .guide
            .data
            .channels
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.current(now))
            .cloned();
        let Some(slot) = slot else {
            self.leave_guide();
            self.stop();
            self.guide.error = Some(
                "This channel has no current program. Refresh Guide or add more titles.".into(),
            );
            return;
        };
        let fullscreen = self.player.fullscreen;
        self.stop();
        // Ignore the stop acknowledgement while this tune resolves its source.
        self.player.generation = self.player.generation.wrapping_add(1);
        self.set_fullscreen(fullscreen);
        self.leave_guide();
        self.guide.selected = Some(id.clone());
        self.guide.tuned = Some((id, slot.clone()));
        self.guide.resolving = true;
        self.guide.error = None;
        if let Some(services) = &mut self.services {
            services.streams(
                self.stream_id,
                self.addons.clone(),
                if slot.program.media.key.kind == MediaKind::Movie {
                    "movie"
                } else {
                    "series"
                }
                .into(),
                slot.program.media.key.id,
            );
        } else {
            self.guide.resolving = false;
            self.guide.error = Some("Playback services are unavailable".into());
        }
    }

    pub(in crate::ui) fn play_guide_alternative(&mut self) {
        let Some((_, slot)) = self.guide.tuned.clone() else {
            return;
        };
        if !slot.contains(chrono::Utc::now().timestamp()) {
            return;
        }
        if self.guide.alternatives.is_empty() {
            self.guide.error = Some(if self.guide.data.policy.allow_torrents {
                "No playable source from the selected providers. Retry or choose another provider."
            } else {
                "No direct stream found. Your provider may only offer torrents; enable them below to play."
            }.into());
            return;
        }
        let stream = self.guide.alternatives.remove(0);
        let fullscreen = self.player.fullscreen;
        self.stop();
        self.library.playing = Some(slot.program.media.clone());
        self.library.next_episode = None;
        self.library.resume_position = None;
        self.library.observed = None;
        self.library.watched_pending = 0.0;
        self.library.last_sample = None;
        self.library.saved_sample = None;
        self.library.last_save = std::time::Instant::now();
        self.player.meta_id = None;
        self.player.video_id = Some(slot.program.media.key.id.clone());
        self.play_source(
            Source::Channel(stream, slot.starts_at),
            slot.program.media.title,
        );
        self.set_fullscreen(fullscreen);
        self.player.background = slot.program.artwork;
        self.guide_join_started();
    }

    pub(in crate::ui) fn seek_guide_now(&mut self) {
        let now = chrono::Utc::now().timestamp();
        if let Some((_, slot)) = &self.guide.tuned {
            if !slot.contains(now) {
                self.stop();
                return;
            }
            let offset = slot.elapsed_seconds(now);
            self.command(&["seek", &offset.to_string(), "absolute"]);
            self.library.observed = None;
        }
    }

    pub(in crate::ui) fn poll_guide(&mut self, tx: &mut Tx) -> bool {
        if let Some((key, (media, metadata, inventory))) = self.guide.metadata.iter().next()
            && let Some(worker) = &self.library.worker
            && worker
                .send(Command::GuideMetadata(
                    media.clone(),
                    metadata.clone(),
                    inventory.clone(),
                ))
                .is_ok()
        {
            let key = key.clone();
            self.guide.metadata.remove(&key);
        }
        if self.navigation.page != Page::Guide {
            return false;
        }
        let tuning_changed = self.poll_guide_tuning(std::time::Instant::now(), tx);
        tx.wake_after(std::time::Duration::from_secs(1));
        let now = chrono::Utc::now().timestamp();
        if now == self.guide.tick {
            return tuning_changed;
        }
        self.guide.tick = now;
        if !self.guide.busy
            && self.guide.metadata.is_empty()
            && now.saturating_sub(self.guide.refreshed) >= 60
        {
            self.refresh_guide();
        }
        if let Some((id, slot)) = &self.guide.tuned
            && !slot.contains(now)
        {
            let id = id.clone();
            self.tune_guide(id);
        }
        self.autoplay_guide();
        true
    }

    pub(in crate::ui) fn guide_artwork(&self) -> Vec<String> {
        let now = chrono::Utc::now().timestamp();
        self.guide
            .data
            .channels
            .iter()
            .find(|c| Some(&c.id) == self.guide.selected.as_ref())
            .and_then(|c| c.current(now))
            .and_then(|slot| slot.program.artwork.clone())
            .into_iter()
            .collect()
    }

    fn guide_step(&mut self, direction: i32, tune: bool) {
        let channels: Vec<_> = self
            .guide
            .data
            .channels
            .iter()
            .filter(|c| c.kind == self.guide.kind)
            .map(|c| c.id.clone())
            .collect();
        if channels.is_empty() {
            return;
        }
        let current = self
            .guide
            .selected
            .as_ref()
            .and_then(|id| channels.iter().position(|c| c == id))
            .unwrap_or(0);
        let next = if direction < 0 {
            if current == 0 {
                channels.len() - 1
            } else {
                current - 1
            }
        } else {
            (current + 1) % channels.len()
        };
        if let Some(id) = channels.get(next) {
            self.guide.selected = Some(id.clone());
            if tune {
                self.tune_guide(id.clone());
            }
        }
    }
}

pub(super) fn shortcut(
    s: &mut State,
    event: &rune::KeyEvent,
    focus: super::shortcuts::Focus,
) -> bool {
    use super::player::actions::PlayerAction;
    use super::shortcuts::Focus;
    if s.navigation.page != Page::Guide
        || focus == Focus::TextInput
        || event.mods.alt
        || event.mods.ctrl
        || event.mods.cmd
        || event.mods.logo
    {
        return false;
    }
    if event.key == rune::Key::Escape {
        if s.player.fullscreen {
            s.set_fullscreen(false);
            return true;
        }
        if s.guide.settings {
            s.guide.settings = false;
            return true;
        }
    }
    if s.guide.settings {
        return false;
    }
    match &event.key {
        rune::Key::Up => s.guide_step(-1, false),
        rune::Key::Down => s.guide_step(1, false),
        rune::Key::Enter => {
            if let Some(id) = s.guide.selected.clone() {
                s.tune_guide(id);
            }
        }
        rune::Key::Text(key) if key.eq_ignore_ascii_case("f") => {
            s.player_action(PlayerAction::ToggleFullscreen);
        }
        rune::Key::Text(key) if key.eq_ignore_ascii_case("m") => {
            s.player_action(PlayerAction::ToggleMute);
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Guide fixtures must deserialize and UI nodes must exist"
)]
mod tests;
