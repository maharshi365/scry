//! Best-effort Stremio watch history parsing, independent of UI and storage.
mod bitfield;
#[cfg(test)]
mod tests;

use anyhow::{Result, bail, ensure};
use scry_addons::Meta;
use scry_library::{HistoryEntry, HistoryImport, Media, MediaKey, MediaKind, Progress};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
};

pub(crate) const MAX_ITEMS: usize = 5_000;
const MAX_BYTES: u64 = 16 * 1024 * 1024;
pub(crate) const MAX_RECORDS: usize = 50_000;

#[derive(Clone, Debug)]
pub(crate) struct Issue {
    pub title: String,
    pub reason: &'static str,
}

#[derive(Debug, Default)]
pub(crate) struct Preview {
    pub input: HistoryImport,
    pub issues: Vec<Issue>,
    pub titles: usize,
}

impl Preview {
    pub fn issue(&mut self, title: &str, reason: &'static str) {
        self.issues.push(Issue {
            title: safe_title(title),
            reason,
        });
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct Item {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "_mtime")]
    modified: Option<String>,
    #[serde(default = "removed_by_default")]
    removed: bool,
    #[serde(default)]
    temp: bool,
    state: WatchState,
}

fn removed_by_default() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct WatchState {
    #[serde(default)]
    time_offset: u64,
    #[serde(default)]
    duration: u64,
    #[serde(default)]
    times_watched: u32,
    #[serde(default)]
    flagged_watched: u32,
    #[serde(default, rename = "video_id")]
    video_id: Option<String>,
    #[serde(default)]
    watched: Option<String>,
}

fn safe_title(value: &str) -> String {
    if value.is_empty()
        || value.len() > 200
        || value.contains("://")
        || value.chars().any(char::is_control)
    {
        "Untitled record".into()
    } else {
        value.into()
    }
}

pub(crate) fn read(path: &Path) -> Result<(Vec<Item>, Preview)> {
    let file =
        std::fs::File::open(path).map_err(|_| anyhow::anyhow!("Cannot read the selected file"))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| anyhow::anyhow!("Cannot read the selected file"))?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "Import files must be 16 MiB or smaller"
    );
    parse(&bytes)
}

pub(crate) fn parse(bytes: &[u8]) -> Result<(Vec<Item>, Preview)> {
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "Import files must be 16 MiB or smaller"
    );
    let root: Value = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("The selected file is not valid JSON"))?;
    let mut rows = Vec::new();
    if let Some(array) = root.as_array() {
        rows.extend(array);
    } else if let Some(object) = root.as_object() {
        let library_user = root.pointer("/library/uid").and_then(Value::as_str);
        let recent_user = root.pointer("/library_recent/uid").and_then(Value::as_str);
        ensure!(
            library_user.is_none() || recent_user.is_none() || library_user == recent_user,
            "The library buckets belong to different accounts"
        );
        for key in ["library", "library_recent", "libraryItems"] {
            if let Some(bucket) = object.get(key) {
                collect_rows(bucket, &mut rows)?;
            }
        }
        if rows.is_empty() && object.contains_key("items") {
            collect_rows(&root, &mut rows)?;
        }
    }
    ensure!(
        !rows.is_empty(),
        "No Stremio library records found. Select a library JSON export"
    );
    ensure!(
        rows.len() <= MAX_ITEMS,
        "Import files must contain at most 5000 records"
    );
    let mut preview = Preview::default();
    let mut items = BTreeMap::new();
    let mut ambiguous = BTreeSet::new();
    for row in rows {
        let Ok(item) = Item::deserialize(row) else {
            preview.issue(
                row.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Untitled record"),
                "Record has invalid or missing fields",
            );
            continue;
        };
        let key = (item.kind.clone(), item.id.clone());
        if ambiguous.contains(&key) {
            continue;
        }
        let date = item
            .modified
            .as_deref()
            .and_then(crate::media_metadata::release_timestamp);
        match items.get(&key) {
            Some((old, old_date)) if date == *old_date && old != &item => {
                preview.issue(
                    &item.name,
                    "Conflicting duplicate records skipped: newest state cannot be determined",
                );
                items.remove(&key);
                ambiguous.insert(key);
            }
            Some((_, old_date)) if date <= *old_date => {
                preview.issue(
                    &item.name,
                    "Duplicate record ignored; newest available state kept",
                );
            }
            Some(_) => {
                preview.issue(
                    &item.name,
                    "Duplicate record ignored; newest available state kept",
                );
                items.insert(key, (item, date));
            }
            None => {
                items.insert(key, (item, date));
            }
        }
    }
    Ok((items.into_values().map(|(item, _)| item).collect(), preview))
}

