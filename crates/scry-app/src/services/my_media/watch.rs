use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher, event::ModifyKind,
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::watch;

const QUIET_PERIOD: Duration = Duration::from_millis(500);
const MAX_DELAY: Duration = Duration::from_secs(3);

pub(super) struct Folders {
    watcher: Option<RecommendedWatcher>,
    roots: Vec<PathBuf>,
    changes: watch::Receiver<u64>,
    signal: watch::Sender<u64>,
    retry: bool,
    dirty_since: Option<tokio::time::Instant>,
    rearm: Arc<AtomicBool>,
}

impl Folders {
    pub fn new() -> Self {
        let (signal, changes) = watch::channel(0);
        Self {
            watcher: None,
            roots: Vec::new(),
            changes,
            signal,
            retry: true,
            dirty_since: None,
            rearm: Arc::default(),
        }
    }

    pub fn needs_retry(&self) -> bool {
        self.retry
    }

    pub async fn configure(mut self, roots: Vec<PathBuf>) -> (Self, bool) {
        tokio::task::spawn_blocking(move || {
            let watching = self.set_roots(&roots);
            (self, watching)
        })
        .await
        .unwrap_or_else(|_| (Self::new(), false))
    }

    pub fn set_roots(&mut self, roots: &[PathBuf]) -> bool {
        let rearm = self.rearm.swap(false, Ordering::AcqRel);
        if self.roots == roots && !self.retry && !rearm && roots.iter().all(|root| root.is_dir()) {
            return true;
        }
        let signal = self.signal.clone();
        let selected = roots.to_vec();
        let rearm = self.rearm.clone();
        let watcher = RecommendedWatcher::new(
            move |event: notify::Result<Event>| {
                if event
                    .as_ref()
                    .map_or(true, |event| relevant(event, &selected))
                {
                    if event.as_ref().map_or(true, |event| {
                        event
                            .paths
                            .iter()
                            .any(|path| selected.iter().any(|root| root.starts_with(path)))
                    }) {
                        rearm.store(true, Ordering::Release);
                    }
                    signal.send_modify(|generation| *generation = generation.wrapping_add(1));
                }
            },
            Config::default().with_follow_symlinks(false),
        );
        let Ok(mut watcher) = watcher else {
            self.watcher.take();
            self.retry = true;
            return false;
        };
        let mut paths = BTreeMap::new();
        for root in roots {
            if let Some(parent) = root.parent() {
                paths
                    .entry(parent.to_owned())
                    .or_insert(RecursiveMode::NonRecursive);
            }
            paths.insert(root.clone(), RecursiveMode::Recursive);
        }
        self.retry = false;
        for (path, mode) in paths {
            self.retry |= watcher.watch(&path, mode).is_err();
        }
        self.roots = roots.to_vec();
        self.watcher = Some(watcher);
        // Reconcile changes made while a new set of native watches was installed.
        self.signal
            .send_modify(|generation| *generation = generation.wrapping_add(1));
        !self.retry
    }

    pub async fn changed(&mut self) {
        if self.dirty_since.is_none() {
            let _ = self.changes.changed().await;
            self.dirty_since = Some(tokio::time::Instant::now());
        }
        let deadline = self.dirty_since.unwrap_or_else(tokio::time::Instant::now) + MAX_DELAY;
        loop {
            let quiet = (tokio::time::Instant::now() + QUIET_PERIOD).min(deadline);
            tokio::select! {
                result = self.changes.changed() => { if result.is_err() { return; } }
                () = tokio::time::sleep_until(quiet) => { self.dirty_since = None; return; }
            }
        }
    }
}

fn relevant(event: &Event, roots: &[PathBuf]) -> bool {
    matches!(
        event.kind,
        EventKind::Any
            | EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(_) | ModifyKind::Any)
    ) && (event.paths.is_empty()
        || event.paths.iter().any(|path| {
            roots.iter().any(|root| {
                root.starts_with(path)
                    || path.starts_with(root)
                        && (crate::local_media::is_video(path)
                            || matches!(
                                event.kind,
                                EventKind::Create(notify::event::CreateKind::Folder)
                                    | EventKind::Remove(notify::event::RemoveKind::Folder)
                            )
                            || path.extension().is_none()
                            || path.is_dir())
            })
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, DataChange, RenameMode};

    #[test]
    fn inventory_events_include_root_remounts_but_not_reads_or_unrelated_siblings() {
        let roots = vec![PathBuf::from("media/shows")];
        for kind in [
            EventKind::Create(CreateKind::File),
            EventKind::Remove(notify::event::RemoveKind::Folder),
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
        ] {
            assert!(relevant(
                &Event::new(kind).add_path(PathBuf::from("media/shows/new.mkv")),
                &roots
            ));
            assert!(relevant(
                &Event::new(kind).add_path(PathBuf::from("media")),
                &roots
            ));
            assert!(!relevant(
                &Event::new(kind).add_path(PathBuf::from("media/other/file.mkv")),
                &roots
            ));
        }
        for kind in [
            EventKind::Access(AccessKind::Any),
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
        ] {
            assert!(!relevant(
                &Event::new(kind).add_path(PathBuf::from("media/shows/file.mkv")),
                &roots
            ));
        }
        assert!(
            !relevant(
                &Event::new(EventKind::Create(CreateKind::File))
                    .add_path(PathBuf::from("media/shows/my-media.json")),
                &roots
            ),
            "Index writes inside a watched root must not trigger another scan"
        );
    }

    #[tokio::test]
    async fn a_burst_is_coalesced_without_losing_the_latest_change() {
        let mut folders = Folders::new();
        for _ in 0..100 {
            folders.signal.send_modify(|n| *n += 1);
        }
        folders.changed().await;
        assert_eq!(*folders.changes.borrow_and_update(), 100);
        assert!(!folders.changes.has_changed().unwrap_or(true));
    }

    #[tokio::test]
    async fn another_worker_command_during_debounce_does_not_lose_a_pending_scan() {
        let mut folders = Folders::new();
        folders.signal.send_modify(|n| *n += 1);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), folders.changed())
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(Duration::from_secs(1), folders.changed())
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    #[expect(
        clippy::unwrap_used,
        reason = "Native watching and temporary media fixtures must initialize"
    )]
    async fn live_changes_refresh_the_inventory_and_preserve_existing_matches() {
        let root = tempfile::tempdir().unwrap();
        let mut index = crate::local_media::Index::default();
        index.add_root(root.path()).unwrap();
        let mut folders = Folders::new();
        assert!(folders.set_roots(&index.roots));
        folders.changed().await;
        let path = root.path().join("Example.2020.mkv");
        std::fs::write(&path, []).unwrap();
        tokio::time::timeout(Duration::from_secs(5), folders.changed())
            .await
            .unwrap();
        index.scan();
        assert_eq!(index.items.len(), 1);
        index.items[0].keep_unmatched = true;
        std::fs::write(root.path().join("Other.2021.mkv"), []).unwrap();
        tokio::time::timeout(Duration::from_secs(5), folders.changed())
            .await
            .unwrap();
        index.scan();
        assert_eq!(index.items.len(), 2);
        assert!(
            index
                .items
                .iter()
                .find(|item| item.title == "Example")
                .unwrap()
                .keep_unmatched
        );
        std::fs::remove_file(path).unwrap();
        tokio::time::timeout(Duration::from_secs(5), folders.changed())
            .await
            .unwrap();
        index.scan();
        assert_eq!(index.items.len(), 1);
        assert_eq!(index.items[0].title, "Other");
    }
}
