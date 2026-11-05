use super::{Entry, MAX_BODY, MAX_ENTRIES};
use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

const BUDGET: u64 = 16 * 1024 * 1024;
const MAX_HEADER: usize = 64 * 1024;
const RETENTION: Duration = Duration::from_hours(7 * 24);

#[derive(Clone)]
pub(super) struct Disk {
    path: Arc<Path>,
    write: Arc<Mutex<()>>,
}

#[derive(Deserialize)]
struct Stored {
    policy: CachePolicy,
    received: SystemTime,
    headers: Vec<(String, String)>,
    checksum: [u8; 32],
}

impl Disk {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            path: path.into(),
            write: Arc::new(Mutex::new(())),
        }
    }

    fn key(&self, key: &str) -> PathBuf {
        self.path
            .join(format!("{}.http", blake3::hash(key.as_bytes()).to_hex()))
    }

    pub(super) fn load(&self, key: &str) -> Option<Entry> {
        let mut file = fs::File::open(self.key(key)).ok()?;
        let file_length = usize::try_from(file.metadata().ok()?.len()).ok()?;
        if file_length > MAX_BODY + MAX_HEADER + 4 {
            return None;
        }
        let mut size = [0; 4];
        file.read_exact(&mut size).ok()?;
        let length = usize::try_from(u32::from_le_bytes(size)).ok()?;
        if length > MAX_HEADER {
            return None;
        }
        let mut header = vec![0; length];
        file.read_exact(&mut header).ok()?;
        let stored: Stored = serde_json::from_slice(&header).ok()?;
        drop(header);
        if SystemTime::now().duration_since(stored.received).ok()? > RETENTION
            || !stored.policy.is_storable()
        {
            return None;
        }
        let body_length = file_length.checked_sub(length + 4)?;
        if body_length > MAX_BODY {
            return None;
        }
        let mut bytes = Vec::with_capacity(body_length);
        file.take((MAX_BODY + 1) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > MAX_BODY {
            return None;
        }
        if blake3::hash(&bytes).as_bytes() != &stored.checksum {
            return None;
        }
        let mut headers = http::HeaderMap::new();
        for (name, value) in stored.headers {
            headers.append(
                http::header::HeaderName::from_bytes(name.as_bytes()).ok()?,
                http::HeaderValue::from_str(&value).ok()?,
            );
        }
        Some(Entry {
            policy: stored.policy,
            received: stored.received,
            bytes: Arc::new(bytes),
            headers,
        })
    }

    pub(super) fn store(&self, key: &str, entry: &Entry) {
        let Ok(_write) = self.write.lock() else {
            return;
        };
        if self
            .load(key)
            .is_some_and(|stored| stored.received > entry.received)
        {
            return;
        }
        if !entry.policy.is_storable() {
            let _ = fs::remove_file(self.key(key));
            return;
        }
        let _ = self.write(key, entry);
    }

    pub(super) fn remove(&self, key: &str) {
        if let Ok(_write) = self.write.lock() {
            let _ = fs::remove_file(self.key(key));
        }
    }

    fn write(&self, key: &str, entry: &Entry) -> std::io::Result<()> {
        #[derive(Serialize)]
        struct StoredRef<'a> {
            policy: &'a CachePolicy,
            received: SystemTime,
            #[serde(serialize_with = "serialize_headers")]
            headers: &'a http::HeaderMap,
            checksum: [u8; 32],
        }

        fs::create_dir_all(&self.path)?;
        let stored = StoredRef {
            policy: &entry.policy,
            received: entry.received,
            headers: &entry.headers,
            checksum: *blake3::hash(&entry.bytes).as_bytes(),
        };
        let header = serde_json::to_vec(&stored)?;
        if header.len() > MAX_HEADER {
            return Ok(());
        }
        let temporary = self.path.join(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(
                &u32::try_from(header.len())
                    .map_err(std::io::Error::other)?
                    .to_le_bytes(),
            )?;
            file.write_all(&header)?;
            file.write_all(&entry.bytes)?;
            drop(file);
            fs::rename(&temporary, self.key(key))?;
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
        for file in fs::read_dir(&self.path)?.flatten() {
            let path = file.path();
            if !owned(&path) {
                continue;
            }
            let Ok(meta) = file.metadata() else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let modified = meta.modified()?;
            if SystemTime::now()
                .duration_since(modified)
                .unwrap_or_default()
                > RETENTION
            {
                fs::remove_file(path)?;
                continue;
            }
            total += meta.len();
            entries.push((modified, path, meta.len()));
        }
        entries.sort_by_key(|entry| entry.0);
        let mut count = entries.len();
        for (_, path, bytes) in entries {
            if total <= BUDGET && count <= MAX_ENTRIES {
                break;
            }
            fs::remove_file(path)?;
            total -= bytes;
            count -= 1;
        }
        Ok(())
    }
}

