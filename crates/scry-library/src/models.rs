use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_PAGE_SIZE: u32 = 200;
pub const COMPLETION_THRESHOLD: f64 = 0.9;
pub const WATCHED_THRESHOLD: f64 = 0.7;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Movie,
    Series,
    Episode,
}

impl MediaKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Series => "series",
            Self::Episode => "episode",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "movie" => Ok(Self::Movie),
            "series" => Ok(Self::Series),
            "episode" => Ok(Self::Episode),
            _ => anyhow::bail!("invalid media kind"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MediaKey {
    pub namespace: String,
    pub kind: MediaKind,
    pub id: String,
}

impl MediaKey {
    pub fn new(namespace: impl Into<String>, kind: MediaKind, id: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            kind,
            id: id.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Media {
    pub key: MediaKey,
    pub title: String,
    pub parent: Option<MediaKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub position_seconds: f64,
    pub duration_seconds: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchedState {
    pub watched: bool,
    pub manual_override: Option<bool>,
}

#[derive(Clone, Copy, Debug)]
pub struct Page {
    pub(crate) limit: u32,
    pub(crate) offset: u32,
}

impl Page {
    /// # Errors
    /// Returns an error if the page size is zero or exceeds the maximum.
    pub fn new(limit: u32, offset: u32) -> Result<Self> {
        ensure!(
            (1..=MAX_PAGE_SIZE).contains(&limit),
            "page limit must be 1..={MAX_PAGE_SIZE}"
        );
        Ok(Self { limit, offset })
    }
}

impl Default for Page {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
        }
    }
}

#[must_use]
pub fn is_complete(position_seconds: f64, duration_seconds: f64) -> bool {
    position_seconds.is_finite()
        && duration_seconds.is_finite()
        && duration_seconds > 0.0
        && position_seconds / duration_seconds >= COMPLETION_THRESHOLD
}
