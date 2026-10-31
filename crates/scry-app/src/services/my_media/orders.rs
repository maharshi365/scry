use crate::local_media::{EpisodeOrder, Item, OrderedEpisode, Ordering, normalized};
use anyhow::{Context, Result, ensure};
use scry_addons::{Client, Meta, http_cache::HttpCache};
use serde::{Deserialize, de::DeserializeOwned};
use std::time::Duration;

const MAX_RESPONSE: usize = 4 * 1024 * 1024;

pub(super) struct Lookup {
    http: HttpCache,
}

impl Lookup {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("Scry/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() < 3
                    && attempt.url().host_str() == Some("api.tvmaze.com")
                {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()?;
        Ok(Self {
            http: HttpCache::new(client, None),
        })
    }

    pub async fn find(
        &self,
        client: &Client,
        item: &Item,
        mode: EpisodeOrder,
    ) -> Result<Vec<Ordering>> {
        let matched = item
            .matched
            .as_ref()
            .context("Match this show before choosing an episode order")?;
        ensure!(
            matched.kind == "series",
            "Episode ordering requires a TV show or anime series"
        );
        let endpoint = if matched.id.starts_with("kitsu:") {
            super::super::KITSU
        } else {
            super::super::CINEMETA
        };
        let addon = client.install(endpoint).await?;
        let meta = client.meta(&addon, "series", &matched.id).await?;
        if mode != EpisodeOrder::Dvd {
            return Ok(vec![Ordering::from_meta(&meta, mode)?]);
        }
        ensure!(
            matched
                .id
                .strip_prefix("tt")
                .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())),
            "DVD lookup requires an IMDb-matched series; choose its Cinemeta match first"
        );
        let show: Show = self
            .json(&format!("lookup/shows?imdb={}", matched.id))
            .await?;
        ensure!(
            show.externals.imdb.as_deref() == Some(matched.id.as_str()),
            "DVD provider returned a different series"
        );
        let lists: Vec<DvdList> = self
            .json(&format!("shows/{}/alternatelists", show.id))
            .await?;
        let mut choices = Vec::new();
        for list in lists.into_iter().filter(|list| list.dvd_release).take(5) {
            let episodes: Vec<DvdEpisode> = self
                .json(&format!(
                    "alternatelists/{}/alternateepisodes?embed=episodes",
                    list.id
                ))
                .await?;
            choices.push(dvd_order(&meta, &list, episodes)?);
        }
        ensure!(
            !choices.is_empty(),
            "TVmaze has no DVD order for this series; the existing order has not changed"
        );
        Ok(choices)
    }

    async fn json<T: DeserializeOwned + 'static>(&self, path: &str) -> Result<T> {
        let url = url::Url::parse(&format!("https://api.tvmaze.com/{path}"))?;
        let response = self.http.get_json::<T>(url, MAX_RESPONSE, true).await?;
        Ok(serde_json::from_slice(&response.bytes)?)
    }
}

#[derive(Deserialize)]
struct Show {
    id: u64,
    externals: Externals,
}
#[derive(Deserialize)]
struct Externals {
    imdb: Option<String>,
}
#[derive(Deserialize)]
struct DvdList {
    id: u64,
    url: String,
    dvd_release: bool,
}
#[derive(Deserialize)]
struct DvdEpisode {
    season: u32,
    number: Option<u32>,
    #[serde(rename = "_embedded")]
    embedded: Embedded,
}
#[derive(Deserialize)]
struct Embedded {
    episodes: Vec<AiredEpisode>,
}
#[derive(Deserialize)]
struct AiredEpisode {
    season: u32,
    number: Option<u32>,
    name: String,
}

