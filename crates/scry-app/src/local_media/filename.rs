use super::Episode;
use std::path::{Path, PathBuf};

pub(crate) fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) struct Parsed {
    pub(super) title: String,
    pub(super) year: Option<u32>,
    pub(super) episode: Option<Episode>,
    pub(super) absolute: bool,
}

fn number(value: &str) -> Option<(u32, &str)> {
    let count = value.bytes().take_while(u8::is_ascii_digit).count();
    if count == 0 {
        return None;
    }
    let (digits, rest) = value.split_at(count);
    Some((digits.parse().ok()?, rest))
}

fn episode(value: &str) -> Option<Episode> {
    let (season, rest) = number(value.strip_prefix('s').unwrap_or(value))?;
    let rest = rest.strip_prefix('e').or_else(|| rest.strip_prefix('x'))?;
    let (first, mut rest) = number(rest)?;
    let mut numbers = vec![first];
    while let Some(next) = rest.strip_prefix('e') {
        let (n, remaining) = number(next)?;
        numbers.push(n);
        rest = remaining;
    }
    if let Some(next) = rest.strip_prefix("-e").or_else(|| rest.strip_prefix('-'))
        && let Some((last, remaining)) = number(next)
    {
        let last = if let Some(next) = remaining
            .strip_prefix('x')
            .or_else(|| remaining.strip_prefix('e'))
        {
            if last != season {
                return None;
            }
            number(next)?.0
        } else {
            last
        };
        if last >= first && last - first <= 30 {
            numbers = (first..=last).collect();
        }
    }
    Some(Episode { season, numbers })
}

pub(super) fn parse_in_roots(path: &Path, roots: &[PathBuf]) -> Parsed {
    let Some(root) = roots
        .iter()
        .filter(|root| path.starts_with(root))
        .min_by_key(|root| root.components().count())
    else {
        return parse(path.file_name().map_or(path, Path::new));
    };
    let relative = path.strip_prefix(root).unwrap_or(path);
    // Include the selected folder itself, but never infer titles from its ancestors.
    let context = Path::new(root.file_name().unwrap_or_default()).join(relative);
    parse(&context)
}

pub(super) fn parse(path: &Path) -> Parsed {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let mut parsed = parse_name(&strip_groups(&stem));
    if parsed.title.is_empty() {
        // Brackets can be the actual title, as in [REC], not a release group.
        parsed = parse_name(&stem.replace(['[', ']', '{', '}'], " "));
    }
    let season = path.parent().and_then(Path::file_name).and_then(|name| {
        let name = name
            .to_string_lossy()
            .to_ascii_lowercase()
            .replace(['.', '_'], " ");
        name.strip_prefix("season")?.trim().parse::<u32>().ok()
    });
    if parsed.episode.is_none()
        && let Some(season) = season
    {
        let name = stem.trim();
        if let Some((number, rest)) = number(name)
            && (rest.is_empty() || rest.starts_with([' ', '-', '.', '_']))
        {
            parsed.episode = Some(Episode {
                season,
                numbers: vec![number],
            });
            parsed.title.clear();
        }
    }
    if parsed.episode.is_none()
        && let Some((title, numbers)) = absolute(path)
        && numbers.first().is_some_and(|n| !(1888..=2100).contains(n))
    {
        parsed.absolute = true;
        parsed.title = title;
    }
    if parsed.title.is_empty()
        || parsed.year.is_none()
            && matches!(
                normalized(&parsed.title).as_str(),
                "video" | "movie" | "main" | "feature"
            )
    {
        let parent = path
            .parent()
            .and_then(|parent| {
                if parent
                    .file_name()?
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .starts_with("season")
                {
                    parent.parent()
                } else {
                    Some(parent)
                }
            })
            .and_then(Path::file_name)
            .unwrap_or_default()
            .to_string_lossy();
        let folder = parse_name(&parent);
        parsed.title = folder.title;
        parsed.year = parsed.year.or(folder.year);
    }
    if parsed.title.is_empty() {
        parsed.title = stem.into_owned();
    }
    parsed
}

fn technical(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "480p"
            | "720p"
            | "1080p"
            | "1080i"
            | "2160p"
            | "4k"
            | "uhd"
            | "ultrahd"
            | "bluray"
            | "blu-ray"
            | "bdrip"
            | "brrip"
            | "webrip"
            | "web-dl"
            | "web-rip"
            | "hdtv"
            | "dvdrip"
            | "x264"
            | "x265"
            | "h264"
            | "h265"
            | "hevc"
            | "avc"
            | "aac"
            | "ac3"
            | "eac3"
            | "dts"
            | "truehd"
            | "atmos"
            | "10bit"
            | "xvid"
            | "divx"
            | "proper"
            | "rerip"
            | "remux"
    )
}

fn strip_groups(value: &str) -> String {
    let mut out = String::new();
    let mut group = None;
    let mut buffer = String::new();
    for c in value.chars() {
        if let Some(end) = group {
            if c == end {
                if end == ')' && !buffer.split([' ', '-', '.', '_']).any(technical) {
                    out.push('(');
                    out.push_str(&buffer);
                    out.push(')');
                }
                out.push(' ');
                buffer.clear();
                group = None;
            } else {
                buffer.push(c);
            }
        } else if let Some(end) = match c {
            '[' => Some(']'),
            '{' => Some('}'),
            '(' => Some(')'),
            _ => None,
        } {
            out.push(' ');
            group = Some(end);
        } else {
            out.push(c);
        }
    }
    if group.is_some() {
        out.push_str(&buffer);
    }
    out
}

