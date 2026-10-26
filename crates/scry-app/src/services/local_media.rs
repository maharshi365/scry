//! Background local indexing and conservative provider-backed matching.
use super::{Event, Services};
use crate::local_media::{Hints, normalize, select_match};
use anyhow::{Context, Result, ensure};
use scry_addons::{Addon, Client, Meta};
use scry_library::{
    Library,
    local_media::{LocalFile, LocalGroup, LocalLink, LocalRoot},
};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Debug, Default)]
pub(crate) struct Snapshot {
    pub roots: Vec<LocalRoot>,
    pub files: Vec<LocalFile>,
    pub message: String,
}

pub(crate) enum Request {
    Refresh,
    Add(PathBuf),
    Remove(i64),
    Scan,
    Clear(i64),
    Search(i64, String, String),
    Inspect(i64, Candidate),
    Confirm(i64, Addon, Meta, Option<String>, bool),
}

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub addon: Addon,
    pub meta: Meta,
}

impl Services {
    pub(crate) fn cancel_local_scan(&self) {
        self.local_cancel.store(true, Ordering::Relaxed);
    }

    pub(crate) fn local_request(
        &self,
        request: Request,
        offset: i64,
        unmatched: bool,
    ) -> Result<()> {
        ensure!(
            !self.local_busy.swap(true, Ordering::AcqRel),
            "Local media is busy"
        );
        self.local_cancel.store(false, Ordering::Relaxed);
        let (data, client, config, inbox, busy, cancel, runtime) = (
            self.data.clone(),
            self.client.clone(),
            self.config.clone(),
            self.inbox.clone(),
            self.local_busy.clone(),
            self.local_cancel.clone(),
            self.runtime.clone(),
        );
        self.runtime.spawn(async move {
            let addons = config.lock().await.clone();
            let output = inbox.clone();
            let result = tokio::task::spawn_blocking(move || -> Result<Snapshot> {
                std::fs::create_dir_all(&data).context("Cannot open local media index")?;
                let db = Library::open(data.join(scry_library::DATABASE_NAME))?;
                let context = Matcher {
                    client,
                    addons,
                    runtime,
                    cancel,
                    inbox: output,
                };
                let message = context.apply(&db, request)?;
                Ok(Snapshot {
                    roots: db.local_roots()?,
                    files: db.local_files(offset, unmatched)?,
                    message,
                })
            })
            .await;
            busy.store(false, Ordering::Release);
            let result = match result {
                Ok(Ok(snapshot)) => Ok(snapshot),
                // Only our contextual messages reach the UI, never OS errors or provider URLs.
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err("Local media task could not finish".into()),
            };
            inbox.send(Event::LocalMedia(result)).await;
        });
        Ok(())
    }

    pub(crate) fn local_sources(
        &self,
        generation: u64,
        namespace: String,
        kind: String,
        id: String,
    ) {
        let (data, inbox) = (self.data.clone(), self.inbox.clone());
        self.runtime.spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                Library::open(data.join(scry_library::DATABASE_NAME))?
                    .local_sources(&namespace, &kind, &id)
            })
            .await;
            inbox
                .send(Event::LocalSources(
                    generation,
                    match result {
                        Ok(Ok(files)) => Ok(files),
                        _ => Err("Cannot read local media sources".into()),
                    },
                ))
                .await;
        });
    }
}

struct Matcher {
    client: Client,
    addons: Vec<Addon>,
    runtime: Arc<tokio::runtime::Runtime>,
    cancel: Arc<AtomicBool>,
    inbox: super::Inbox,
}

