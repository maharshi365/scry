use super::Ordering;
use anyhow::Result;
use scry_addons::Meta;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Item {
    pub id: String,
    pub title: String,
    pub year: Option<u32>,
    pub kind: String,
    pub files: Vec<VideoFile>,
    pub matched: Option<Match>,
    pub keep_unmatched: bool,
    pub ordering: Option<Ordering>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct VideoFile {
    pub path: PathBuf,
    pub episode: Option<Episode>,
    pub corrected: bool,
    pub available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Episode {
    pub season: u32,
    pub numbers: Vec<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Match {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub year: String,
    pub poster: Option<String>,
    pub manual: bool,
}

impl Match {
    pub fn from_meta(meta: &Meta, manual: bool) -> Self {
        Self {
            id: meta.id.clone(),
            kind: meta.kind.clone(),
            name: meta.name.clone(),
            year: meta.year.clone(),
            poster: meta.poster.clone(),
            manual,
        }
    }
}

impl Item {
    pub fn meta(&self) -> Result<Meta> {
        let (id, kind, name, year, poster) = self.matched.as_ref().map_or_else(
            || {
                (
                    self.id.as_str(),
                    self.kind.as_str(),
                    self.title.as_str(),
                    self.year.map(|v| v.to_string()).unwrap_or_default(),
                    None,
                )
            },
            |m| {
                (
                    m.id.as_str(),
                    m.kind.as_str(),
                    m.name.as_str(),
                    m.year.clone(),
                    m.poster.clone(),
                )
            },
        );
        let mut episodes = BTreeMap::new();
        for file in &self.files {
            if self.needs_episode_order() {
                continue;
            }
            if let Some(ordering) = &self.ordering {
                let mapped = ordering.resolve(file);
                for video in mapped {
                    episodes.insert((video.file_season, video.file_episode, video.id.clone()), serde_json::json!({
                        "id":video.id, "title":video.title, "season":video.file_season, "episode":video.file_episode,
                        "released":video.released
                    }));
                }
                continue;
            }
            if let Some(episode) = &file.episode {
                for number in &episode.numbers {
                    episodes.insert((episode.season, *number, format!("{id}:{}:{number}", episode.season)), serde_json::json!({
                        "id":format!("{id}:{}:{number}", episode.season),
                        "title":format!("Episode {number}"), "season":episode.season, "episode":number
                    }));
                }
            }
        }
        let mut videos: Vec<_> = episodes.into_values().collect();
        if kind == "series" {
            videos.extend(self.files.iter().filter(|f| self.unresolved(f)).map(|file| serde_json::json!({
                "id":file.local_id(), "title":file.path.file_name().unwrap_or_default().to_string_lossy()
            })));
        }
        Ok(serde_json::from_value(serde_json::json!({
            "id":id, "type":kind, "name":name, "year":year, "poster":poster,
            "videos":videos
        }))?)
    }

    pub fn sources(&self, kind: &str, id: &str) -> Vec<&VideoFile> {
        let (media_id, media_kind) = self
            .matched
            .as_ref()
            .map_or((self.id.as_str(), self.kind.as_str()), |m| (&m.id, &m.kind));
        if media_kind != kind {
            return Vec::new();
        }
        self.files
            .iter()
            .filter(|file| {
                if !file.available {
                    return false;
                }
                if kind == "movie" {
                    return media_id == id;
                }
                if self.needs_episode_order() {
                    return id == file.local_id();
                }
                if let Some(ordering) = &self.ordering {
                    let mapped = ordering.resolve(file);
                    return if mapped.is_empty() {
                        id == file.local_id()
                    } else {
                        mapped.iter().any(|video| video.id == id)
                    };
                }
                file.episode.as_ref().map_or_else(
                    || id == file.local_id(),
                    |ep| {
                        ep.numbers
                            .iter()
                            .any(|n| id == format!("{media_id}:{}:{n}", ep.season))
                    },
                )
            })
            .collect()
    }

    fn unresolved(&self, file: &VideoFile) -> bool {
        if let Some(ordering) = &self.ordering {
            return ordering.resolve(file).is_empty();
        }
        self.needs_episode_order() || file.episode.is_none()
    }

    fn needs_episode_order(&self) -> bool {
        self.ordering.is_none()
            && self
                .matched
                .as_ref()
                .is_some_and(|matched| matched.kind == "series" && matched.id.starts_with("kitsu:"))
    }
}

impl VideoFile {
    fn local_id(&self) -> String {
        format!(
            "local-file:{}",
            blake3::hash(self.path.as_os_str().as_encoded_bytes()).to_hex()
        )
    }
}
