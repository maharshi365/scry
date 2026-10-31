//! A serial worker owns folder scans, metadata matching, and index writes.
mod orders;
mod watch;
use super::{Event, Inbox, Services};
use crate::local_media::{self, Index, Match};
use anyhow::{Context, Result, ensure};
use scry_addons::{Client, Meta};
use std::path::PathBuf;
use tokio::{runtime::Runtime, sync::mpsc};

pub(crate) enum Command {
    Add(PathBuf),
    Remove(PathBuf),
    Scan,
    StopMatching,
    Match(String, Option<Match>),
    Episode(String, PathBuf, local_media::Episode),
    Search(u64, String),
    FindOrders(u64, String, local_media::EpisodeOrder),
    ApplyOrder(String, Option<local_media::Ordering>),
}

impl Services {
    pub fn media_command(&self, command: Command) -> Result<()> {
        self.my_media
            .as_ref()
            .context("My Media is unavailable")?
            .try_send(command)
            .map_err(|_| anyhow::anyhow!("My Media is busy; try again"))
    }
}

pub(super) fn start(runtime: &Runtime, data: PathBuf, inbox: Inbox) -> mpsc::Sender<Command> {
    let (tx, mut rx) = mpsc::channel(8);
    runtime.spawn(async move {
        let folder = data.clone();
        let loaded = tokio::task::spawn_blocking(move || local_media::load(&folder)).await;
        let Ok(Ok(mut index)) = loaded else {
                inbox.send(Event::MyMedia(Err("Cannot load My Media. The saved index has not been changed.".into()), String::new(), true)).await;
                return;
        };
        inbox.send(Event::MyMedia(Ok(index.clone()), String::new(), true)).await;
        let client = Client::new();
        let orders = orders::Lookup::new();
        let (mut folders, _) = watch::Folders::new().configure(index.roots.clone()).await;
        let mut retry = tokio::time::interval(std::time::Duration::from_secs(30));
        retry.tick().await;
        let mut pending = None;
        loop {
            let command = if let Some(command) = pending.take() { command } else {
                tokio::select! {
                    command = rx.recv() => { let Some(command) = command else { break; }; command }
                    () = folders.changed() => Command::Scan,
                    _ = retry.tick() => { if !folders.needs_retry() { continue; } Command::Scan }
                }
            };
            if let Command::Search(token, query) = command {
                let result = match &client {
                    Ok(client) => search(client, &query).await.map_err(|_| "Cannot search metadata. Check your connection and try again.".into()),
                    Err(_) => Err("Metadata search is unavailable".into()),
                };
                inbox.send(Event::MediaCandidates(token, result)).await;
                continue;
            }
            if let Command::FindOrders(token, id, mode) = command {
                let result = match (&client, &orders, index.items.iter().find(|i| i.id == id)) {
                    (Ok(client), Ok(orders), Some(item)) => orders.find(client, item, mode).await.map_err(|error| error.to_string()),
                    _ => Err("Episode-order lookup is unavailable".into()),
                };
                inbox.send(Event::MediaOrders(token, id, result)).await;
                continue;
            }
            let scan = matches!(command, Command::Add(_) | Command::Remove(_) | Command::Scan);
            let folder = data.clone();
            let next = index.clone();
            let result = tokio::task::spawn_blocking(move || update(next, command, &folder)).await;
            match result {
                Ok(Ok((mut next, mut status))) => {
                    let (updated_folders, watching) = folders.configure(next.roots.clone()).await;
                    folders = updated_folders;
                    if !watching { status.push_str(" Some folders cannot be watched yet; retrying automatically."); }
                    // Publish discovered files before waiting for metadata services.
                    inbox.send(Event::MyMedia(Ok(next.clone()), "Matching titles…".into(), false)).await;
                    if scan && let Ok(client) = &client {
                        let mut failed = false;
                        for item in &mut next.items {
                            if rx.is_closed() { break; }
                            if item.matched.is_some() || item.keep_unmatched || !item.files.iter().any(|f| f.available) { continue; }
                            let result = tokio::select! {
                                result = search(client, &item.title) => result,
                                command = rx.recv() => { pending = command; break; }
                                () = folders.changed() => { pending = Some(Command::Scan); break; }
                            };
                            let Ok(candidates) = result else { failed = true; break; };
                            if let Some(candidate) = best_match(&item.title, item.year, &item.kind, &candidates) {
                                item.matched = Some(Match::from_meta(candidate, false));
                            }
                        }
                        if failed { status.push_str(" Metadata lookup is unavailable; unmatched files are still playable."); }
                    }
                    let folder = data.clone();
                    let saved = next.clone();
                    let persisted = tokio::task::spawn_blocking(move || local_media::save(&folder, &saved)).await;
                    if !matches!(persisted, Ok(Ok(()))) { status.push_str(" Cannot save metadata matches. Rescan to retry."); }
                    index = next;
                    inbox.send(Event::MyMedia(Ok(index.clone()), status, true)).await;
                }
                _ => inbox.send(Event::MyMedia(Err("Cannot update My Media. Check the folder and try again.".into()), String::new(), true)).await,
            }
        }
    });
    // Refresh availability at startup without discarding saved manual matches.
    let _ = tx.try_send(Command::Scan);
    tx
}

