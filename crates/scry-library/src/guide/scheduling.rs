use super::{
    Channel, ChannelSource, Collection, EpisodeOrder, GuideMetadata, HORIZON, MAX_CHANNELS,
    Program, Slot, SourcePolicy,
};
use crate::{Media, MediaKey, MediaKind};
use anyhow::Result;
use diesel::{
    prelude::*,
    sql_types::{BigInt, Bool, Nullable, Text},
};
use std::collections::{BTreeMap, HashMap};

#[derive(QueryableByName)]
struct CandidateRow {
    #[diesel(sql_type = Text)]
    namespace: String,
    #[diesel(sql_type = Text)]
    kind: String,
    #[diesel(sql_type = Text)]
    external_id: String,
    #[diesel(sql_type = Text)]
    title: String,
    #[diesel(sql_type = Nullable<Text>)]
    parent_namespace: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    parent_external_id: Option<String>,
    #[diesel(sql_type = Text)]
    show_title: String,
    #[diesel(sql_type = Nullable<BigInt>)]
    season: Option<i64>,
    #[diesel(sql_type = Nullable<BigInt>)]
    episode: Option<i64>,
    #[diesel(sql_type = Nullable<BigInt>)]
    duration: Option<i64>,
    #[diesel(sql_type = Bool)]
    watched: bool,
    #[diesel(sql_type = Bool)]
    saved: bool,
    #[diesel(sql_type = Text)]
    metadata: String,
}

struct Candidate {
    program: Program,
    genres: Vec<String>,
    watched: bool,
    saved: bool,
}

fn candidates(connection: &mut SqliteConnection, now: i64) -> Result<Vec<Candidate>> {
    let rows = diesel::sql_query("SELECT m.namespace,m.kind,m.external_id,m.title,p.namespace AS parent_namespace,p.external_id AS parent_external_id,COALESCE(p.title,m.title) AS show_title,e.season,e.episode,CAST(COALESCE(CASE WHEN CAST(pr.duration_seconds AS INTEGER) BETWEEN 60 AND 43200 THEN pr.duration_seconds END,r.seconds) AS INTEGER) AS duration,COALESCE(w.watched,0) AS watched,EXISTS(SELECT 1 FROM watchlist l WHERE l.media_id=COALESCE(p.media_id,m.media_id)) AS saved,COALESCE(g.payload,'{}') AS metadata FROM media m LEFT JOIN media p ON p.media_id=m.parent_id JOIN episode_releases e ON e.media_id=m.media_id LEFT JOIN progress pr ON pr.media_id=m.media_id LEFT JOIN media_runtime r ON r.media_id=m.media_id LEFT JOIN watched w ON w.media_id=m.media_id LEFT JOIN guide_metadata g ON g.media_id=COALESCE(p.media_id,m.media_id) WHERE e.released_at<=? AND (m.kind='movie' OR (m.kind='episode' AND e.season>0 AND e.episode>0)) AND (EXISTS(SELECT 1 FROM watchlist l WHERE l.media_id=COALESCE(p.media_id,m.media_id)) OR w.watched=1 OR EXISTS(SELECT 1 FROM playlist_items pi WHERE pi.media_id=COALESCE(p.media_id,m.media_id))) ORDER BY m.media_id LIMIT 20000")
        .bind::<BigInt,_>(now).load::<CandidateRow>(connection)?;
    rows.into_iter()
        .map(|row| {
            let kind = MediaKind::parse(&row.kind)?;
            let metadata: GuideMetadata = serde_json::from_str(&row.metadata)?;
            let known = row
                .duration
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| (60..=43_200).contains(n));
            Ok(Candidate {
                program: Program {
                    media: Media {
                        key: MediaKey::new(row.namespace, kind, row.external_id),
                        title: row.title,
                        parent: row
                            .parent_namespace
                            .zip(row.parent_external_id)
                            .map(|(namespace, id)| MediaKey::new(namespace, MediaKind::Series, id)),
                    },
                    show_title: row.show_title,
                    season: row.season.and_then(|n| n.try_into().ok()),
                    episode: row.episode.and_then(|n| n.try_into().ok()),
                    description: metadata.description,
                    artwork: metadata.artwork,
                    duration: known.unwrap_or(if kind == MediaKind::Movie { 7200 } else { 1800 }),
                    estimated: known.is_none(),
                },
                genres: metadata.genres,
                watched: row.watched,
                saved: row.saved,
            })
        })
        .collect()
}

fn automatic(
    channels: &mut Vec<Channel>,
    id: String,
    name: String,
    kind: MediaKind,
    source: ChannelSource,
) {
    if channels.len() < MAX_CHANNELS && !channels.iter().any(|channel| channel.id == id) {
        channels.push(Channel {
            id,
            name,
            kind,
            source,
            slots: Vec::new(),
        });
    }
}

