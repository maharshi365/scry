use super::Command;
use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher, event::ModifyKind,
};
use std::{
    collections::{BTreeMap, BTreeSet},
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
// A large burst needs one full scan, not an unbounded queue of paths.
const DIRECTORY_LIMIT: usize = 256;
const PATH_LIMIT: usize = 32 * 1024;

#[derive(Default)]
enum Changes {
    #[default]
    Idle,
    Full,
    Directories(BTreeSet<PathBuf>),
}

impl Changes {
    fn add(&mut self, directory: PathBuf) {
        if matches!(self, Self::Full) {
            return;
        }
        if directory.as_os_str().as_encoded_bytes().len() > PATH_LIMIT {
            *self = Self::Full;
            return;
        }
        if matches!(self, Self::Idle) {
            *self = Self::Directories(BTreeSet::new());
        }
        if let Self::Directories(directories) = self {
            if directories
                .iter()
                .any(|parent| directory.starts_with(parent))
            {
                return;
            }
            directories.retain(|child| !child.starts_with(&directory));
            directories.insert(directory);
            if directories.len() > DIRECTORY_LIMIT {
                *self = Self::Full;
            }
        }
    }

    fn record(&mut self, event: &Event, roots: &[PathBuf]) {
        if event.need_rescan()
            || event.paths.is_empty()
            || event
                .paths
                .iter()
                .any(|path| roots.iter().any(|root| root.starts_with(path)))
        {
            *self = Self::Full;
            return;
        }
        for path in &event.paths {
            if roots.iter().any(|root| path.starts_with(root))
                && let Some(parent) = path.parent()
            {
                self.add(parent.to_owned());
            }
        }
    }
}

pub(super) struct Folders {
    watcher: Option<RecommendedWatcher>,
    roots: Vec<PathBuf>,
    changes: watch::Receiver<Changes>,
    signal: watch::Sender<Changes>,
    retry: bool,
    dirty_since: Option<tokio::time::Instant>,
    rearm: Arc<AtomicBool>,
}

impl Folders {
    pub fn new() -> Self {
        let (signal, changes) = watch::channel(Changes::Idle);
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
                        event.need_rescan()
                            || event.paths.is_empty()
                            || event
                                .paths
                                .iter()
                                .any(|path| selected.iter().any(|root| root.starts_with(path)))
                    }) {
                        rearm.store(true, Ordering::Release);
                    }
                    signal.send_modify(|changes| match &event {
                        Ok(event) => changes.record(event, &selected),
                        Err(_) => *changes = Changes::Full,
                    });
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
        self.signal.send_modify(|changes| *changes = Changes::Full);
        !self.retry
    }

    pub async fn changed(&mut self) -> Command {
        loop {
            if self.dirty_since.is_none() {
                let _ = self.changes.changed().await;
                self.dirty_since = Some(tokio::time::Instant::now());
            }
            let deadline = self.dirty_since.unwrap_or_else(tokio::time::Instant::now) + MAX_DELAY;
            loop {
                let quiet = (tokio::time::Instant::now() + QUIET_PERIOD).min(deadline);
                tokio::select! {
                    result = self.changes.changed() => { if result.is_err() { return Command::Scan; } }
                    () = tokio::time::sleep_until(quiet) => break,
                }
            }
            self.dirty_since = None;
            let mut batch = Changes::Idle;
            // Drain under the sender's write lock without waking our own receiver. A callback
            // racing the last debounce tick is either in this batch or wakes the next pass.
            self.signal.send_if_modified(|changes| {
                batch = std::mem::take(changes);
                false
            });
            match batch {
                Changes::Full => return Command::Scan,
                Changes::Directories(directories) => {
                    return Command::ScanChanged(directories.into_iter().collect());
                }
                Changes::Idle => {}
            }
        }
    }
}

