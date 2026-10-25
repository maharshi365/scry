//! Metadata-only source ranking. Swarm counts are hints, not measured throughput.
use scry_addons::Stream;
use scry_library::guide::SourcePolicy;
use std::{cmp::Reverse, collections::BTreeMap};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Metrics {
    pub seeders: Option<u64>,
    pub bytes: Option<u64>,
}

impl Metrics {
    pub(super) fn read(stream: &Stream) -> Self {
        let labels = [&stream.title, &stream.description];
        Self {
            seeders: stream
                .behavior_hints
                .get("seeders")
                .and_then(serde_json::Value::as_u64)
                .or_else(|| {
                    labels.iter().find_map(|label| {
                        marked(label, '👤').and_then(|text| count(text.split_whitespace().next()?))
                    })
                }),
            bytes: stream
                .behavior_hints
                .get("videoSize")
                .and_then(serde_json::Value::as_u64)
                .filter(|bytes| *bytes > 0)
                .or_else(|| {
                    labels
                        .iter()
                        .find_map(|label| marked(label, '💾').and_then(size))
                }),
        }
    }

    fn availability(self) -> u8 {
        match self.seeders {
            // A soft preference, not a claim that ten peers guarantee playback.
            Some(10..) => 0,
            Some(1..=9) => 1,
            None => 2,
            Some(0) => 3,
        }
    }
}

fn marked(text: &str, marker: char) -> Option<&str> {
    text.lines().take(32).find_map(|line| {
        (line.len() <= 1024)
            .then(|| line.split_once(marker).map(|(_, tail)| tail.trim_start()))
            .flatten()
    })
}

fn count(text: &str) -> Option<u64> {
    if text.is_empty()
        || text.len() > 24
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b',')
    {
        return None;
    }
    if text.contains(',') {
        let mut groups = text.split(',');
        let first = groups.next()?;
        if first.is_empty() || first.len() > 3 || groups.any(|group| group.len() != 3) {
            return None;
        }
    }
    text.replace(',', "").parse().ok()
}

fn size(text: &str) -> Option<u64> {
    let mut words = text.split_whitespace();
    let number = words.next()?;
    let unit = words.next()?.to_ascii_uppercase();
    // Torrentio's display formatter uses powers of 1024 even for MB/GB labels.
    let multiplier: u128 = match unit.as_str() {
        "B" => 1,
        "KB" | "KIB" => 1024,
        "MB" | "MIB" => 1024 * 1024,
        "GB" | "GIB" => 1024 * 1024 * 1024,
        "TB" | "TIB" => 1024u128.pow(4),
        _ => return None,
    };
    let (whole, decimal) = number.split_once('.').unwrap_or((number, ""));
    let whole = u128::from(count(whole)?).checked_mul(multiplier)?;
    if decimal.len() > 6 || !decimal.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fractional = if decimal.is_empty() {
        0
    } else {
        u128::from(decimal.parse::<u64>().ok()?).checked_mul(multiplier)?
            / 10u128.pow(u32::try_from(decimal.len()).ok()?)
    };
    u64::try_from(whole.checked_add(fractional)?)
        .ok()
        .filter(|bytes| *bytes > 0)
}

pub(super) fn is_torrent(stream: &Stream) -> bool {
    stream.url.is_none() && stream.info_hash.is_some()
}

fn resolution(text: &str) -> Option<u16> {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .find_map(|word| {
            if ["4k", "2160", "2160p", "uhd"]
                .iter()
                .any(|value| word.eq_ignore_ascii_case(value))
            {
                Some(2160)
            } else {
                let number = word
                    .strip_suffix('p')
                    .or_else(|| word.strip_suffix('P'))
                    .unwrap_or(word);
                match number {
                    "1440" => Some(1440),
                    "1080" => Some(1080),
                    "720" => Some(720),
                    "576" => Some(576),
                    "480" => Some(480),
                    "360" => Some(360),
                    "272" => Some(272),
                    _ => None,
                }
            }
        })
}

