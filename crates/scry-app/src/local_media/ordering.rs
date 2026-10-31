use super::{VideoFile, filename};
use anyhow::{Result, ensure};
use scry_addons::Meta;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum EpisodeOrder {
    Aired,
    Absolute,
    Dvd,
}

impl EpisodeOrder {
    pub fn label(self) -> &'static str {
        match self {
            Self::Aired => "Aired",
            Self::Absolute => "Anime / absolute",
            Self::Dvd => "DVD",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Ordering {
    pub series_id: String,
    pub mode: EpisodeOrder,
    pub name: String,
    pub source: Option<String>,
    pub episodes: Vec<OrderedEpisode>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct OrderedEpisode {
    pub file_season: u32,
    pub file_episode: u32,
    pub id: String,
    pub title: String,
    pub season: u32,
    pub episode: u32,
    pub released: Option<String>,
}

impl Ordering {
    pub fn from_meta(meta: &Meta, mode: EpisodeOrder) -> Result<Self> {
        ensure!(
            meta.kind == "series" && mode != EpisodeOrder::Dvd,
            "A series episode inventory is required"
        );
        let mut videos: Vec<_> = meta
            .videos
            .iter()
            .filter(|v| {
                v.season
                    .is_some_and(|s| mode == EpisodeOrder::Aired || s > 0)
                    && v.episode.is_some_and(|n| n > 0)
            })
            .collect();
        videos.sort_by_key(|v| (v.season, v.episode));
        let mut previous = (1, 0);
        let mut episodes = Vec::new();
        for (index, video) in videos.into_iter().enumerate() {
            let (Some(season), Some(episode)) = (video.season, video.episode) else {
                continue;
            };
            if mode == EpisodeOrder::Absolute {
                ensure!(
                    (season == previous.0 && episode == previous.1 + 1)
                        || (previous.1 > 0 && season == previous.0 + 1 && episode == 1),
                    "Absolute order needs a complete, unambiguous episode inventory"
                );
                previous = (season, episode);
            }
            episodes.push(OrderedEpisode {
                file_season: if mode == EpisodeOrder::Absolute {
                    1
                } else {
                    season
                },
                file_episode: if mode == EpisodeOrder::Absolute {
                    u32::try_from(index + 1)?
                } else {
                    episode
                },
                id: video.id.clone(),
                title: video.title.clone(),
                season,
                episode,
                released: video.released.clone(),
            });
        }
        let order = Self {
            series_id: meta.id.clone(),
            mode,
            name: mode.label().into(),
            source: None,
            episodes,
        };
        order.validate()?;
        Ok(order)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.series_id.is_empty()
                && !self.episodes.is_empty()
                && self.episodes.len() <= 20_000,
            "Episode order is empty or too large"
        );
        let mut positions = BTreeSet::new();
        let mut links = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for episode in &self.episodes {
            ensure!(
                !episode.id.is_empty(),
                "Episode order has an empty provider ID"
            );
            ensure!(
                links.insert((
                    episode.file_season,
                    episode.file_episode,
                    episode.id.as_str()
                )),
                "Episode order repeats a mapping"
            );
            ensure!(
                positions.insert((episode.file_season, episode.file_episode))
                    || self.mode == EpisodeOrder::Dvd,
                "Episode numbering is ambiguous"
            );
            targets.insert(episode.id.as_str());
        }
        ensure!(
            targets.len() == self.episodes.len(),
            "Episode order repeats a provider episode"
        );
        Ok(())
    }

    pub fn resolve(&self, file: &VideoFile) -> Vec<&OrderedEpisode> {
        let position = if self.mode == EpisodeOrder::Absolute && !file.corrected {
            if let Some(episode) = &file.episode {
                if episode.season != 1 {
                    return Vec::new();
                }
                Some((1, episode.numbers.clone()))
            } else {
                filename::absolute(&file.path).map(|(_, numbers)| (1, numbers))
            }
        } else {
            file.episode
                .as_ref()
                .map(|episode| (episode.season, episode.numbers.clone()))
        };
        let Some((season, numbers)) = position else {
            return Vec::new();
        };
        let mut mapped = Vec::new();
        for number in numbers {
            let matches: Vec<_> = self
                .episodes
                .iter()
                .filter(|e| {
                    if file.corrected {
                        e.season == season && e.episode == number
                    } else {
                        e.file_season == season && e.file_episode == number
                    }
                })
                .collect();
            if matches.is_empty() {
                return Vec::new();
            }
            mapped.extend(matches);
        }
        mapped
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Ordering fixtures must parse")]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn meta() -> Meta {
        serde_json::from_value(
            serde_json::json!({"id":"show", "type":"series", "name":"Example", "videos":[
                {"id":"special", "title":"Special", "season":0, "episode":1},
                {"id":"provider:second-season", "title":"Third", "season":2, "episode":1},
                {"id":"provider:first", "title":"First", "season":1, "episode":1},
                {"id":"provider:second", "title":"Second", "season":1, "episode":2}
            ]}),
        )
        .unwrap()
    }

    #[test]
    fn absolute_order_excludes_specials_preserves_provider_ids_and_crosses_seasons() {
        let order = Ordering::from_meta(&meta(), EpisodeOrder::Absolute).unwrap();
        let file = VideoFile {
            path: PathBuf::from("Example - 003.mkv"),
            episode: None,
            corrected: false,
            available: true,
        };
        let mapped = order.resolve(&file);
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].id, "provider:second-season");
        assert_eq!((mapped[0].season, mapped[0].episode), (2, 1));
    }

    #[test]
    fn incomplete_multi_episode_mapping_does_not_partially_match() {
        let order = Ordering::from_meta(&meta(), EpisodeOrder::Absolute).unwrap();
        let file = VideoFile {
            path: PathBuf::from("Example - 003-004.mkv"),
            episode: None,
            corrected: false,
            available: true,
        };
        assert!(order.resolve(&file).is_empty());
    }

    #[test]
    fn missing_episodes_are_not_compressed_into_a_false_absolute_order() {
        let mut meta = meta();
        meta.videos.retain(|video| video.id != "provider:second");
        meta.videos.push(scry_addons::Video {
            id: "late".into(),
            title: "Fourth".into(),
            season: Some(1),
            episode: Some(4),
            released: None,
            thumbnail: None,
            runtime: String::new(),
        });
        assert!(Ordering::from_meta(&meta, EpisodeOrder::Absolute).is_err());
    }

    #[test]
    fn aired_order_keeps_specials_and_manual_corrections_use_canonical_numbers() {
        let order = Ordering::from_meta(&meta(), EpisodeOrder::Aired).unwrap();
        assert!(order.episodes.iter().any(|episode| episode.id == "special"));
        let order = Ordering::from_meta(&meta(), EpisodeOrder::Absolute).unwrap();
        let file = VideoFile {
            path: PathBuf::from("Example - 001.mkv"),
            episode: Some(crate::local_media::Episode {
                season: 2,
                numbers: vec![1],
            }),
            corrected: true,
            available: true,
        };
        assert_eq!(order.resolve(&file)[0].id, "provider:second-season");
    }
}