fn relevant(event: &Event, roots: &[PathBuf]) -> bool {
    event.need_rescan()
        || matches!(
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
    fn lost_events_force_a_scan_even_when_the_event_kind_is_not_inventory_related() {
        let event = Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan);
        assert!(relevant(&event, &[PathBuf::from("media")]));
        let mut changes = Changes::Idle;
        changes.record(&event, &[PathBuf::from("media")]);
        assert!(matches!(changes, Changes::Full));
    }

    #[test]
    fn dirty_directories_collapse_subtrees_and_fall_back_when_bounded_capacity_is_exceeded() {
        let mut changes = Changes::Idle;
        changes.add(PathBuf::from("media/show/season"));
        changes.add(PathBuf::from("media/show"));
        changes.add(PathBuf::from("media/show/other"));
        assert!(
            matches!(&changes, Changes::Directories(dirs) if *dirs == BTreeSet::from([PathBuf::from("media/show")]))
        );
        for n in 0..=DIRECTORY_LIMIT {
            changes.add(PathBuf::from(format!("other/{n}")));
        }
        assert!(matches!(changes, Changes::Full));
        changes.add(PathBuf::from("small"));
        assert!(
            matches!(changes, Changes::Full),
            "Further events must not downgrade a full scan"
        );
    }

    #[test]
    fn moves_scan_both_parents_and_root_changes_request_a_full_scan() {
        let roots = [PathBuf::from("media")];
        let mut changes = Changes::Idle;
        changes.record(
            &Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                .add_path(PathBuf::from("media/old/A.mkv"))
                .add_path(PathBuf::from("media/new/A.mkv")),
            &roots,
        );
        assert!(
            matches!(&changes, Changes::Directories(dirs) if *dirs == BTreeSet::from([PathBuf::from("media/old"), PathBuf::from("media/new")]))
        );
        changes.record(
            &Event::new(EventKind::Remove(notify::event::RemoveKind::Folder))
                .add_path(PathBuf::from("media")),
            &roots,
        );
        assert!(matches!(changes, Changes::Full));
    }

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
        for n in 0..100 {
            folders
                .signal
                .send_modify(|changes| changes.add(PathBuf::from(format!("media/{n}"))));
        }
        let command = folders.changed().await;
        assert!(
            matches!(command, Command::ScanChanged(dirs) if dirs.len() == 100 && dirs.contains(&PathBuf::from("media/99")))
        );
        assert!(!folders.changes.has_changed().unwrap_or(true));
        folders
            .signal
            .send_modify(|changes| changes.add(PathBuf::from("next")));
        assert!(
            matches!(folders.changed().await, Command::ScanChanged(dirs) if dirs == [PathBuf::from("next")])
        );
    }

    #[tokio::test]
    async fn another_worker_command_during_debounce_does_not_lose_a_pending_scan() {
        let mut folders = Folders::new();
        folders
            .signal
            .send_modify(|changes| changes.add(PathBuf::from("pending")));
        assert!(
            tokio::time::timeout(Duration::from_millis(20), folders.changed())
                .await
                .is_err()
        );
        let command = tokio::time::timeout(Duration::from_secs(1), folders.changed()).await;
        assert!(
            matches!(command, Ok(Command::ScanChanged(dirs)) if dirs == [PathBuf::from("pending")])
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
        let command = tokio::time::timeout(Duration::from_secs(5), folders.changed())
            .await
            .unwrap();
        apply_scan(&mut index, command);
        assert_eq!(index.items.len(), 1);
        index.items[0].keep_unmatched = true;
        std::fs::write(root.path().join("Other.2021.mkv"), []).unwrap();
        let command = tokio::time::timeout(Duration::from_secs(5), folders.changed())
            .await
            .unwrap();
        apply_scan(&mut index, command);
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
        let command = tokio::time::timeout(Duration::from_secs(5), folders.changed())
            .await
            .unwrap();
        apply_scan(&mut index, command);
        assert_eq!(index.items.len(), 1);
        assert_eq!(index.items[0].title, "Other");
    }

    fn apply_scan(index: &mut crate::local_media::Index, command: Command) {
        if let Command::ScanChanged(directories) = command {
            index.scan_changed(&directories);
        } else {
            assert!(matches!(command, Command::Scan));
            index.scan();
        }
    }
}
