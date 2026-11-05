use super::{
    Item, VideoFile,
    filename::{normalized, parse_in_roots},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
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
        Self::scan_directories(&self.roots, &mut self.items, &self.roots)
    }

    pub fn scan_changed(&mut self, directories: &[PathBuf]) -> usize {
        let mut scopes = BTreeSet::<PathBuf>::new();
        for directory in directories {
            if directory
                .components()
                .any(|part| part == Component::ParentDir)
            {
                continue;
            }
            let Some(root) = self
                .roots
                .iter()
                .filter(|root| directory.starts_with(root))
                .max_by_key(|root| root.components().count())
            else {
                continue;
            };
            let scope = scan_scope(root, directory);
            if scopes.iter().any(|parent| scope.starts_with(parent)) {
                continue;
            }
            scopes.retain(|child| !child.starts_with(&scope));
            scopes.insert(scope);
        }
        Self::scan_directories(
            &self.roots,
            &mut self.items,
            &scopes.into_iter().collect::<Vec<_>>(),
        )
    }

    fn scan_directories(
        roots: &[PathBuf],
        current: &mut Vec<Item>,
        directories: &[PathBuf],
    ) -> usize {
        if directories.is_empty() {
            return 0;
        }
        let (found, incomplete) = discover(roots, directories);
        let mut previous = BTreeMap::new();
        let mut items = BTreeMap::new();
        for mut item in std::mem::take(current) {
            for mut file in item.files.drain(..) {
                let path = std::mem::take(&mut file.path);
                previous.insert(path, (item.id.clone(), file));
            }
            items.insert(item.id.clone(), item);
        }
        for path in found {
            if let Some((id, mut file)) = previous.remove(&path) {
                if let Some(item) = items.get_mut(&id) {
                    file.path = path;
                    file.available = true;
                    item.files.push(file);
                }
                continue;
            }
            let parsed = parse_in_roots(&path, roots);
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
            let untouched = !directories.iter().any(|dir| path.starts_with(dir));
            if (untouched || incomplete.iter().any(|dir| path.starts_with(dir)))
                && let Some(item) = items.get_mut(&id)
            {
                if !untouched {
                    file.available = false;
                }
                file.path = path;
                item.files.push(file);
            }
        }
        *current = items
            .into_values()
            .filter(|i| !i.files.is_empty())
            .collect();
        current.sort_by_cached_key(|i| i.title.to_lowercase());
        incomplete.len()
    }
}

// Deleted directories must be reconciled through a surviving parent. Never climb above a
// selected root: a disconnected drive is offline, not an empty library.
fn scan_scope(root: &Path, directory: &Path) -> PathBuf {
    let mut scope = directory;
    for ancestor in directory.ancestors().take_while(|path| *path != root) {
        if fs::symlink_metadata(ancestor).is_ok_and(|meta| meta.file_type().is_symlink()) {
            scope = ancestor.parent().unwrap_or(root);
        }
    }
    while scope != root {
        match fs::symlink_metadata(scope) {
            Ok(meta) if meta.is_dir() => break,
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => break,
            _ => scope = scope.parent().unwrap_or(root),
        }
    }
    scope.to_owned()
}

fn discover(roots: &[PathBuf], directories: &[PathBuf]) -> (BTreeSet<PathBuf>, Vec<PathBuf>) {
    let mut found = BTreeSet::new();
    let mut incomplete = Vec::new();
    let mut visited = BTreeSet::new();
    let mut pending = directories.to_vec();
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
                incomplete.extend_from_slice(directories);
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
        [
            "mkv", "mp4", "avi", "mov", "m4v", "webm", "ts", "m2ts", "mpg", "mpeg", "flv", "wmv",
            "ogv", "vob", "mxf", "3gp", "3g2", "asf", "rmvb",
        ]
        .iter()
        .any(|extension| e.eq_ignore_ascii_case(extension))
    })
}
