//! Local virtual channels. Published slots are immutable; only tuning needs a source.
mod scheduling;
mod storage;

use crate::{Media, MediaKind, Playlist};
use serde::{Deserialize, Serialize};

pub use storage::GuideMetadata;

pub const MAX_CHANNELS: usize = 32;
const HORIZON: i64 = 24 * 60 * 60;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelSource {
    Collection(i64),
    Library,
    Comfort,
    Genre(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpisodeOrder {
    #[default]
    Shuffle,
    Sequential,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Program {
    pub media: Media,
    pub show_title: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub description: String,
    pub artwork: Option<String>,
    pub duration: u32,
    pub estimated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub starts_at: i64,
    pub program: Program,
}

impl Slot {
    #[must_use]
    pub fn ends_at(&self) -> i64 {
        self.starts_at
            .saturating_add(i64::from(self.program.duration))
    }

    #[must_use]
    pub fn contains(&self, now: i64) -> bool {
        self.starts_at <= now && now < self.ends_at()
    }

    #[must_use]
    pub fn elapsed_seconds(&self, now: i64) -> u32 {
        u32::try_from(
            now.saturating_sub(self.starts_at)
                .clamp(0, i64::from(self.program.duration)),
        )
        .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub kind: MediaKind,
    pub source: ChannelSource,
    pub slots: Vec<Slot>,
}

impl Channel {
    #[must_use]
    pub fn current(&self, now: i64) -> Option<&Slot> {
        self.slots.iter().find(|slot| slot.contains(now))
    }
}

#[derive(Clone, Debug)]
pub struct Collection {
    pub playlist: Playlist,
    pub members: Vec<Media>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SourcePolicy {
    /// Keep the Guide preview idle until a channel is selected for playback.
    pub autoplay_disabled: bool,
    pub allow_torrents: bool,
    /// Empty means any installed provider. Stored by display name, never private URL.
    pub provider: String,
    pub quality: String,
    pub order: EpisodeOrder,
    pub watched_only: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub channels: Vec<Channel>,
    pub collections: Vec<Collection>,
    pub policy: SourcePolicy,
}

#[derive(Clone, Debug)]
pub enum GuideAction {
    Refresh,
    CreateCollection(String),
    RenameCollection(i64, String),
    DeleteCollection(i64),
    Membership {
        collection: i64,
        media: Media,
        included: bool,
    },
    EnableCollection(i64),
    Policy(SourcePolicy),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Stored {
    channels: Vec<Channel>,
    policy: SourcePolicy,
}

pub(crate) use storage::validate;

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Guide fixtures use known nonempty collections and fail on storage errors"
)]
mod tests;
