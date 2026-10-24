//! Messages shared by the UI handle and the database thread.
use scry_library::{Media, MediaKey, MediaKind, Progress, WatchedState};

#[derive(Clone, Debug)]
pub enum Command {
    TrackedSeries {
        generation: u64,
        offset: u32,
    },
    ReconcileSeries(SeriesRequest),
    SetEpisodesWatched {
        parent: Media,
        episodes: Vec<Media>,
        watched: bool,
    },
    SeriesStates {
        generation: u64,
        requests: Vec<SeriesRequest>,
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
        offset: u32,
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
    ToggleWatchlist(Media),
    SetWatchlist(Media, bool),
    SetWatched(Media, bool),
    ClearOverride(Media),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeriesRequest {
    pub key: MediaKey,
    pub released: Vec<MediaKey>,
    pub metadata_known: bool,
}

#[derive(Debug)]
pub struct SeriesState {
    pub key: MediaKey,
    pub tracked: bool,
    pub watched: bool,
    pub progress: Option<Progress>,
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
    TrackedSeries {
        generation: u64,
        items: Vec<Media>,
    },
    SeriesReconciled {
        changed: bool,
    },
    SeriesStates {
        generation: u64,
        states: Vec<SeriesState>,
        changed: bool,
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
}