impl Matcher {
    fn apply(&self, db: &Library, request: Request) -> Result<String> {
        match request {
            Request::Refresh => Ok(String::new()),
            Request::Add(path) => {
                let path = std::fs::canonicalize(path).context("Cannot open local folder")?;
                ensure!(path.is_dir(), "Choose an existing folder");
                let identity = path.to_string_lossy().to_lowercase();
                for root in db.local_roots()? {
                    let existing = root.path.to_lowercase();
                    if existing == identity {
                        return Ok("Folder is already indexed".into());
                    }
                    ensure!(
                        !Path::new(&existing).starts_with(&identity)
                            && !Path::new(&identity).starts_with(&existing),
                        "This folder overlaps an existing root; choose separate roots"
                    );
                }
                db.add_local_root(path.to_str().context("Folder path is not valid Unicode")?)?;
                self.scan(db)
            }
            Request::Remove(id) => {
                db.remove_local_root(id)?;
                Ok("Folder removed from Scry. Media files were not changed.".into())
            }
            Request::Scan => self.scan(db),
            Request::Clear(id) => {
                db.clear_local_link(id)?;
                Ok("Match cleared. You can search for a replacement.".into())
            }
            Request::Search(id, query, kind) => {
                let query = crate::search::query(&query).map_err(anyhow::Error::msg)?;
                ensure!(!query.is_empty(), "Enter a title to search");
                let hints = Hints {
                    title: query,
                    ..Hints::default()
                };
                let candidates = self.runtime.block_on(self.search(&hints, &kind))?;
                self.runtime
                    .block_on(self.inbox.send(Event::LocalCandidates(id, candidates)));
                Ok("Choose the correct title below".into())
            }
            Request::Confirm(id, addon, meta, episode, group) => {
                self.confirm(db, id, &addon, &meta, episode.as_deref(), group)
            }
            Request::Inspect(id, candidate) => {
                let meta = self
                    .runtime
                    .block_on(self.network(self.client.meta(
                        &candidate.addon,
                        &candidate.meta.kind,
                        &candidate.meta.id,
                    )))
                    .context("Cannot load the selected title's metadata")?;
                self.runtime.block_on(self.inbox.send(Event::LocalChoice(
                    id,
                    Candidate {
                        addon: candidate.addon,
                        meta,
                    },
                )));
                Ok("Confirm the movie or choose an episode".into())
            }
        }
    }

    fn confirm(
        &self,
        db: &Library,
        id: i64,
        addon: &Addon,
        meta: &Meta,
        episode: Option<&str>,
        group: bool,
    ) -> Result<String> {
        self.check_cancel()?;
        let file = db.local_file(id)?;
        let hints: Hints =
            serde_json::from_str(&file.hints).context("Cannot read indexed file evidence")?;
        let namespace = namespace(addon, &meta.id);
        let link = make_link(&namespace, meta, &hints, episode, true)
            .context("Choose an episode for this file")?;
        db.cache_local_metadata(&namespace, &meta.kind, &meta.id, &metadata_json(meta, None))?;
        db.link_local_file(id, &link)?;
        let mut linked = 1;
        if group && meta.kind == "series" {
            db.set_local_group(
                file.root_id,
                &hints.group,
                &normalize(&hints.title),
                &LocalGroup {
                    namespace: namespace.clone(),
                    metadata: metadata_json(meta, None),
                },
            )?;
            let mut offset = 0;
            loop {
                self.check_cancel()?;
                let files = db.local_files(offset, false)?;
                let done = files.len() < 100;
                for sibling in files {
                    if sibling.id == id || sibling.root_id != file.root_id || sibling.manual {
                        continue;
                    }
                    let other: Hints = serde_json::from_str(&sibling.hints)
                        .context("Cannot read indexed file evidence")?;
                    if other.group == hints.group
                        && normalize(&other.title) == normalize(&hints.title)
                        && !other.conflict
                        && other.explicit_id.as_ref().is_none_or(|id| id == &meta.id)
                        && let Some(link) = make_link(&namespace, meta, &other, None, true)
                    {
                        db.link_local_file(sibling.id, &link)?;
                        linked += 1;
                    }
                }
                if done {
                    break;
                }
                offset += 100;
            }
        }
        Ok(format!("Confirmed {linked} file match(es)"))
    }

    fn check_cancel(&self) -> Result<()> {
        ensure!(
            !self.cancel.load(Ordering::Relaxed),
            "Scan cancelled; existing matches were preserved"
        );
        Ok(())
    }

    async fn network<T>(&self, future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
        tokio::pin!(future);
        loop {
            self.check_cancel()?;
            tokio::select! {
                result = &mut future => return result,
                () = tokio::time::sleep(std::time::Duration::from_millis(100)) => {},
            }
        }
    }

