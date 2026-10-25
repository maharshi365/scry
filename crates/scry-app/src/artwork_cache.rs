//! Bounded, disposable thumbnail storage. Keys contain no URLs or title names.
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const BUDGET: u64 = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;
const MAX_PIXELS: usize = 160 * 240 * 4;
const LIFETIME: Duration = Duration::from_hours(7 * 24);

pub struct DiskCache {
    path: PathBuf,
    budget: u64,
}

impl DiskCache {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            budget: BUDGET,
        }
    }

    fn key(&self, url: &str) -> PathBuf {
        self.path
            .join(format!("{}.rgba", blake3::hash(url.as_bytes()).to_hex()))
    }

    pub fn load(&self, url: &str) -> Option<(u32, u32, Vec<u8>)> {
        let path = self.key(url);
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .or_else(|_| fs::File::open(path))
            .ok()?;
        let metadata = file.metadata().ok()?;
        let modified = metadata.modified().ok()?;
        if SystemTime::now().duration_since(modified).ok()? > LIFETIME {
            return None;
        }
        let length = metadata.len();
        if !(8..=(MAX_PIXELS + 8) as u64).contains(&length) {
            return None;
        }
        let mut header = [0; 8];
        file.read_exact(&mut header).ok()?;
        let width = u32::from_le_bytes(header[..4].try_into().ok()?);
        let height = u32::from_le_bytes(header[4..].try_into().ok()?);
        let mut pixels = Vec::with_capacity(usize::try_from(length - 8).ok()?);
        (&file)
            .take((MAX_PIXELS + 1) as u64)
            .read_to_end(&mut pixels)
            .ok()?;
        if !valid(width, height, pixels.len()) {
            return None;
        }
        // Keep frequently revisited thumbnails ahead of old entries for eviction.
        let _ = file.set_times(fs::FileTimes::new().set_accessed(SystemTime::now()));
        Some((width, height, pixels))
    }

    pub fn store(&self, url: &str, width: u32, height: u32, pixels: &[u8]) {
        if !valid(width, height, pixels.len()) {
            return;
        }
        let _ = self.write(url, width, height, pixels);
    }

    fn write(&self, url: &str, width: u32, height: u32, pixels: &[u8]) -> std::io::Result<()> {
        fs::create_dir_all(&self.path)?;
        let temporary = self.path.join(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&width.to_le_bytes())?;
            file.write_all(&height.to_le_bytes())?;
            file.write_all(pixels)?;
            drop(file);
            fs::rename(&temporary, self.key(url))?;
            self.prune()
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }

    fn prune(&self) -> std::io::Result<()> {
        let mut entries = Vec::new();
        let mut total = 0;
        for entry in fs::read_dir(&self.path)?.flatten() {
            let path = entry.path();
            if !owned_file(&path) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let modified = metadata.modified()?;
            if SystemTime::now()
                .duration_since(modified)
                .unwrap_or_default()
                > LIFETIME
            {
                fs::remove_file(path)?;
                continue;
            }
            total += metadata.len();
            entries.push((
                metadata.accessed().unwrap_or(modified),
                path,
                metadata.len(),
            ));
        }
        entries.sort_by_key(|entry| entry.0);
        let mut count = entries.len();
        for (_, path, size) in entries {
            if total <= self.budget && count <= MAX_ENTRIES {
                break;
            }
            fs::remove_file(path)?;
            total -= size;
            count -= 1;
        }
        Ok(())
    }
}

fn valid(width: u32, height: u32, length: usize) -> bool {
    width > 0
        && height > 0
        && width <= 160
        && height <= 240
        && length == width as usize * height as usize * 4
}

fn owned_file(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("rgba") => stem.len() == 64 && stem.bytes().all(|byte| byte.is_ascii_hexdigit()),
        Some("tmp") => uuid::Uuid::parse_str(stem).is_ok(),
        _ => false,
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Filesystem fixtures and asserted cache hits must fail the test on error"
)]
mod tests {
    use super::*;

    #[test]
    fn revisits_read_exact_pixels_from_a_new_cache_instance() {
        let directory = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(directory.path().into());
        let url = "https://example.com/private-title?token=secret";
        cache.store(url, 2, 1, &[1, 2, 3, 255, 4, 5, 6, 255]);
        let reopened = DiskCache::new(directory.path().into());
        assert_eq!(
            reopened.load(url),
            Some((2, 1, vec![1, 2, 3, 255, 4, 5, 6, 255]))
        );
        assert!(
            !cache
                .key(url)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .contains("secret")
        );
        assert_eq!(reopened.load("https://example.com/other"), None);
        reopened.store(url, 1, 1, &[9, 8, 7, 255]);
        assert_eq!(reopened.load(url), Some((1, 1, vec![9, 8, 7, 255])));
    }