pub(super) fn extend(
    connection: &mut SqliteConnection,
    channels: &mut Vec<Channel>,
    collections: &[Collection],
    policy: &SourcePolicy,
    now: i64,
) -> Result<()> {
    let candidates = candidates(connection, now)?;
    for (id, name, kind, source) in [
        (
            "comfort",
            "Comfort TV",
            MediaKind::Series,
            ChannelSource::Comfort,
        ),
        (
            "library-tv",
            "Library Mix",
            MediaKind::Series,
            ChannelSource::Library,
        ),
        (
            "library-movies",
            "Movie Night",
            MediaKind::Movie,
            ChannelSource::Library,
        ),
        (
            "comfort-movies",
            "Movie Rewinds",
            MediaKind::Movie,
            ChannelSource::Comfort,
        ),
    ] {
        if candidates
            .iter()
            .any(|candidate| matches_channel(candidate, kind, &source, collections))
        {
            automatic(channels, id.into(), name.into(), kind, source);
        }
    }
    let mut genres = BTreeMap::<String, Vec<MediaKey>>::new();
    for candidate in candidates
        .iter()
        .filter(|c| c.saved && c.program.media.key.kind == MediaKind::Episode)
    {
        for genre in &candidate.genres {
            let keys = genres.entry(genre.clone()).or_default();
            let key = group_key(&candidate.program);
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
    }
    for (genre, shows) in genres
        .into_iter()
        .filter(|(_, shows)| shows.len() >= 2)
        .take(6)
    {
        let _ = shows;
        automatic(
            channels,
            format!("genre-{genre}"),
            genre.clone(),
            MediaKind::Series,
            ChannelSource::Genre(genre),
        );
    }
    for channel in channels {
        // Keep a bounded recent tail for repeat avoidance, and never rewrite published slots.
        channel
            .slots
            .retain(|slot| slot.ends_at() > now.saturating_sub(HORIZON));
        let eligible: Vec<_> = candidates
            .iter()
            .filter(|candidate| {
                matches_channel(candidate, channel.kind, &channel.source, collections)
                    && (!policy.watched_only || candidate.watched)
            })
            .collect();
        fill(channel, &eligible, policy.order, now);
    }
    Ok(())
}

fn matches_channel(
    candidate: &Candidate,
    kind: MediaKind,
    source: &ChannelSource,
    collections: &[Collection],
) -> bool {
    if (candidate.program.media.key.kind == MediaKind::Movie) != (kind == MediaKind::Movie) {
        return false;
    }
    match source {
        ChannelSource::Library => candidate.saved,
        ChannelSource::Comfort => candidate.watched,
        ChannelSource::Genre(genre) => candidate.saved && candidate.genres.contains(genre),
        ChannelSource::Collection(id) => collections
            .iter()
            .find(|group| group.playlist.id == *id)
            .is_some_and(|group| {
                group
                    .members
                    .iter()
                    .any(|member| &member.key == group_key(&candidate.program))
            }),
    }
}

fn group_key(program: &Program) -> &MediaKey {
    program.media.parent.as_ref().unwrap_or(&program.media.key)
}

fn hash(value: &str, seed: u64) -> u64 {
    value
        .bytes()
        .fold(seed ^ 0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
}

fn fill(channel: &mut Channel, candidates: &[&Candidate], order: EpisodeOrder, now: i64) {
    let mut groups: BTreeMap<(&str, &str), Vec<&Program>> = BTreeMap::new();
    for candidate in candidates {
        let key = group_key(&candidate.program);
        groups
            .entry((key.namespace.as_str(), key.id.as_str()))
            .or_default()
            .push(&candidate.program);
    }
    for programs in groups.values_mut() {
        programs.sort_by_key(|p| (p.season, p.episode, p.media.key.id.clone()));
    }
    let mut recent: HashMap<MediaKey, i64> = channel
        .slots
        .iter()
        .map(|slot| (slot.program.media.key.clone(), slot.starts_at))
        .collect();
    // New channels begin at the day's boundary, not at app launch. Their first
    // tune therefore joins an already-running program instead of resetting it.
    let anchor = now - now.rem_euclid(HORIZON);
    let mut starts_at = channel.slots.last().map_or(anchor, Slot::ends_at);
    if starts_at < anchor {
        starts_at = anchor;
    }
    while starts_at < now.saturating_add(HORIZON) && channel.slots.len() < 3000 {
        let previous = channel.slots.last().map(|slot| group_key(&slot.program));
        let seed = hash(&channel.id, u64::try_from(starts_at).unwrap_or_default());
        let Some((_, programs)) = groups
            .iter()
            .filter(|(_, programs)| {
                groups.len() == 1
                    || programs
                        .first()
                        .is_some_and(|p| Some(group_key(p)) != previous)
            })
            .min_by_key(|((namespace, id), _)| hash(id, hash(namespace, seed)))
        else {
            break;
        };
        let program = if order == EpisodeOrder::Sequential {
            let last = channel.slots.iter().rev().find(|slot| {
                programs
                    .first()
                    .is_some_and(|p| group_key(p) == group_key(&slot.program))
            });
            let next = last
                .and_then(|slot| {
                    programs
                        .iter()
                        .position(|p| p.media.key == slot.program.media.key)
                })
                .map_or(0, |index| (index + 1) % programs.len());
            programs.get(next).copied()
        } else {
            programs.iter().copied().min_by_key(|program| {
                (
                    recent.get(&program.media.key).copied().unwrap_or(i64::MIN),
                    hash(&program.media.key.id, seed),
                )
            })
        };
        let Some(program) = program else {
            break;
        };
        recent.insert(program.media.key.clone(), starts_at);
        channel.slots.push(Slot {
            starts_at,
            program: program.clone(),
        });
        starts_at = starts_at.saturating_add(i64::from(program.duration));
    }
}
