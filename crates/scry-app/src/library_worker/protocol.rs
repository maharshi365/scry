//! Messages shared by the UI handle and the database thread.
use scry_library::{Media, MediaKey, Playlist, Progress, WatchedState};

#[derive(Debug)]
pub enum Command {
    /// `offset` applies to watchlist, watched items, and the selected playlist.
    /// The playlist selector contains its first 200 playlists.
    Refresh {
        generation: u64,
        selected: Option<MediaKey>,
        playlist: Option<i64>,
        offset: u32,
        /// Visible episode identities, at most 200. Empty uses stored episodes.
        episodes: Vec<MediaKey>,
    },
    /// Entries are upserted in order (parents before episodes).
    Upsert(Vec<Media>),
    ToggleWatchlist(Media),
    SetWatched(Media, bool),
    ClearOverride(Media),
    CreatePlaylist(String),
    RenamePlaylist(i64, String),
    DeletePlaylist(i64),
    AddToPlaylist(i64, Media),
    RemoveFromPlaylist(i64, MediaKey),
    ReorderPlaylist(i64, Vec<MediaKey>),
    SaveProgress(Media, f64, f64),
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
pub struct Snapshot {
    pub watchlist: Vec<Media>,
    pub watched: Vec<Media>,
    pub playlists: Vec<Playlist>,
    pub items: Vec<Media>,
    /// `None` also covers a selected key not yet stored locally.
    pub selected_state: Option<SelectedState>,
    /// First 200 registered episodes of the selected series, or the selected
    /// episode's parent series. No provider calls are made here.
    pub episode_states: Vec<(MediaKey, WatchedState)>,
}

#[derive(Debug)]
pub enum Event {
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