fn collect_rows<'a>(bucket: &'a Value, rows: &mut Vec<&'a Value>) -> Result<()> {
    let bucket = bucket.get("items").unwrap_or(bucket);
    if let Some(array) = bucket.as_array() {
        rows.extend(array);
    } else if let Some(map) = bucket.as_object() {
        rows.extend(map.values());
    } else {
        bail!("Invalid Stremio library container");
    }
    Ok(())
}

fn imdb(value: &str) -> bool {
    value.len() <= 32
        && value
            .strip_prefix("tt")
            .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

pub(crate) fn supported(item: &Item) -> bool {
    matches!(item.kind.as_str(), "movie" | "series") && imdb(&item.id)
}

pub(crate) fn needs_metadata(item: &Item) -> bool {
    supported(item)
        && item.kind == "series"
        && item
            .state
            .watched
            .as_deref()
            .is_some_and(|value| !value.is_empty())
}

fn episode_id(parent: &str, id: &str) -> bool {
    let mut parts = id.split(':');
    parts.next() == Some(parent)
        && parts
            .next()
            .is_some_and(|v| v.parse::<u32>().is_ok_and(|n| n.to_string() == v))
        && parts
            .next()
            .is_some_and(|v| v.parse::<u32>().is_ok_and(|n| n > 0 && n.to_string() == v))
        && parts.next().is_none()
}

pub(crate) fn append(preview: &mut Preview, item: &Item, meta: Option<&Meta>) {
    if !supported(item) {
        preview.issue(
            &item.name,
            "Unsupported ID or media type; no title matching was attempted",
        );
        return;
    }
    let title = safe_title(&item.name);
    let kind = if item.kind == "movie" {
        MediaKind::Movie
    } else {
        MediaKind::Series
    };
    let parent = Media {
        key: MediaKey::new("imdb", kind, &item.id),
        title: title.clone(),
        parent: None,
    };
    let before = preview.input.entries.len();
    let mut entries = BTreeMap::new();
    if kind == MediaKind::Movie && (item.state.flagged_watched > 0 || item.state.times_watched > 0)
    {
        entries.insert(
            item.id.clone(),
            HistoryEntry {
                media: parent.clone(),
                watched: true,
                progress: None,
                active: false,
            },
        );
    }
    let meta = meta
        .filter(|meta| meta.id == item.id && meta.kind == item.kind && meta.videos.len() <= 10_000);
    let inventory = meta
        .filter(|_| kind == MediaKind::Series)
        .map(|meta| crate::media_metadata::episode_inventory(&parent.key, meta))
        .unwrap_or_default();
    let inventory = if kind == MediaKind::Series
        && meta.is_some_and(|meta| meta.videos.len() != inventory.len())
    {
        preview.issue(
            &title,
            "Episode history skipped: duplicate or invalid metadata identities",
        );
        Vec::new()
    } else {
        inventory
    };
    append_watched(preview, item, &inventory, &mut entries);
    append_resume(preview, item, &parent, &inventory, &mut entries);
    let extra = entries.len()
        + inventory.len()
        + usize::from(kind == MediaKind::Series && !entries.is_empty());
    if preview.input.entries.len()
        + preview.input.inventory.len()
        + preview.input.parents.len()
        + extra
        > MAX_RECORDS
    {
        preview.issue(&title, "History limit reached; this title was skipped");
        return;
    }
    if !entries.is_empty() && kind == MediaKind::Series {
        preview.input.parents.push(parent);
        preview.input.inventory.extend(inventory);
    }
    preview.input.entries.extend(entries.into_values());
    if preview.input.entries.len() > before {
        preview.titles += 1;
    } else {
        preview.issue(
            &title,
            "No safely importable watched state or resume position",
        );
    }
}

fn append_watched(
    preview: &mut Preview,
    item: &Item,
    inventory: &[scry_library::EpisodeRelease],
    entries: &mut BTreeMap<String, HistoryEntry>,
) {
    let title = safe_title(&item.name);
    if item.kind == "series"
        && let Some(encoded) = item.state.watched.as_deref().filter(|v| !v.is_empty())
    {
        match bitfield::decode(encoded, &item.id, inventory) {
            Ok(ids) => {
                let mut skipped_release = false;
                for release in inventory {
                    if ids.contains(release.media.key.id.as_str()) {
                        if scry_library::can_mark_watched(
                            MediaKind::Episode,
                            release.season,
                            release.released_at,
                            chrono::Utc::now().timestamp(),
                        ) {
                            entries.insert(
                                release.media.key.id.clone(),
                                HistoryEntry {
                                    media: release.media.clone(),
                                    watched: true,
                                    progress: None,
                                    active: false,
                                },
                            );
                        } else {
                            skipped_release = true;
                        }
                    }
                }
                if skipped_release {
                    preview.issue(&title, "Some episode watches were skipped: release date is unknown or in the future");
                }
            }
            Err(reason) => preview.issue(&title, reason),
        }
    }
}

fn append_resume(
    preview: &mut Preview,
    item: &Item,
    parent: &Media,
    inventory: &[scry_library::EpisodeRelease],
    entries: &mut BTreeMap<String, HistoryEntry>,
) {
    let title = &parent.title;
    let kind = parent.key.kind;
    if item.state.time_offset > 0 {
        if let Some(sample) = sample(&item.state) {
            let video = item
                .state
                .video_id
                .as_deref()
                .unwrap_or(if kind == MediaKind::Movie {
                    &item.id
                } else {
                    ""
                });
            let media = if kind == MediaKind::Movie && video == item.id {
                Some(parent.clone())
            } else if kind == MediaKind::Series && episode_id(&item.id, video) {
                Some(
                    inventory
                        .iter()
                        .find(|r| r.media.key.id == video)
                        .map_or_else(
                            || Media {
                                key: MediaKey::new("imdb", MediaKind::Episode, video),
                                title: format!("{title} · {video}"),
                                parent: Some(parent.key.clone()),
                            },
                            |r| r.media.clone(),
                        ),
                )
            } else {
                None
            };
            if let Some(media) = media {
                let entry = entries.entry(media.key.id.clone()).or_insert(HistoryEntry {
                    media,
                    watched: false,
                    progress: None,
                    active: false,
                });
                entry.progress = Some(sample);
                entry.active = !item.removed || item.temp;
            } else {
                preview.issue(
                    title,
                    "Resume position skipped: missing or conflicting video ID",
                );
            }
        } else {
            preview.issue(title, "Resume position skipped: invalid duration or offset");
        }
    }
}

fn sample(state: &WatchState) -> Option<Progress> {
    // Bound conversion to exactly representable millisecond integers, and reject
    // offsets beyond the file rather than silently converting them to completion.
    let duration = u32::try_from(state.duration).ok()?;
    let offset = u32::try_from(state.time_offset).ok()?;
    if offset > duration {
        return None;
    }
    Progress::new(f64::from(offset) / 1000.0, f64::from(duration) / 1000.0)
}
