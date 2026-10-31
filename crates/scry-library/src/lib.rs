//! Synchronous, local-only app storage. Run on a worker, never the UI thread.
#![forbid(unsafe_code)]

mod catalog;
mod collections;
pub mod guide;
mod playback;
mod stats;
mod storage;

pub use catalog::models::{
    COMPLETION_THRESHOLD, EpisodeRelease, MAX_PAGE_SIZE, Media, MediaKey, MediaKind, MediaState,
    Page, Playlist, Progress, SeriesProgress, WATCHED_THRESHOLD, WatchedState, is_complete,
};
pub use playback::{skip_segments::SkipSegmentsRecord, watch_status::can_mark_watched};
pub use stats::{
    achievements::{
        Achievement, AchievementDb, AchievementEntry, AchievementEvaluation, Evaluation,
    },
    metrics::{NerdStats, StatsPeriod, TopTitle},
};
pub use storage::{
    addons::AddonRecord,
    database::{DATABASE_NAME, Library},
};