    fn scan_roots(&self, db: &Library) -> Result<(usize, usize)> {
        let mut count = 0;
        let mut failed = 0;
        for root in db.local_roots()? {
            self.check_cancel()?;
            if let Ok(found) = crate::local_media::scan(db, &root, &self.cancel) {
                count += found;
            } else {
                self.check_cancel()?;
                db.set_local_root_online(root.id, false)?;
                failed += 1;
            }
            self.inbox.notify(Event::LocalProgress(format!(
                "Indexed {count} files · matching titles…"
            )));
        }
        Ok((count, failed))
    }

    fn scan(&self, db: &Library) -> Result<String> {
        let (count, failed) = self.scan_roots(db)?;
        let mut cache: BTreeMap<String, Option<Candidate>> = BTreeMap::new();
        let mut offset = 0;
        let mut linked = 0;
        loop {
            self.check_cancel()?;
            let files = db.local_files(offset, false)?;
            let done = files.len() < 100;
            for file in files {
                self.check_cancel()?;
                if !file.available || file.external_id.is_some() {
                    continue;
                }
                let hints: Hints = serde_json::from_str(&file.hints)
                    .context("Cannot read indexed file evidence")?;
                if hints.conflict || hints.title.is_empty() && hints.explicit_id.is_none() {
                    continue;
                }
                if let Some(group) =
                    db.local_group(file.root_id, &hints.group, &normalize(&hints.title))?
                {
                    let meta: Meta = serde_json::from_str(&group.metadata)
                        .context("Cannot read the saved show match")?;
                    if hints.explicit_id.as_ref().is_none_or(|id| id == &meta.id)
                        && let Some(link) = make_link(&group.namespace, &meta, &hints, None, false)
                    {
                        db.link_local_file(file.id, &link)?;
                        linked += 1;
                        continue;
                    }
                }
                let kind = if hints.season.is_some() {
                    "series"
                } else {
                    "movie"
                };
                let key = format!(
                    "{kind}:{}:{:?}:{:?}",
                    normalize(&hints.title),
                    hints.year,
                    hints.explicit_id
                );
                if !cache.contains_key(&key) {
                    if cache.len() >= 64 {
                        cache.clear();
                    }
                    let candidate = self.resolve_candidate(db, &hints, kind)?;
                    cache.insert(key.clone(), candidate);
                }
                if let Some(Some(candidate)) = cache.get(&key)
                    && let Some(link) = make_link(
                        &namespace(&candidate.addon, &candidate.meta.id),
                        &candidate.meta,
                        &hints,
                        None,
                        false,
                    )
                {
                    db.link_local_file(file.id, &link)?;
                    linked += 1;
                }
                if linked % 25 == 0 {
                    self.inbox.notify(Event::LocalProgress(format!(
                        "Indexed {count} files · auto-matched {linked}"
                    )));
                }
            }
            if done {
                break;
            }
            offset += 100;
        }
        Ok(format!(
            "Indexed {count} files · auto-matched {linked} · {failed} incomplete/offline roots. Existing matches preserved."
        ))
    }

    fn resolve_candidate(
        &self,
        db: &Library,
        hints: &Hints,
        kind: &str,
    ) -> Result<Option<Candidate>> {
        let candidate = self
            .runtime
            .block_on(self.search(hints, kind))
            .ok()
            .and_then(|candidates| {
                let metas: Vec<_> = candidates.iter().map(|c| c.meta.clone()).collect();
                candidates.get(select_match(hints, &metas)?).cloned()
            });
        self.check_cancel()?;
        let candidate = candidate.and_then(|candidate| {
            self.runtime
                .block_on(self.network(self.client.meta(
                    &candidate.addon,
                    kind,
                    &candidate.meta.id,
                )))
                .ok()
                .map(|meta| Candidate {
                    addon: candidate.addon,
                    meta,
                })
        });
        self.check_cancel()?;
        if let Some(candidate) = &candidate {
            db.cache_local_metadata(
                &namespace(&candidate.addon, &candidate.meta.id),
                &candidate.meta.kind,
                &candidate.meta.id,
                &metadata_json(&candidate.meta, None),
            )?;
        }
        Ok(candidate)
    }