fn quality(stream: &Stream, preferred: &str) -> u8 {
    let actual = resolution(&stream.name)
        .or_else(|| {
            stream
                .behavior_hints
                .get("filename")
                .and_then(serde_json::Value::as_str)
                .and_then(resolution)
        })
        .or_else(|| resolution(&stream.title))
        .or_else(|| resolution(&stream.description));
    if preferred.is_empty() {
        match actual {
            Some(720 | 1080) => 0,
            Some(1440 | 2160) => 1,
            Some(_) => 2,
            None => 3,
        }
    } else {
        u8::from(actual != resolution(preferred))
    }
}

struct Candidate<T> {
    source: T,
    unsupported: bool,
    torrent: bool,
    metrics: Metrics,
    availability: u8,
    quality: u8,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Rank {
    unsupported: bool,
    torrent: bool,
    availability: u8,
    quality: u8,
    weak_swarm: bool,
    weak_seeders: Reverse<u64>,
    bytes: u64,
    seeders: Reverse<u64>,
}

impl<T> Candidate<T> {
    fn rank(&self, strongest: &BTreeMap<(u8, u8), u64>) -> Rank {
        let peers = self.metrics.seeders.unwrap_or_default();
        let best = strongest
            .get(&(self.availability, self.quality))
            .copied()
            .unwrap_or_default();
        // Keep at least half the strongest reported swarm in the same quality
        // group. Then prefer the smaller episode file, not a multi-GB remux.
        let weak_swarm = self.torrent && peers < best.div_ceil(2);
        Rank {
            unsupported: self.unsupported,
            torrent: self.torrent,
            availability: self.availability,
            quality: self.quality,
            weak_swarm,
            weak_seeders: Reverse(if weak_swarm { peers } else { 0 }),
            bytes: if self.torrent {
                self.metrics.bytes.unwrap_or(u64::MAX)
            } else {
                0
            },
            seeders: Reverse(if self.torrent { peers } else { 0 }),
        }
    }
}

pub(super) fn select_sources(streams: Vec<(String, Stream)>, policy: &SourcePolicy) -> Vec<Stream> {
    rank_sources(
        streams.into_iter().filter(|(provider, stream)| {
            stream.validate_playback().is_ok()
                && (policy.allow_torrents || !is_torrent(stream))
                && (policy.provider.is_empty() || provider == &policy.provider)
        }),
        |(provider, stream)| (provider.as_str(), stream),
        &policy.quality,
    )
    .into_iter()
    .map(|(_, stream)| stream)
    .collect()
}

/// Rank owned or borrowed sources without changing their playback identity.
pub(super) fn rank_sources<T>(
    streams: impl IntoIterator<Item = T>,
    describe: impl Fn(&T) -> (&str, &Stream),
    preferred: &str,
) -> Vec<T> {
    let mut candidates: Vec<_> = streams
        .into_iter()
        .map(|source| {
            let (_, stream) = describe(&source);
            let torrent = is_torrent(stream);
            let metrics = Metrics::read(stream);
            let quality = quality(stream, preferred);
            let unsupported = stream.validate_playback().is_err();
            Candidate {
                source,
                unsupported,
                torrent,
                metrics,
                availability: if torrent { metrics.availability() } else { 0 },
                quality,
            }
        })
        .collect();
    let mut strongest = BTreeMap::<_, u64>::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.torrent && !candidate.unsupported)
    {
        let best = strongest
            .entry((candidate.availability, candidate.quality))
            .or_default();
        *best = (*best).max(candidate.metrics.seeders.unwrap_or_default());
    }
    candidates.sort_by(|a, b| {
        a.rank(&strongest).cmp(&b.rank(&strongest)).then_with(|| {
            let (a_provider, a_stream) = describe(&a.source);
            let (b_provider, b_stream) = describe(&b.source);
            (a_provider, &a_stream.name, &a_stream.title).cmp(&(
                b_provider,
                &b_stream.name,
                &b_stream.title,
            ))
        })
    });
    candidates
        .into_iter()
        .map(|candidate| candidate.source)
        .collect()
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Source metadata fixtures must deserialize and contain a selected stream"
)]
mod tests;
