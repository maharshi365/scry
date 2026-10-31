//! Local video discovery and durable matches, independent of network and UI work.
mod filename;
mod index;
mod item;
mod ordering;
mod storage;

pub(crate) use filename::normalized;
pub(crate) use index::{Index, is_video};
pub(crate) use item::{Episode, Item, Match, VideoFile};
pub(crate) use ordering::{EpisodeOrder, OrderedEpisode, Ordering};
pub(crate) use storage::{load, save};

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Local media fixtures must be created and read"
)]
mod tests;