    #[test]
    fn corrupt_oversized_and_expired_entries_are_misses() {
        let directory = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(directory.path().into());
        cache.store("a", 1, 1, &[0; 4]);
        let file = fs::File::options()
            .write(true)
            .open(cache.key("a"))
            .unwrap();
        file.set_times(
            fs::FileTimes::new()
                .set_modified(SystemTime::now() - LIFETIME - Duration::from_secs(1)),
        )
        .unwrap();
        assert!(cache.load("a").is_none());
        drop(file);
        fs::write(cache.key("b"), [0; 7]).unwrap();
        fs::write(cache.key("c"), vec![0; MAX_PIXELS + 100]).unwrap();
        assert!(cache.load("b").is_none());
        assert!(cache.load("c").is_none());
        cache.store("d", 161, 1, &[0; 644]);
        assert!(!cache.key("d").exists());
        cache.prune().unwrap();
        assert!(!cache.key("a").exists());
    }

    #[test]
    fn invalid_dimensions_and_mismatched_pixel_lengths_are_misses() {
        let directory = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(directory.path().into());
        for (width, height, length) in [
            (0_u32, 1_u32, 4),
            (161, 1, 644),
            (1, 241, 964),
            (2, 1, 4),
            (1, 1, 8),
        ] {
            let mut bytes = width.to_le_bytes().to_vec();
            bytes.extend_from_slice(&height.to_le_bytes());
            bytes.resize(8 + length, 0);
            fs::write(cache.key("invalid"), bytes).unwrap();
            assert!(cache.load("invalid").is_none());
        }
        let pixels = vec![42; MAX_PIXELS];
        cache.store("maximum", 160, 240, &pixels);
        assert_eq!(cache.load("maximum"), Some((160, 240, pixels)));
    }

    #[test]
    fn byte_budget_evicts_oldest_and_preserves_unrelated_files() {
        let directory = tempfile::tempdir().unwrap();
        let cache = DiskCache {
            path: directory.path().into(),
            budget: 24,
        };
        fs::write(directory.path().join("keep.txt"), "not cache data").unwrap();
        cache.store("a", 1, 1, &[1; 4]);
        fs::File::options()
            .write(true)
            .open(cache.key("a"))
            .unwrap()
            .set_times(
                fs::FileTimes::new().set_accessed(SystemTime::now() - Duration::from_mins(1)),
            )
            .unwrap();
        cache.store("b", 1, 1, &[2; 4]);
        cache.store("c", 1, 1, &[3; 4]);
        assert!(cache.load("a").is_none());
        assert_eq!(cache.load("b").unwrap().2, vec![2; 4]);
        assert_eq!(cache.load("c").unwrap().2, vec![3; 4]);
        assert_eq!(
            fs::read_to_string(directory.path().join("keep.txt")).unwrap(),
            "not cache data"
        );
    }

    #[test]
    fn revisits_promote_eviction_priority_without_extending_freshness() {
        let directory = tempfile::tempdir().unwrap();
        let cache = DiskCache {
            path: directory.path().into(),
            budget: 24,
        };
        for (key, age) in [("a", 60), ("b", 30)] {
            cache.store(key, 1, 1, &[1; 4]);
            fs::File::options()
                .write(true)
                .open(cache.key(key))
                .unwrap()
                .set_times(
                    fs::FileTimes::new().set_accessed(SystemTime::now() - Duration::from_secs(age)),
                )
                .unwrap();
        }
        let modified = fs::metadata(cache.key("a")).unwrap().modified().unwrap();
        assert!(cache.load("a").is_some());
        assert_eq!(
            fs::metadata(cache.key("a")).unwrap().modified().unwrap(),
            modified
        );
        cache.store("c", 1, 1, &[3; 4]);
        assert!(cache.load("a").is_some());
        assert!(cache.load("b").is_none());
        assert_eq!(cache.load("c").unwrap().2, vec![3; 4]);
    }

    #[test]
    fn entry_count_is_bounded_even_for_tiny_thumbnails() {
        let directory = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(directory.path().into());
        for index in 0..=MAX_ENTRIES {
            fs::write(cache.key(&index.to_string()), [0; 12]).unwrap();
        }
        cache.prune().unwrap();
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), MAX_ENTRIES);
    }

    #[test]
    fn cache_storage_failure_is_a_miss_not_an_artwork_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("not-a-directory");
        fs::write(&path, "occupied").unwrap();
        let cache = DiskCache::new(path.clone());
        cache.store("a", 1, 1, &[0; 4]);
        assert!(cache.load("a").is_none());
        assert_eq!(fs::read_to_string(path).unwrap(), "occupied");
    }
}
