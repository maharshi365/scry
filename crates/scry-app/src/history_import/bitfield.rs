use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Decompress, FlushDecompress, Status};
use scry_library::EpisodeRelease;
use std::collections::BTreeSet;

const INVALID: &str = "Episode history skipped: invalid watched bitmap";
const MAPPING: &str = "Episode history skipped: episode order or anchor could not be verified";
const MAX_BITS: usize = 50_000;

pub(super) fn decode<'inventory>(
    encoded: &str,
    parent: &str,
    inventory: &'inventory [EpisodeRelease],
) -> Result<BTreeSet<&'inventory str>, &'static str> {
    if encoded.len() > 16_384 {
        return Err(INVALID);
    }
    let (prefix, payload) = encoded.rsplit_once(':').ok_or(INVALID)?;
    let (anchor, length) = prefix.rsplit_once(':').ok_or(INVALID)?;
    let length = length.parse::<usize>().map_err(|_| INVALID)?;
    if length == 0 || length > MAX_BITS {
        return Err(INVALID);
    }
    let compressed = STANDARD.decode(payload).map_err(|_| INVALID)?;
    let mut bits = vec![0; MAX_BITS / 8 + 1];
    let mut decoder = Decompress::new(true);
    let status = decoder
        .decompress(&compressed, &mut bits, FlushDecompress::Finish)
        .map_err(|_| INVALID)?;
    let output_length = usize::try_from(decoder.total_out()).map_err(|_| INVALID)?;
    if status != Status::StreamEnd
        || decoder.total_in() != compressed.len() as u64
        || output_length > MAX_BITS / 8
    {
        return Err(INVALID);
    }
    bits.truncate(output_length);
    if anchor == "undefined" && bits.iter().all(|b| *b == 0) {
        return Ok(BTreeSet::new());
    }
    if inventory.is_empty() || inventory.len() > MAX_BITS {
        return Err(MAPPING);
    }
    let mut sorted: Vec<_> = inventory.iter().collect();
    sorted.sort_by_key(|r| (r.season, r.episode, r.released_at));
    let mut seen = BTreeSet::new();
    for release in &sorted {
        let (Some(season), Some(episode)) = (release.season, release.episode) else {
            return Err(MAPPING);
        };
        if release.media.key.id != format!("{parent}:{season}:{episode}")
            || !seen.insert((season, episode))
        {
            return Err(MAPPING);
        }
    }
    // Do not apply Stremio's positional shifting heuristic: an offset can hide
    // inserted episodes and transfer a watched flag to the wrong episode.
    if sorted
        .get(length - 1)
        .is_none_or(|r| r.media.key.id != anchor)
    {
        return Err(MAPPING);
    }
    let mut ids = BTreeSet::new();
    for (byte_index, byte) in bits.iter().enumerate() {
        for bit in 0..8 {
            if byte & (1 << bit) == 0 {
                continue;
            }
            let index = byte_index * 8 + bit;
            let release = sorted.get(index).ok_or(MAPPING)?;
            if index >= length {
                return Err(MAPPING);
            }
            ids.insert(release.media.key.id.as_str());
        }
    }
    if !ids.contains(anchor) {
        return Err(MAPPING);
    }
    Ok(ids)
}