    async fn search(&self, hints: &Hints, kind: &str) -> Result<Vec<Candidate>> {
        self.check_cancel()?;
        if let Some(id) = &hints.explicit_id {
            for addon in self
                .addons
                .iter()
                .filter(|addon| addon.supports("meta", kind, id))
                .take(8)
            {
                self.check_cancel()?;
                if let Ok(meta) = self.network(self.client.meta(addon, kind, id)).await {
                    return Ok(vec![Candidate {
                        addon: addon.clone(),
                        meta,
                    }]);
                }
            }
            anyhow::bail!("No enabled metadata provider could resolve the explicit ID");
        }
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();
        let requests = crate::search::plan(self.addons.clone()).map_err(anyhow::Error::msg)?;
        for request in requests
            .into_iter()
            .filter(|r| {
                r.catalog.kind == kind || !matches!(r.catalog.kind.as_str(), "movie" | "series")
            })
            .take(8)
        {
            self.check_cancel()?;
            let found = self
                .network(
                    self.client
                        .catalog(&request.addon, &request.catalog, Some(&hints.title)),
                )
                .await
                .context("A metadata provider could not complete the title search")?;
            for meta in found
                .into_iter()
                .filter(|m| m.kind == kind && !m.id.trim().is_empty())
            {
                let namespace = namespace(&request.addon, &meta.id);
                if seen.insert((namespace, meta.id.clone())) {
                    candidates.push(Candidate {
                        addon: request.addon.clone(),
                        meta,
                    });
                }
            }
        }
        ensure!(
            !candidates.is_empty(),
            "No matching titles found. Enable a searchable metadata add-on or try another title."
        );
        Ok(candidates)
    }
}

fn namespace(addon: &Addon, id: &str) -> String {
    if id
        .split(':')
        .next()
        .and_then(|id| id.strip_prefix("tt"))
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit()))
    {
        "imdb".into()
    } else {
        addon.manifest.id.clone()
    }
}

fn make_link(
    namespace: &str,
    meta: &Meta,
    hints: &Hints,
    episode: Option<&str>,
    manual: bool,
) -> Option<LocalLink> {
    let video = if meta.kind == "series" {
        let mut videos = meta.videos.iter().filter(|v| {
            if let Some(id) = episode {
                v.id == id
            } else {
                !hints.conflict
                    && hints.season.is_some()
                    && hints.episode.is_some()
                    && v.season == hints.season
                    && v.episode == hints.episode
            }
        });
        let video = videos.next()?;
        if videos.next().is_some() || video.id.is_empty() {
            return None;
        }
        Some(video)
    } else if meta.kind == "movie" {
        None
    } else {
        return None;
    };
    let metadata = metadata_json(meta, video.map(|v| v.id.as_str()));
    Some(LocalLink {
        namespace: namespace.to_owned(),
        kind: if video.is_some() { "episode" } else { "movie" }.into(),
        external_id: video.map_or_else(|| meta.id.clone(), |v| v.id.clone()),
        title: video.map_or_else(
            || meta.name.clone(),
            |v| {
                format!(
                    "{} · S{} E{} · {}",
                    meta.name,
                    v.season.unwrap_or(0),
                    v.episode.unwrap_or(0),
                    v.title
                )
            },
        ),
        metadata,
        manual,
    })
}

fn metadata_json(meta: &Meta, episode: Option<&str>) -> String {
    let videos: Vec<_> = meta.videos.iter().filter(|v| episode.is_none_or(|id| v.id == id))
        .map(|v| serde_json::json!({"id":v.id,"title":v.title,"season":v.season,"episode":v.episode,"released":v.released})).collect();
    serde_json::json!({"id":meta.id,"type":meta.kind,"name":meta.name,"year":meta.year,
        "poster":meta.poster,"background":meta.background,"logo":meta.logo,"description":meta.description,
        "released":meta.released,"videos":videos}).to_string()
}

#[cfg(test)]
#[path = "local_media_tests.rs"]
#[expect(
    clippy::unwrap_used,
    reason = "Invalid metadata fixtures and missing match results must fail tests"
)]
mod tests;
