//! Messages shared by the UI handle and the database thread.
use scry_library::{Media, MediaKey, MediaKind, Progress, WatchedState};

#[derive(Clone, Debug)]
pub enum Command {
    UpNext {
        generation: u64,
        offset: u32,
    },
    TrackedSeries {
        generation: u64,
        offset: u32,
    },
    SyncSeries {
        key: MediaKey,
        releases: Vec<scry_library::EpisodeRelease>,
    },
    SetEpisodesWatched {
        parent: Media,
        episodes: Vec<Media>,
        inventory: Vec<scry_library::EpisodeRelease>,
        watched: bool,
    },
    SeriesStates {
        generation: u64,
        keys: Vec<MediaKey>,
    },
    MovieStates {
        generation: u64,
        keys: Vec<MediaKey>,
    },
    Browse {
        generation: u64,
        watched: bool,
        kind: MediaKind,
        offset: u32,
    },
    Overview {
        generation: u64,
        watching_offset: u32,
        calendar_offset: u32,
    },
    SavePlayback(Media, f64, f64, f64),
    FinishPlayback(MediaKey, Option<Media>),
    Refresh {
        generation: u64,
        selected: Option<MediaKey>,
        /// Visible episode identities, at most 200. Empty uses stored episodes.
        episodes: Vec<MediaKey>,
    },
    /// Entries are upserted in order (parents before episodes).
    Upsert(Vec<Media>),
    RegisterRelease(Media, Option<i64>),
    ToggleWatchlist(Media),
    SetWatchlist(Media, bool),
    SetWatched(Media, bool),
    SetWatchedMetadata {
        item: Media,
        released_at: Option<i64>,
        inventory: Option<Vec<scry_library::EpisodeRelease>>,
        watched: bool,
    },
    /// A FIFO barrier for shutdown: all preceding writes finish before this ack.
    Flush(u64),
}

#[derive(Debug)]
pub struct SelectedState {
    pub key: MediaKey,
    pub watched: WatchedState,
    pub watchlist: bool,
    pub progress: Option<Progress>,
}

#[derive(Debug)]
pub struct MovieState {
    pub key: MediaKey,
    pub watched: bool,
    pub progress: Option<Progress>,
}

#[derive(Debug)]
pub struct SeriesState {
    pub key: MediaKey,
    pub tracked: bool,
    pub watched: bool,
    pub progress: Option<Progress>,
    pub summary: Option<scry_library::SeriesProgress>,
}

#[derive(Debug)]
pub struct Snapshot {
    /// `None` also covers a selected key not yet stored locally.
    pub selected_state: Option<SelectedState>,
    /// First 200 registered episodes of the selected series, or the selected
    /// episode's parent series. No provider calls are made here.
    pub episode_states: Vec<(MediaKey, WatchedState)>,
}

#[derive(Debug)]
pub enum Event {
    UpNext {
        generation: u64,
        items: Vec<(Media, Option<Progress>)>,
        has_more: bool,
    },
    TrackedSeries {
        generation: u64,
        items: Vec<Media>,
    },
    SeriesSynced {
        changed: bool,
    },
    SeriesStates {
        generation: u64,
        states: Vec<SeriesState>,
    },
    MovieStates {
        generation: u64,
        states: Vec<MovieState>,
    },
    Browse {
        generation: u64,
        items: Vec<Media>,
    },
    Overview {
        generation: u64,
        watching: Vec<(Media, Progress)>,
        watching_watched: Vec<MediaKey>,
        next_release_at: Option<i64>,
        series: Vec<Media>,
    },
    Ready,
    Snapshot {
        generation: u64,
        data: Snapshot,
    },
    Changed,
    Flushed(u64),
    /// Fixed messages only: database details and media values stay private.
    Error(String),
    /// A failed read/sync request is not an acknowledgement of a queued UI mutation.
    RequestError(String),
}

impl Command {
    pub(super) fn acknowledges_mutation(&self) -> bool {
        matches!(
            self,
            Self::SetEpisodesWatched { .. }
                | Self::SavePlayback(..)
                | Self::FinishPlayback(..)
                | Self::Upsert(..)
                | Self::ToggleWatchlist(..)
                | Self::SetWatchlist(..)
                | Self::SetWatched(..)
                | Self::SetWatchedMetadata { .. }
                | Self::RegisterRelease(..)
        )
    }
}
