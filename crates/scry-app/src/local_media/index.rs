use super::{
    Item, VideoFile,
    filename::{normalized, parse_in_roots},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const ENTRY_LIMIT: usize = 1_000_000;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Index {
    pub roots: Vec<PathBuf>,
    pub items: Vec<Item>,
}

impl Index {
    pub fn add_root(&mut self, path: &Path) -> Result<()> {
        let path = fs::canonicalize(path).context("Cannot open this media folder")?;
        ensure!(path.is_dir(), "Select a folder, not a file");
        if !self.roots.contains(&path) {
            self.roots.push(path);
        }
        Ok(())
    }

    pub fn scan(&mut self) -> usize {
        let previous: BTreeMap<_, _> = self
            .items
            .iter()
            .flat_map(|item| {
                item.files
                    .iter()
                    .map(move |f| (f.path.clone(), (item.id.clone(), f.clone())))
            })
            .collect();
        let mut items: BTreeMap<_, _> = self
            .items
            .iter()
            .cloned()
            .map(|mut item| {
                item.files.clear();
                (item.id.clone(), item)
            })
            .collect();
        let (found, incomplete) = discover(&self.roots);
        let mut previous = previous;
        for path in found {
            if let Some((id, mut file)) = previous.remove(&path) {
                if let Some(item) = items.get_mut(&id) {
                    file.available = true;
                    item.files.push(file);
                }
                continue;
            }
            let parsed = parse_in_roots(&path, &self.roots);
            let kind = if parsed.episode.is_some() || parsed.absolute {
                "series"
            } else {
                "movie"
            };
            let identity = format!("{kind}:{}:{:?}", normalized(&parsed.title), parsed.year);
            let id = format!("local:{}", blake3::hash(identity.as_bytes()).to_hex());
            let item = items.entry(id.clone()).or_insert_with(|| Item {
                id,
                title: parsed.title,
                year: parsed.year,
                kind: kind.into(),
                files: Vec::new(),
                matched: None,
                keep_unmatched: false,
                ordering: None,
            });
            item.files.push(VideoFile {
                path,
                episode: parsed.episode,
                corrected: false,
                available: true,
            });
        }
        for (path, (id, mut file)) in previous {
            if incomplete.iter().any(|dir| path.starts_with(dir))
                && let Some(item) = items.get_mut(&id)
            {
                file.available = false;
                item.files.push(file);
            }
        }
        self.items = items
            .into_values()
            .filter(|i| !i.files.is_empty())
            .collect();
        self.items.sort_by_cached_key(|i| i.title.to_lowercase());
        incomplete.len()
    }
}

fn discover(roots: &[PathBuf]) -> (BTreeSet<PathBuf>, Vec<PathBuf>) {
    let mut found = BTreeSet::new();
    let mut incomplete = Vec::new();
    let mut visited = BTreeSet::new();
    let mut pending = roots.to_vec();
    let mut entries = 0usize;
    while let Some(dir) = pending.pop() {
        let Ok(canonical) = fs::canonicalize(&dir) else {
            incomplete.push(dir);
            continue;
        };
        // Resolve junctions too, so a selected folder cannot escape to another drive.
        if !roots.iter().any(|root| canonical.starts_with(root)) || !visited.insert(canonical) {
            continue;
        }
        let Ok(children) = fs::read_dir(&dir) else {
            incomplete.push(dir);
            continue;
        };
        for child in children {
            entries += 1;
            if entries > ENTRY_LIMIT {
                incomplete.extend_from_slice(roots);
                pending.clear();
                break;
            }
            let Ok(child) = child else {
                incomplete.push(dir.clone());
                continue;
            };
            let path = child.path();
            let Ok(kind) = child.file_type() else {
                incomplete.push(dir.clone());
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() && is_video(&path) {
                found.insert(path);
            }
        }
    }
    (found, incomplete)
}

pub(crate) fn is_video(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "mkv"
                | "mp4"
                | "avi"
                | "mov"
                | "m4v"
                | "webm"
                | "ts"
                | "m2ts"
                | "mpg"
                | "mpeg"
                | "flv"
                | "wmv"
                | "ogv"
                | "vob"
                | "mxf"
                | "3gp"
                | "3g2"
                | "asf"
                | "rmvb"
        )
    })
}