fn update(mut index: Index, command: Command, data: &std::path::Path) -> Result<(Index, String)> {
    let stopped = matches!(command, Command::StopMatching);
    let scan = matches!(
        command,
        Command::Add(_) | Command::Remove(_) | Command::Scan
    );
    match command {
        Command::Add(path) => index.add_root(&path)?,
        Command::Remove(path) => {
            index.roots.retain(|root| root != &path);
            for item in &mut index.items {
                item.files
                    .retain(|file| index.roots.iter().any(|root| file.path.starts_with(root)));
            }
            index.items.retain(|item| !item.files.is_empty());
        }
        Command::Match(id, matched) => {
            let item = index
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .context("Media no longer exists")?;
            item.keep_unmatched = matched.is_none();
            if item.matched.as_ref().map(|m| (&m.id, &m.kind))
                != matched.as_ref().map(|m| (&m.id, &m.kind))
            {
                item.ordering = None;
            }
            item.matched = matched;
        }
        Command::Episode(id, path, episode) => {
            ensure!(
                !episode.numbers.is_empty() && episode.numbers.len() <= 32,
                "Invalid episode mapping"
            );
            let item = index
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .context("Media no longer exists")?;
            let file = item
                .files
                .iter_mut()
                .find(|f| f.path == path)
                .context("File no longer exists")?;
            file.episode = Some(episode);
            file.corrected = true;
            item.kind = "series".into();
            if item.matched.as_ref().is_some_and(|m| m.kind != "series") {
                item.matched = None;
                item.keep_unmatched = true;
            }
        }
        Command::ApplyOrder(id, ordering) => {
            let item = index
                .items
                .iter_mut()
                .find(|item| item.id == id)
                .context("Media no longer exists")?;
            if let Some(ordering) = &ordering {
                ordering.validate()?;
                ensure!(
                    item.matched.as_ref().is_some_and(
                        |matched| matched.id == ordering.series_id && matched.kind == "series"
                    ),
                    "Episode order belongs to a different series"
                );
            }
            item.ordering = ordering;
        }
        Command::Scan
        | Command::StopMatching
        | Command::Search(_, _)
        | Command::FindOrders(_, _, _) => {}
    }
    let failures = if scan { index.scan() } else { 0 };
    local_media::save(data, &index)?;
    let status = if stopped {
        format!(
            "Matching stopped. {} titles indexed. Rescan to match remaining titles.",
            index.items.len()
        )
    } else if failures == 0 {
        format!("{} titles indexed.", index.items.len())
    } else {
        format!(
            "{} titles indexed. Some folders could not be read; their files are marked offline.",
            index.items.len()
        )
    };
    Ok((index, status))
}

async fn search(client: &Client, query: &str) -> Result<Vec<Meta>> {
    ensure!(
        !query.trim().is_empty() && query.chars().count() <= 256,
        "Enter a title of at most 256 characters"
    );
    let mut candidates = Vec::new();
    let mut completed = false;
    if let Ok(addon) = client.install(super::CINEMETA).await {
        for kind in ["movie", "series"] {
            if let Some(catalog) = addon
                .manifest
                .catalogs
                .iter()
                .find(|c| c.kind == kind && c.extra.iter().any(|e| e.name == "search"))
                && let Ok(found) = client.catalog(&addon, catalog, Some(query)).await
            {
                candidates.extend(found.into_iter().take(20));
                completed = true;
            }
        }
    }
    if let Ok(anime) = client.install(super::KITSU).await
        && let Some(catalog) = anime
            .manifest
            .catalogs
            .iter()
            .find(|c| c.kind == "anime" && c.extra.iter().any(|e| e.name == "search"))
        && let Ok(found) = client.catalog(&anime, catalog, Some(query)).await
    {
        candidates.extend(found.into_iter().take(20));
        completed = true;
    }
    ensure!(completed, "Metadata search is unavailable");
    let mut seen = std::collections::BTreeSet::new();
    candidates.retain(|meta| seen.insert((meta.kind.clone(), meta.id.clone())));
    Ok(candidates)
}

