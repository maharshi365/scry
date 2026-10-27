//! Season filtering, episode paging, and stable protocol identity selection.
use super::{EPISODES_PER_PAGE, State};
use std::ops::Range;

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
        self.episode_season
            .filter(|season| seasons.contains(season))
            .or_else(|| seasons.first().copied())
            .flatten()
    }

    pub(super) fn season_episode_indices(&self) -> Vec<usize> {
        let season = self.active_season();
        let Some(meta) = &self.detail else {
            return Vec::new();
        };
        let query = self.episode_search.text.trim().to_lowercase();
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
        let range = self.episode_range();
        let start = self
            .episode_state_offset
            .unwrap_or(range.start)
            .min(indices.len());
        indices[start..start.saturating_add(EPISODES_PER_PAGE).min(indices.len())].to_vec()
    }

    pub(super) fn select_season(&mut self, season: Option<u32>) {
        self.season_menu_open = false;
        if !self.seasons().contains(&season) || self.active_season() == season {
            return;
        }
        self.episode_season = Some(season);
        self.episode_offset = 0;
        self.episode_state_offset = None;
        self.episode_viewport = None;
        self.episode_search = rune::TextEdit::new("");
        self.selected_video_id = None;
        self.stream_id += 1;
        self.streams.clear();
        self.stream_provider = None;
        self.source_menu_open = false;
        self.source_sort_open = false;
        self.warnings.clear();
        self.error = None;
        self.loading = false;
        self.status = "Choose an episode to find streams.".into();
        self.register_library_detail();
    }

    pub(super) fn episode_range(&self) -> Range<usize> {
        let count = self.season_episode_indices().len();
        let last = count.saturating_sub(1) / EPISODES_PER_PAGE * EPISODES_PER_PAGE;
        let start = (self.episode_offset / EPISODES_PER_PAGE * EPISODES_PER_PAGE).min(last);
        start..start.saturating_add(EPISODES_PER_PAGE).min(count)
    }

    pub(super) fn move_episode_page(&mut self, next: bool) {
        let range = self.episode_range();
        self.episode_offset = if next {
            range.start.saturating_add(EPISODES_PER_PAGE)
        } else {
            range.start.saturating_sub(EPISODES_PER_PAGE)
        };
        self.episode_offset = self.episode_range().start;
        if self.episode_offset != range.start {
            self.register_library_detail();
        }
    }

    pub(super) fn select_episode(&mut self, index: usize) {
        self.navigation.anchor = Some(format!("episode-{index}"));
        if let Some(meta) = &self.detail
            && let Some(video) = meta.videos.get(index)
        {
            self.find_streams(meta.kind.clone(), video.id.clone());
        }
    }

    pub(super) fn show_episodes(&mut self) {
        self.selected_video_id = None;
        self.stream_id += 1;
        self.streams.clear();
        self.stream_provider = None;
        self.loading = false;
        self.source_menu_open = false;
        self.source_sort_open = false;
        self.warnings.clear();
        self.error = None;
        self.refresh_library();
    }
}
