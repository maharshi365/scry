use anyhow::{Context, Result, bail};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    time::SystemTime,
};

// Like Stremio, the current entry may exceed the budget until another video opens.
pub(crate) const DEFAULT_BUDGET: u64 = 2 * 1024 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_RETAINED_ENTRIES: usize = 128;
const MAX_ENTRY_PATHS: usize = 16_384;

pub(crate) struct Cache {
    root: PathBuf,
    budget: u64,
    // Keep exclusive ownership until the torrent engine has released its files.
    _lock: File,
}

impl Cache {
    pub(crate) fn open(root: &Path, budget: u64) -> Result<Self> {
        fs::create_dir_all(root)?;
        check_directory(root)?;
        let root = root.join("v1");
        fs::create_dir_all(&root)?;
        check_directory(&root)?;
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(".lock"))?;
        lock.try_lock().context("Torrent cache is already in use")?;
        Ok(Self {
            root,
            budget,
            _lock: lock,
        })
    }

    pub(crate) fn entry(&self, hash: &str) -> Result<PathBuf> {
        if !is_hash(hash) {
            bail!("Invalid torrent cache identity");
        }
        self.trim(hash)?;
        let entry = self.root.join(hash);
        fs::create_dir_all(&entry)?;
        check_directory(&entry)?;
        fs::create_dir_all(entry.join("data"))?;
        check_directory(&entry.join("data"))?;
        // Persistent data must not redirect engine writes through a link.
        entry_size(&entry)?;
        Ok(entry)
    }

    pub(crate) fn metadata(entry: &Path) -> Result<Option<Vec<u8>>> {
        let path = entry.join("metadata.torrent");
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
            Ok(metadata) => {
                if !metadata.is_file() || metadata.len() > MAX_METADATA_BYTES {
                    bail!("Invalid cached torrent metadata");
                }
                Ok(Some(fs::read(path)?))
            }
        }
    }

    pub(crate) fn save_metadata(entry: &Path, bytes: &[u8]) -> Result<()> {
        if bytes.len() as u64 > MAX_METADATA_BYTES {
            bail!("Torrent metadata exceeds cache limit");
        }
        let destination = entry.join("metadata.torrent");
        if !destination.exists() {
            let temporary = entry.join("metadata.tmp");
            fs::write(&temporary, bytes)?;
            fs::rename(temporary, destination)?;
        }
        File::create(entry.join("access"))?;
        Ok(())
    }

    pub(crate) fn trim(&self, active: &str) -> Result<()> {
        let mut entries = Vec::new();
        let mut total = 0_u64;
        for (count, item) in fs::read_dir(&self.root)?.enumerate() {
            if count >= MAX_ENTRIES {
                bail!("Torrent cache has too many entries");
            }
            let item = item?;
            let name = item.file_name();
            let Some(name) = name.to_str().filter(|name| is_hash(name)) else {
                continue;
            };
            if !item.file_type()?.is_dir() {
                continue;
            }
            let path = item.path();
            let size = entry_size(&path)?;
            total = total.saturating_add(size);
            if name != active {
                let accessed = fs::metadata(path.join("access"))
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                entries.push((accessed, path, size));
            }
        }
        entries.sort_by_key(|(accessed, _, _)| *accessed);
        let mut inactive = entries.len();
        for (_, path, size) in entries {
            // Reserve one slot even when the next active entry does not exist yet.
            if total <= self.budget && inactive < MAX_RETAINED_ENTRIES {
                break;
            }
            // Only hash directories in our versioned namespace, under the cache lock.
            fs::remove_dir_all(path)?;
            total = total.saturating_sub(size);
            inactive -= 1;
        }
        Ok(())
    }
}