fn serialize_headers<S: serde::Serializer>(
    headers: &http::HeaderMap,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(
        headers
            .iter()
            .filter_map(|(key, value)| Some((key.as_str(), value.to_str().ok()?))),
    )
}

fn owned(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    match path.extension().and_then(|s| s.to_str()) {
        Some("http") => stem.len() == 64 && stem.bytes().all(|c| c.is_ascii_hexdigit()),
        Some("tmp") => uuid::Uuid::parse_str(stem).is_ok(),
        _ => false,
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Cache file fixtures must fail tests on error"
)]
mod tests {
    use super::*;

    fn entry(body: &[u8], directive: &str) -> Entry {
        let request = http::Request::get("https://addon.example/private/meta/movie/film.json")
            .body(())
            .unwrap();
        let response = http::Response::builder()
            .header("cache-control", directive)
            .body(())
            .unwrap();
        Entry {
            policy: CachePolicy::new(&request, &response),
            bytes: Arc::new(body.to_vec()),
            received: SystemTime::now(),
            headers: response.headers().clone(),
        }
    }

    #[test]
    fn corrupt_sources_are_misses_and_no_store_removes_previous_data() {
        let directory = tempfile::tempdir().unwrap();
        let disk = Disk::new(directory.path().into());
        disk.store("film", &entry(b"poster", "max-age=60"));
        let mut corrupted = fs::read(disk.key("film")).unwrap();
        *corrupted.last_mut().unwrap() = 0;
        fs::write(disk.key("film"), corrupted).unwrap();
        assert!(disk.load("film").is_none());
        disk.store("film", &entry(b"updated", "max-age=60"));
        assert_eq!(disk.load("film").unwrap().bytes.as_slice(), b"updated");
        disk.store("film", &entry(b"private", "no-store"));
        assert!(!disk.key("film").exists());
    }

    #[test]
    fn disk_headers_preserve_repeated_values_and_skip_non_text_values() {
        let directory = tempfile::tempdir().unwrap();
        let disk = Disk::new(directory.path().into());
        let mut source = entry(b"poster", "max-age=60");
        source.headers.append("x-label", "first".parse().unwrap());
        source.headers.append("x-label", "second".parse().unwrap());
        source
            .headers
            .insert("x-binary", http::HeaderValue::from_bytes(b"\xff").unwrap());
        disk.store("film", &source);

        let loaded = disk.load("film").unwrap();
        let labels: Vec<_> = loaded.headers.get_all("x-label").iter().collect();
        assert_eq!(labels, ["first", "second"]);
        assert!(!loaded.headers.contains_key("x-binary"));
        assert_eq!(loaded.bytes.as_slice(), b"poster");
    }

    #[test]
    fn late_older_requests_cannot_overwrite_newer_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let disk = Disk::new(directory.path().into());
        let older = entry(b"old", "max-age=60");
        let newer = entry(b"new", "max-age=60");
        disk.store("film", &newer);
        disk.store("film", &older);
        assert_eq!(disk.load("film").unwrap().bytes.as_slice(), b"new");
    }

    #[test]
    fn disk_budget_and_entry_limit_preserve_unrelated_files() {
        let directory = tempfile::tempdir().unwrap();
        let disk = Disk::new(directory.path().into());
        fs::write(directory.path().join("keep.txt"), b"not cache data").unwrap();
        for index in 0..=MAX_ENTRIES {
            disk.store(&index.to_string(), &entry(b"small", "max-age=60"));
        }
        assert_eq!(
            fs::read_dir(directory.path())
                .unwrap()
                .flatten()
                .filter(|entry| owned(&entry.path()))
                .count(),
            MAX_ENTRIES
        );
        assert_eq!(
            fs::read(directory.path().join("keep.txt")).unwrap(),
            b"not cache data"
        );
        for index in 0..6 {
            disk.store(
                &format!("large-{index}"),
                &entry(&vec![42; MAX_BODY], "max-age=60"),
            );
        }
        let total: u64 = fs::read_dir(directory.path())
            .unwrap()
            .flatten()
            .filter(|entry| owned(&entry.path()))
            .map(|entry| entry.metadata().unwrap().len())
            .sum();
        assert!(total <= BUDGET);
    }
}
