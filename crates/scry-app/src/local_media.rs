//! Filename evidence and read-only filesystem traversal for local media.
use anyhow::{Context, Result, ensure};
use scry_addons::Meta;
use scry_library::{Library, local_media::LocalRoot};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Hints {
    pub title: String,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub explicit_id: Option<String>,
    pub conflict: bool,
    pub group: String,
}

pub(crate) fn normalize(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().collect::<String>()
            } else {
                " ".into()
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokens(value: &str) -> Vec<&str> {
    value
        .split(|c: char| c.is_whitespace() || matches!(c, '.' | '_' | '[' | ']' | '(' | ')'))
        .filter(|v| !v.is_empty())
        .collect()
}

fn number_pair(token: &str) -> Option<(u32, u32, bool)> {
    let lower = token.to_ascii_lowercase();
    let (season, rest) = if let Some(s) = lower.strip_prefix('s') {
        s.split_once('e')?
    } else {
        lower.split_once('x')?
    };
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let episode = rest.get(..end)?.parse().ok()?;
    Some((season.parse().ok()?, episode, end != rest.len()))
}

fn season_folder(value: &str) -> Option<u32> {
    let normalized = normalize(value);
    if normalized == "specials" {
        return Some(0);
    }
    normalized
        .strip_prefix("season")
        .or_else(|| normalized.strip_prefix('s'))?
        .trim()
        .parse()
        .ok()
}

fn clean_title(value: &str) -> (String, Option<u32>) {
    let mut title = Vec::new();
    let mut year = None;
    for token in tokens(value) {
        let lower = token.to_ascii_lowercase();
        if let Ok(number) = token.parse::<u32>()
            && (1900..=2099).contains(&number)
            && !title.is_empty()
        {
            year = Some(number);
            break;
        }
        if number_pair(token).is_some()
            || matches!(
                lower.as_str(),
                "720p"
                    | "1080p"
                    | "2160p"
                    | "480p"
                    | "4k"
                    | "bluray"
                    | "brrip"
                    | "webrip"
                    | "web-dl"
                    | "web"
                    | "hdtv"
                    | "dvdrip"
                    | "x264"
                    | "x265"
                    | "h264"
                    | "h265"
                    | "hevc"
                    | "remux"
            )
            || lower.starts_with("tt") && lower.chars().skip(2).all(|c| c.is_ascii_digit())
        {
            break;
        }
        title.push(token);
    }
    (
        title
            .join(" ")
            .trim_matches(|c: char| c == '-' || c.is_whitespace())
            .to_owned(),
        year,
    )
}

pub(crate) fn imdb_id(value: &str) -> Option<String> {
    value.match_indices("tt").find_map(|(start, _)| {
        let digits: String = value
            .get(start + 2..)?
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        (digits.len() >= 7 && digits.len() <= 10).then(|| format!("tt{digits}"))
    })
}

pub(crate) fn infer(path: &Path) -> Hints {
    let stem = path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or_default();
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let folder = parent
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default();
    let folder_season = season_folder(folder);
    let parts = tokens(stem);
    let pair = parts.iter().find_map(|token| number_pair(token));
    let season = pair.map(|p| p.0).or(folder_season);
    let episode = pair
        .map(|p| p.1)
        .or_else(|| folder_season.and_then(|_| parts.first()?.parse().ok()));
    let (mut title, mut year) = clean_title(stem);
    let show_folder = if folder_season.is_some() {
        parent.parent().unwrap_or(parent)
    } else {
        parent
    };
    if title.is_empty()
        || title.chars().all(|c| c.is_ascii_digit()) && year.is_none()
        || pair.is_none() && folder_season.is_some() && episode.is_some()
    {
        (title, year) = clean_title(
            show_folder
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default(),
        );
    }
    let folder_year = clean_title(folder).1;
    let conflict = path
        .components()
        .filter_map(|part| part.as_os_str().to_str())
        .any(|part| {
            tokens(part).iter().any(|token| {
                matches!(
                    token.to_ascii_lowercase().as_str(),
                    "sample" | "trailer" | "extras" | "featurettes"
                )
            })
        })
        || pair.is_some_and(|(s, _, multi)| multi || folder_season.is_some_and(|f| f != s))
        || pair.is_some()
            && parts.iter().any(|part| {
                part.to_ascii_lowercase()
                    .strip_prefix('e')
                    .is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()))
            })
        || parts
            .iter()
            .filter(|token| number_pair(token).is_some())
            .count()
            > 1
        || year.zip(folder_year).is_some_and(|(a, b)| a != b);
    Hints {
        title,
        year: year.or(folder_year),
        season,
        episode,
        explicit_id: imdb_id(&path.to_string_lossy()),
        conflict,
        group: show_folder.to_string_lossy().into_owned(),
    }
}