fn best_match<'a>(
    title: &str,
    year: Option<u32>,
    kind: &str,
    candidates: &'a [Meta],
) -> Option<&'a Meta> {
    let title = local_media::normalized(title);
    let mut found = candidates.iter().filter(|meta| {
        meta.kind == kind
            && local_media::normalized(&meta.name) == title
            && year.is_none_or(|year| meta.year.starts_with(&year.to_string()))
    });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Metadata fixtures must parse")]
mod tests {
    use super::*;
    #[test]
    fn matching_requires_unique_title_kind_and_year() {
        let candidates: Vec<Meta> = serde_json::from_value(serde_json::json!([
            {"id":"old", "type":"movie", "name":"Dune", "year":"1984"},
            {"id":"new", "type":"movie", "name":"Dune", "year":"2021"},
            {"id":"tv", "type":"series", "name":"Dune", "year":"2000"}
        ]))
        .unwrap();
        assert!(best_match("Dune", None, "movie", &candidates).is_none());
        assert_eq!(
            best_match("Dune", Some(2021), "movie", &candidates)
                .unwrap()
                .id,
            "new"
        );
        assert!(best_match("Dune", Some(2020), "movie", &candidates).is_none());
        assert!(best_match("Something else", None, "series", &candidates).is_none());
    }

    #[test]
    fn review_changes_survive_scan_and_do_not_change_source_files() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("media");
        std::fs::create_dir(&media).unwrap();
        let path = media.join("Example.S01E01.mkv");
        std::fs::write(&path, b"unchanged video content").unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        let (index, _) = update(Index::default(), Command::Add(media), dir.path()).unwrap();
        let id = index.items[0].id.clone();
        let meta: Meta = serde_json::from_value(
            serde_json::json!({"id":"ttshow", "type":"series", "name":"Correct show"}),
        )
        .unwrap();
        let (index, _) = update(
            index,
            Command::Match(id.clone(), Some(Match::from_meta(&meta, true))),
            dir.path(),
        )
        .unwrap();
        let (index, _) = update(
            index,
            Command::Episode(
                id.clone(),
                path.clone(),
                local_media::Episode {
                    season: 2,
                    numbers: vec![5],
                },
            ),
            dir.path(),
        )
        .unwrap();
        let (index, _) = update(index, Command::Scan, dir.path()).unwrap();
        assert_eq!(index.items[0].sources("series", "ttshow:2:5").len(), 1);
        assert!(index.items[0].sources("series", "ttshow:1:1").is_empty());
        assert!(
            local_media::load(dir.path()).unwrap().items[0]
                .matched
                .as_ref()
                .unwrap()
                .manual
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"unchanged video content");
        let (index, _) = update(index, Command::Match(id, None), dir.path()).unwrap();
        let (index, _) = update(index, Command::Scan, dir.path()).unwrap();
        assert!(index.items[0].keep_unmatched);
        assert!(index.items[0].matched.is_none());
    }

    #[test]
    fn removing_a_root_does_not_remove_files_or_overlapping_roots() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("media");
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let path = nested.join("Movie.2020.mkv");
        std::fs::write(&path, []).unwrap();
        let (index, _) = update(Index::default(), Command::Add(root), dir.path()).unwrap();
        let (index, _) = update(index, Command::Add(nested), dir.path()).unwrap();
        let root = index.roots[0].clone();
        let (index, _) = update(index, Command::Remove(root), dir.path()).unwrap();
        assert_eq!(index.roots.len(), 1);
        assert_eq!(index.items[0].files.len(), 1);
        assert!(path.exists());
    }

    #[test]
    fn an_order_for_another_series_cannot_replace_the_saved_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("media");
        std::fs::create_dir(&media).unwrap();
        std::fs::write(media.join("Example.S01E01.mkv"), []).unwrap();
        let (mut index, _) = update(Index::default(), Command::Add(media), dir.path()).unwrap();
        let id = index.items[0].id.clone();
        let meta: Meta = serde_json::from_value(serde_json::json!({"id":"show", "type":"series", "name":"Example", "videos":[{"id":"show:1:1", "title":"One", "season":1, "episode":1}]})).unwrap();
        index.items[0].matched = Some(Match::from_meta(&meta, true));
        let order =
            local_media::Ordering::from_meta(&meta, local_media::EpisodeOrder::Absolute).unwrap();
        let (index, _) = update(
            index,
            Command::ApplyOrder(id.clone(), Some(order.clone())),
            dir.path(),
        )
        .unwrap();
        let mut wrong = order;
        wrong.series_id = "another-show".into();
        assert!(update(index, Command::ApplyOrder(id, Some(wrong)), dir.path()).is_err());
        assert_eq!(
            local_media::load(dir.path()).unwrap().items[0]
                .ordering
                .as_ref()
                .unwrap()
                .series_id,
            "show"
        );
    }
}
