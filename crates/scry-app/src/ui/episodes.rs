//! Season filtering, episode paging, and stable protocol identity selection.
use super::{EPISODES_PER_PAGE, State};
use std::ops::Range;

#[derive(Clone, Copy, Default)]
pub(super) enum SeasonSelection {
    #[default]
    Automatic,
    Selected(Option<u32>),
}

pub(super) struct EpisodesState {
    // Stable protocol ID, including season/episode suffix, for library integration.
    pub selected: Option<String>,
    pub offset: usize,
    pub viewport: Option<[f32; 4]>,
    pub state_offset: Option<usize>,
    pub search: rune::TextEdit,
    pub season: SeasonSelection,
    pub menu_open: bool,
}

impl Default for EpisodesState {
    fn default() -> Self {
        Self {
            selected: None,
            offset: 0,
            viewport: None,
            state_offset: None,
            search: rune::TextEdit::new(""),
            season: SeasonSelection::Automatic,
            menu_open: false,
        }
    }
}

#[derive(Clone)]
pub(super) struct EpisodeSnapshot {
    selected: Option<String>,
    offset: usize,
    season: SeasonSelection,
}

impl EpisodesState {
    pub fn range(&self, count: usize) -> Range<usize> {
        let last = count.saturating_sub(1) / EPISODES_PER_PAGE * EPISODES_PER_PAGE;
        let start = (self.offset / EPISODES_PER_PAGE * EPISODES_PER_PAGE).min(last);
        start..start.saturating_add(EPISODES_PER_PAGE).min(count)
    }

    pub fn select_season(&mut self, season: Option<u32>) {
        *self = Self {
            season: SeasonSelection::Selected(season),
            ..Self::default()
        };
    }

    pub fn snapshot(&self) -> EpisodeSnapshot {
        EpisodeSnapshot {
            selected: self.selected.clone(),
            offset: self.offset,
            season: self.season,
        }
    }

    pub fn restore(&mut self, snapshot: EpisodeSnapshot) {
        *self = Self {
            selected: snapshot.selected,
            offset: snapshot.offset,
            season: snapshot.season,
            ..Self::default()
        };
    }
}

pub(super) fn season_label(season: Option<u32>) -> String {
    match season {
        Some(0) => "Specials".into(),
        Some(number) => format!("Season {number}"),
        None => "Other episodes".into(),
    }
}

impl State {
    pub(super) fn seasons(&self) -> Vec<Option<u32>> {
        let mut seasons: Vec<_> = self
            .detail
            .meta
            .iter()
            .flat_map(|meta| &meta.videos)
            .map(|video| video.season)
            .collect();
        seasons.sort_by_key(|season| match season {
            Some(0) => (1, 0),
            Some(number) => (0, *number),
            None => (2, 0),
        });
        seasons.dedup();
        seasons
    }

    pub(super) fn active_season(&self) -> Option<u32> {
        let seasons = self.seasons();
        let selected = match self.detail.episodes.season {
            SeasonSelection::Automatic => None,
            SeasonSelection::Selected(season) => Some(season),
        };
        selected
            .filter(|season| seasons.contains(season))
            .or_else(|| seasons.first().copied())
            .flatten()
    }

    pub(super) fn season_episode_indices(&self) -> Vec<usize> {
        let season = self.active_season();
        let Some(meta) = &self.detail.meta else {
            return Vec::new();
        };
        let query = self.detail.episodes.search.text.trim().to_lowercase();
        let mut indices: Vec<_> = meta
            .videos
            .iter()
            .enumerate()
            .filter(|(_, video)| {
                video.season == season
                    && (query.is_empty()
                        || format!("{} {}", video.episode.unwrap_or(0), video.title)
                            .to_lowercase()
                            .contains(&query))
            })
            .map(|(index, _)| index)
            .collect();
        indices.sort_by_key(|index| meta.videos[*index].episode);
        indices
    }

    pub(super) fn visible_episode_indices(&self) -> Vec<usize> {
        let indices = self.season_episode_indices();
        let range = self.detail.episodes.range(indices.len());
        let start = self
            .detail
            .episodes
            .state_offset
            .unwrap_or(range.start)
            .min(indices.len());
        indices[start..start.saturating_add(EPISODES_PER_PAGE).min(indices.len())].to_vec()
    }

    pub(super) fn select_season(&mut self, season: Option<u32>) {
        self.detail.episodes.menu_open = false;
        if !self.seasons().contains(&season) || self.active_season() == season {
            return;
        }
        self.library.quick_play = None;
        self.detail.episodes.select_season(season);
        self.runtime.streams.invalidate();
        self.detail.sources.reset();
        self.detail.sources.request.status = "Choose an episode to find streams.".into();
        self.register_library_detail();
    }

    pub(super) fn episode_range(&self) -> Range<usize> {
        self.detail
            .episodes
            .range(self.season_episode_indices().len())
    }

    pub(super) fn move_episode_page(&mut self, next: bool) {
        let count = self.season_episode_indices().len();
        let range = self.detail.episodes.range(count);
        self.detail.episodes.offset = if next {
            range.start.saturating_add(EPISODES_PER_PAGE)
        } else {
            range.start.saturating_sub(EPISODES_PER_PAGE)
        };
        self.detail.episodes.offset = self.detail.episodes.range(count).start;
        if self.detail.episodes.offset != range.start {
            self.register_library_detail();
        }
    }

    pub(super) fn select_episode(&mut self, index: usize) {
        self.navigation.anchor = Some(format!("episode-{index}"));
        if let Some(meta) = &self.detail.meta
            && let Some(video) = meta.videos.get(index)
        {
            self.library.quick_play = None;
            self.find_streams(meta.kind.clone(), video.id.clone());
        }
    }

    pub(super) fn show_episodes(&mut self) {
        self.library.quick_play = None;
        self.detail.episodes.selected = None;
        self.runtime.streams.invalidate();
        self.detail.sources.reset();
        self.refresh_library();
    }
}