fn is_hash(name: &str) -> bool {
    name.len() == 40 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn check_directory(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.is_dir() {
        bail!("Invalid torrent cache directory");
    }
    Ok(())
}

fn entry_size(root: &Path) -> Result<u64> {
    let mut pending = vec![root.to_owned()];
    let mut count = 0;
    let mut size = 0_u64;
    while let Some(path) = pending.pop() {
        count += 1;
        if count > MAX_ENTRY_PATHS {
            bail!("Torrent cache entry has too many paths");
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_symlink() {
            bail!("Torrent cache contains a symbolic link");
        }
        if metadata.is_dir() {
            for item in fs::read_dir(path)? {
                if pending.len() >= MAX_ENTRY_PATHS {
                    bail!("Torrent cache entry has too many paths");
                }
                pending.push(item?.path());
            }
        } else if metadata.is_file() {
            size = size.saturating_add(metadata.len());
        } else {
            bail!("Invalid torrent cache file");
        }
    }
    Ok(size)
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Cache fixture failures must fail the test immediately"
)]
mod tests {
    use super::*;

    fn retained_entries(cache: &Cache) -> Vec<PathBuf> {
        fs::read_dir(&cache.root)
            .unwrap()
            .map(|item| item.unwrap().path())
            .filter(|path| path.is_dir())
            .collect()
    }

    #[test]
    fn repeated_playback_and_restarts_keep_total_bytes_within_budget() {
        let root = tempfile::tempdir().unwrap();
        let budget = 1024;
        for batch in 0..8 {
            let cache = Cache::open(root.path(), budget).unwrap();
            for video in 0..32 {
                let hash = format!("{:040x}", batch * 32 + video);
                let entry = cache.entry(&hash).unwrap();
                fs::write(entry.join("data/video"), vec![1; 137]).unwrap();
                Cache::save_metadata(&entry, &[2; 23]).unwrap();
                cache.trim(&hash).unwrap();
                let entries = retained_entries(&cache);
                let total: u64 = entries.iter().map(|path| entry_size(path).unwrap()).sum();
                assert!(total <= budget, "video {hash}: retained {total} bytes");
                assert!(entries.len() <= MAX_RETAINED_ENTRIES);
                assert_eq!(fs::read(entry.join("data/video")).unwrap(), vec![1; 137]);
            }
        }
    }

    #[test]
    fn interrupted_starts_cannot_accumulate_unbounded_files_or_empty_entries() {
        let root = tempfile::tempdir().unwrap();
        let cache = Cache::open(root.path(), 256).unwrap();
        for video in 0..512 {
            let hash = format!("{video:040x}");
            let entry = cache.entry(&hash).unwrap();
            // Simulate cancellation after creating an entry or writing metadata.
            if video % 2 == 0 {
                fs::write(entry.join("metadata.tmp"), [0; 80]).unwrap();
            }
            let entries = retained_entries(&cache);
            let total: u64 = entries.iter().map(|path| entry_size(path).unwrap()).sum();
            assert!(total <= 256 + 80, "interruption {video}: {total} bytes");
            assert!(
                entries.len() <= MAX_RETAINED_ENTRIES,
                "interruption {video}"
            );
        }
        // Empty directories still consume disk space, even with a zero byte size.
        for video in 512..768 {
            cache.entry(&format!("{video:040x}")).unwrap();
            assert!(retained_entries(&cache).len() <= MAX_RETAINED_ENTRIES);
        }
    }

    #[test]
    fn oversized_video_exemption_does_not_accumulate_across_switches_or_restarts() {
        let root = tempfile::tempdir().unwrap();
        for video in 0..32 {
            let cache = Cache::open(root.path(), 64).unwrap();
            let hash = format!("{video:040x}");
            let entry = cache.entry(&hash).unwrap();
            fs::write(entry.join("data/video"), [0; 128]).unwrap();
            cache.trim(&hash).unwrap();
            assert_eq!(retained_entries(&cache), vec![entry]);
            assert_eq!(entry_size(&cache.root).unwrap(), 128);
        }
        let cache = Cache::open(root.path(), 64).unwrap();
        let hash = "f".repeat(40);
        let entry = cache.entry(&hash).unwrap();
        fs::write(entry.join("data/video"), [0; 16]).unwrap();
        cache.trim(&hash).unwrap();
        assert_eq!(retained_entries(&cache), vec![entry]);
        assert_eq!(entry_size(&cache.root).unwrap(), 16);
    }

    #[test]
    fn eviction_keeps_active_and_recent_entries_and_ignores_unowned_paths() {
        let root = tempfile::tempdir().unwrap();
        let cache = Cache::open(root.path(), 8).unwrap();
        let old = cache.entry(&"a".repeat(40)).unwrap();
        let recent = cache.entry(&"b".repeat(40)).unwrap();
        let active_hash = "c".repeat(40);
        let active = cache.entry(&active_hash).unwrap();
        for entry in [&old, &recent, &active] {
            fs::write(entry.join("data/video"), b"1234").unwrap();
        }
        let access = File::create(recent.join("access")).unwrap();
        access.set_modified(SystemTime::now()).unwrap();
        let unrelated = cache.root.join("user-files");
        fs::create_dir(&unrelated).unwrap();
        fs::write(unrelated.join("keep"), b"untouched").unwrap();
        cache.trim(&active_hash).unwrap();
        assert!(!old.exists());
        assert_eq!(fs::read(recent.join("data/video")).unwrap(), b"1234");
        assert_eq!(fs::read(active.join("data/video")).unwrap(), b"1234");
        assert_eq!(fs::read(unrelated.join("keep")).unwrap(), b"untouched");
        fs::write(active.join("data/video"), b"larger than the entire budget").unwrap();
        cache.trim(&active_hash).unwrap();
        assert!(!recent.exists());
        assert!(active.join("data/video").exists());
    }

    #[test]
    fn cache_lock_prevents_another_engine_from_evicting_live_files() {
        let root = tempfile::tempdir().unwrap();
        let cache = Cache::open(root.path(), 8).unwrap();
        assert!(Cache::open(root.path(), 0).is_err());
        drop(cache);
        assert!(Cache::open(root.path(), 0).is_ok());
    }

    #[test]
    fn invalid_identities_cannot_create_paths_outside_the_cache() {
        let root = tempfile::tempdir().unwrap();
        let cache = Cache::open(root.path(), 8).unwrap();
        for hash in ["../outside", "", "0123", &"z".repeat(40)] {
            assert!(cache.entry(hash).is_err(), "identity {hash}");
        }
    }

    #[test]
    fn cache_metadata_is_reusable_and_oversized_metadata_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let cache = Cache::open(root.path(), DEFAULT_BUDGET).unwrap();
        let entry = cache.entry(&"a".repeat(40)).unwrap();
        assert!(Cache::metadata(&entry).unwrap().is_none());
        Cache::save_metadata(&entry, b"torrent metadata").unwrap();
        assert_eq!(
            Cache::metadata(&entry).unwrap().unwrap(),
            b"torrent metadata"
        );
        assert!(!entry.join("metadata.tmp").exists());
        File::options()
            .write(true)
            .open(entry.join("metadata.torrent"))
            .unwrap()
            .set_len(MAX_METADATA_BYTES + 1)
            .unwrap();
        assert!(Cache::metadata(&entry).is_err());
    }

    #[test]
    fn eviction_never_sweeps_existing_unversioned_downloads() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("video"), b"existing user data").unwrap();
        let cache = Cache::open(root.path(), 0).unwrap();
        cache.entry(&"a".repeat(40)).unwrap();
        cache.trim(&"b".repeat(40)).unwrap();
        assert_eq!(
            fs::read(existing.join("video")).unwrap(),
            b"existing user data"
        );
    }
}