fn prefix_length(tokens: &[&str]) -> usize {
    let Some(first) = tokens.first() else {
        return 0;
    };
    let lower = first.to_ascii_lowercase();
    let codec = ["divx", "xvid", "dvdrip"]
        .iter()
        .find_map(|codec| lower.strip_prefix(codec));
    let Some(suffix) = codec else {
        return 0;
    };
    let language = |v: &str| matches!(v, "ita" | "eng" | "en" | "it" | "multi");
    if !suffix.trim_matches('-').is_empty() && !language(suffix.trim_matches('-')) {
        return 0;
    }
    1 + tokens
        .iter()
        .skip(1)
        .take_while(|t| **t == "-" || language(&t.to_ascii_lowercase()))
        .count()
}

fn episode_at(tokens: &[&str], index: usize) -> Option<Episode> {
    let token = tokens.get(index)?.to_ascii_lowercase();
    if token == "season" {
        let season = tokens.get(index + 1)?.parse().ok()?;
        let label = tokens.get(index + 2)?.to_ascii_lowercase();
        let next = label.strip_prefix("episode")?;
        let next = if next.is_empty() {
            tokens.get(index + 3)?
        } else {
            next
        };
        return Some(Episode {
            season,
            numbers: vec![number(next)?.0],
        });
    }
    if let Some(episode) = episode(&token) {
        return Some(episode);
    }
    let (season, suffix) = number(token.strip_prefix('s').unwrap_or(&token))?;
    let next = tokens.get(index + 1)?.to_ascii_lowercase();
    let rest = if token.starts_with('s') && suffix.is_empty() {
        next.strip_prefix('e')?
    } else if matches!(suffix, "st" | "nd" | "rd" | "th") && next == "season" {
        tokens.get(index + 2)?
    } else {
        return None;
    };
    let (episode, _) = number(rest)?;
    Some(Episode {
        season,
        numbers: vec![episode],
    })
}

fn parse_name(name: &str) -> Parsed {
    use chrono::Datelike;
    let cleaned = name.replace(['.', '_'], " ");
    let tokens: Vec<_> = cleaned.split_whitespace().collect();
    let tokens = tokens.get(prefix_length(&tokens)..).unwrap_or_default();
    let ep = tokens
        .iter()
        .enumerate()
        .find_map(|(i, _)| episode_at(tokens, i).map(|e| (i, e)));
    let release = tokens.iter().enumerate().skip(1).find_map(|(i, token)| {
        let token = token.trim_matches(['(', ')']).to_ascii_lowercase();
        let split_codec = matches!(token.as_str(), "h" | "x")
            && tokens
                .get(i + 1)
                .is_some_and(|t| matches!(*t, "264" | "265"));
        let edition = matches!(
            token.as_str(),
            "extended" | "remastered" | "imax" | "directors" | "director's"
        ) && tokens.iter().skip(i + 1).any(|t| technical(t));
        (technical(&token) || split_codec || edition).then_some(i)
    });
    let boundary = ep
        .as_ref()
        .map(|(i, _)| *i)
        .into_iter()
        .chain(release)
        .min()
        .unwrap_or(tokens.len());
    let max_year = u32::try_from(chrono::Utc::now().year() + 1).unwrap_or(2100);
    let years: Vec<_> = tokens
        .iter()
        .enumerate()
        .take(boundary)
        .filter(|(i, token)| *i > 0 || token.starts_with('('))
        .filter_map(|(i, token)| {
            let value = token.trim_matches(['(', ')']);
            value
                .parse::<u32>()
                .ok()
                .filter(|y| value.len() == 4 && (1888..=max_year).contains(y))
                .map(|y| (i, y))
        })
        .collect();
    let year = years
        .iter()
        .rev()
        .find(|(i, _)| tokens.get(*i).is_some_and(|t| t.starts_with('(')))
        .or_else(|| years.last())
        .copied();
    let end = [ep.as_ref().map(|(i, _)| *i), year.map(|(i, _)| i), release]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(tokens.len());
    let title = tokens
        .get(..end)
        .unwrap_or_default()
        .join(" ")
        .trim_matches([' ', '-'])
        .trim_matches(['(', ')'])
        .to_owned();
    Parsed {
        title,
        year: year.map(|(_, y)| y),
        episode: ep.map(|(_, e)| e),
        absolute: false,
    }
}

pub(super) fn absolute(path: &Path) -> Option<(String, Vec<u32>)> {
    let stem = strip_groups(&path.file_stem()?.to_string_lossy()).replace(['.', '_'], " ");
    let (title, suffix) = stem.rsplit_once(" - ").unwrap_or(("", stem.trim()));
    let token = suffix.split_whitespace().next()?;
    let token = token.split_once('v').map_or(token, |(number, version)| {
        if version.parse::<u32>().is_ok() {
            number
        } else {
            token
        }
    });
    let (first, rest) = number(token)?;
    if first == 0 {
        return None;
    }
    let numbers = if let Some(last) = rest.strip_prefix('-') {
        let (last, rest) = number(last)?;
        if !rest.is_empty() || last < first || last - first > 31 {
            return None;
        }
        (first..=last).collect()
    } else if rest.is_empty() {
        vec![first]
    } else {
        return None;
    };
    if title.is_empty()
        && suffix
            .split_whitespace()
            .skip(1)
            .any(|token| !technical(token))
    {
        return None;
    }
    Some((title.trim().to_owned(), numbers))
}