fn dvd_order(meta: &Meta, list: &DvdList, episodes: Vec<DvdEpisode>) -> Result<Ordering> {
    ensure!(
        episodes.len() <= 20_000,
        "DVD episode inventory is too large"
    );
    let mut mapped = Vec::new();
    for episode in episodes {
        let Some(number) = episode.number else {
            continue;
        };
        ensure!(
            !episode.embedded.episodes.is_empty(),
            "DVD position has no aired episode mapping"
        );
        for aired in episode.embedded.episodes {
            let title = normalized(&aired.name);
            let candidates: Vec<_> = meta
                .videos
                .iter()
                .filter(|video| !title.is_empty() && normalized(&video.title) == title)
                .collect();
            let target = if candidates.len() == 1 { candidates.first().copied() }
                else { candidates.into_iter().find(|video| video.season == Some(aired.season) && video.episode == aired.number) }
                .context("DVD and metadata episode titles disagree; keep the current order and correct the match")?;
            let (Some(season), Some(target_number)) = (target.season, target.episode) else {
                anyhow::bail!("DVD target has no canonical episode number");
            };
            mapped.push(OrderedEpisode {
                file_season: episode.season,
                file_episode: number,
                id: target.id.clone(),
                title: target.title.clone(),
                season,
                episode: target_number,
                released: target.released.clone(),
            });
        }
    }
    let label = url::Url::parse(&list.url)
        .ok()
        .and_then(|url| url.path_segments()?.next_back().map(str::to_owned))
        .unwrap_or_else(|| format!("DVD list {}", list.id))
        .replace('-', " ");
    let order = Ordering {
        series_id: meta.id.clone(),
        mode: EpisodeOrder::Dvd,
        name: label,
        source: Some(format!("https://www.tvmaze.com/alternatelists/{}", list.id)),
        episodes: mapped,
    };
    order.validate()?;
    Ok(order)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "DVD fixtures must parse")]
mod tests {
    use super::*;

    #[test]
    fn dvd_numbers_map_to_provider_ids_even_when_provider_airing_numbers_differ() {
        let meta: Meta = serde_json::from_value(
            serde_json::json!({"id":"show", "type":"series", "name":"Example", "videos":[
                {"id":"native:pilot", "title":"Pilot", "season":1, "episode":1},
                {"id":"native:second", "title":"The next day", "season":1, "episode":2}
            ]}),
        )
        .unwrap();
        let list = DvdList {
            id: 1,
            url: "https://www.tvmaze.com/alternatelists/1/example-dvd".into(),
            dvd_release: true,
        };
        let episodes: Vec<DvdEpisode> = serde_json::from_value(serde_json::json!([
            {"season":1, "number":1, "_embedded":{"episodes":[{"season":1, "number":11, "name":"Pilot"}]}},
            {"season":1, "number":2, "_embedded":{"episodes":[{"season":1, "number":1, "name":"The next day"}]}}
        ])).unwrap();
        let order = dvd_order(&meta, &list, episodes).unwrap();
        assert_eq!(order.episodes[0].id, "native:pilot");
        assert_eq!(order.episodes[1].id, "native:second");
        assert_eq!(
            order.source.as_deref(),
            Some("https://www.tvmaze.com/alternatelists/1")
        );
    }

    #[test]
    fn conflicting_episode_titles_are_not_treated_as_an_airing_order_fallback() {
        let meta: Meta = serde_json::from_value(
            serde_json::json!({"id":"show", "type":"series", "name":"Example", "videos":[
                {"id":"native:pilot", "title":"Pilot", "season":1, "episode":1}
            ]}),
        )
        .unwrap();
        let list = DvdList {
            id: 1,
            url: "https://www.tvmaze.com/alternatelists/1".into(),
            dvd_release: true,
        };
        let episodes: Vec<DvdEpisode> = serde_json::from_value(serde_json::json!([
            {"season":1, "number":1, "_embedded":{"episodes":[{"season":1, "number":1, "name":"Different episode"}]}}
        ])).unwrap();
        assert!(dvd_order(&meta, &list, episodes).is_err());
    }
}