pub(crate) fn select_match(hints: &Hints, candidates: &[Meta]) -> Option<usize> {
    if hints.conflict {
        return None;
    }
    let mut matching = candidates.iter().enumerate().filter(|(_, meta)| {
        if let Some(id) = &hints.explicit_id {
            return &meta.id == id;
        }
        !hints.title.is_empty()
            && normalize(&meta.name) == normalize(&hints.title)
            && hints.year.is_none_or(|year| {
                meta.year.get(..4).and_then(|y| y.parse::<u32>().ok()) == Some(year)
            })
    });
    let (index, _) = matching.next()?;
    matching.next().is_none().then_some(index)
}

pub(crate) fn scan(db: &Library, root: &LocalRoot, cancel: &AtomicBool) -> Result<usize> {
    let root_path = std::fs::canonicalize(&root.path).context("Local folder is unavailable")?;
    ensure!(root_path.is_dir(), "Local folder is unavailable");
    let generation = db.begin_local_scan(root.id)?;
    let mut stack = vec![(root_path.clone(), 0_u8)];
    let mut count = 0;
    let mut directories = 0;
    while let Some((directory, depth)) = stack.pop() {
        ensure!(!cancel.load(Ordering::Relaxed), "Scan cancelled");
        directories += 1;
        ensure!(directories <= 20_000, "Folder scan directory limit reached");
        for entry in std::fs::read_dir(directory)
            .context("Cannot read a local folder; missing files were not removed")?
        {
            ensure!(!cancel.load(Ordering::Relaxed), "Scan cancelled");
            let entry = entry.context("Cannot read a local folder entry")?;
            let kind = entry
                .file_type()
                .context("Cannot inspect a local folder entry")?;
            let path = entry.path();
            if is_link(&entry)? {
                continue;
            }
            if kind.is_dir() {
                ensure!(
                    depth < 64 && stack.len() < 20_000,
                    "Folder scan depth or directory limit reached"
                );
                stack.push((path, depth + 1));
                continue;
            }
            if !kind.is_file()
                || !path.extension().and_then(|v| v.to_str()).is_some_and(|v| {
                    matches!(
                        v.to_ascii_lowercase().as_str(),
                        "mkv"
                            | "mp4"
                            | "avi"
                            | "mov"
                            | "m4v"
                            | "webm"
                            | "mpg"
                            | "mpeg"
                            | "ts"
                            | "m2ts"
                            | "wmv"
                    )
                })
            {
                continue;
            }
            count += 1;
            ensure!(count <= 100_000, "Folder scan file limit reached");
            let relative = path.strip_prefix(&root_path)?;
            let root_name = root_path
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new(""));
            let context_path = Path::new(root_name).join(relative);
            let mut hints = infer(&context_path);
            sidecar_evidence(&root_path, &path, &mut hints);
            let metadata = entry.metadata().context("Cannot inspect a local video")?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
            db.record_local_file(
                root.id,
                relative
                    .to_str()
                    .context("Local video path is not valid Unicode")?,
                i64::try_from(metadata.len())?,
                modified,
                &serde_json::to_string(&hints)?,
                generation,
            )?;
        }
    }
    ensure!(!cancel.load(Ordering::Relaxed), "Scan cancelled");
    db.finish_local_scan(root.id, generation)?;
    Ok(count)
}

fn sidecar_evidence(root: &Path, path: &Path, hints: &mut Hints) {
    let parent = path.parent().unwrap_or(root);
    let directory = if parent
        .file_name()
        .and_then(|v| v.to_str())
        .and_then(season_folder)
        .is_some()
    {
        parent
            .parent()
            .filter(|p| p.starts_with(root))
            .unwrap_or(parent)
    } else {
        parent
    };
    let named = directory.join(if hints.season.is_some() {
        "tvshow.nfo"
    } else {
        "movie.nfo"
    });
    for sidecar in [path.with_extension("nfo"), named] {
        if let Some(id) = sidecar_id(&sidecar) {
            hints.conflict |= hints
                .explicit_id
                .as_ref()
                .is_some_and(|existing| existing != &id);
            hints.explicit_id = Some(id);
        }
    }
}

fn sidecar_id(path: &Path) -> Option<String> {
    use std::io::Read;
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > 65_536 || link_metadata(&metadata) {
        return None;
    }
    // Bound the read too: the file can grow after its metadata was inspected.
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(65_537)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() > 65_536 {
        return None;
    }
    imdb_id(&text)
}

fn is_link(entry: &std::fs::DirEntry) -> Result<bool> {
    let metadata =
        std::fs::symlink_metadata(entry.path()).context("Cannot inspect a local folder entry")?;
    Ok(link_metadata(&metadata))
}

fn link_metadata(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
