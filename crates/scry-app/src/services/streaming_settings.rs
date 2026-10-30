use super::{Event, Inbox};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tokio::{runtime::Runtime, sync::mpsc};

pub(crate) const CACHE_SIZES_GIB: &[u64] = &[2, 5, 10, 20];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Preferences {
    pub cache_gib: u64,
}

impl Default for Preferences {
    fn default() -> Self {
        Self { cache_gib: 2 }
    }
}

impl Preferences {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            CACHE_SIZES_GIB.contains(&self.cache_gib),
            "Invalid torrent cache size"
        );
        Ok(())
    }

    fn cache_bytes(&self) -> u64 {
        self.cache_gib * 1024 * 1024 * 1024
    }
}

pub(super) fn start(runtime: &Runtime, data: PathBuf, inbox: Inbox) -> mpsc::Sender<Preferences> {
    let (tx, mut rx) = mpsc::channel::<Preferences>(1);
    runtime.spawn(async move {
        let folder = data.clone();
        let loaded = tokio::task::spawn_blocking(move || load(&folder)).await;
        let loaded =
            loaded.unwrap_or_else(|_| Err(anyhow::anyhow!("Cannot load streaming settings")));
        inbox
            .send(Event::StreamingPreferences(loaded.map_err(|_| {
                "Cannot load streaming settings; defaults are in use".into()
            })))
            .await;
        while let Some(preferences) = rx.recv().await {
            let folder = data.clone();
            let saved = tokio::task::spawn_blocking(move || {
                save(&folder, &preferences)?;
                Ok::<_, anyhow::Error>(preferences)
            })
            .await;
            let saved =
                saved.unwrap_or_else(|_| Err(anyhow::anyhow!("Cannot save streaming settings")));
            inbox
                .send(Event::StreamingPreferences(saved.map_err(|_| {
                    "Cannot save streaming settings; try again".into()
                })))
                .await;
        }
    });
    tx
}

pub(super) async fn cache_budget(data: PathBuf) -> Result<u64> {
    // Use the same safe default as the settings UI if the saved file is invalid.
    tokio::task::spawn_blocking(move || load(&data).unwrap_or_default().cache_bytes())
        .await
        .context("Cannot read streaming settings")
}

fn load(data: &Path) -> Result<Preferences> {
    let file = match std::fs::File::open(data.join("streaming-settings.json")) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Preferences::default());
        }
        Err(error) => return Err(error.into()),
    };
    let mut text = String::new();
    file.take(4097).read_to_string(&mut text)?;
    ensure!(text.len() <= 4096, "Streaming settings are too large");
    let preferences: Preferences = serde_json::from_str(&text)?;
    preferences.validate()?;
    Ok(preferences)
}

fn save(data: &Path, preferences: &Preferences) -> Result<()> {
    preferences.validate()?;
    std::fs::create_dir_all(data)?;
    let pending = data.join(format!(
        "streaming-settings-{}.pending",
        uuid::Uuid::new_v4()
    ));
    let result: Result<()> = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        file.write_all(&serde_json::to_vec(preferences)?)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&pending, data.join("streaming-settings.json"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(pending);
    }
    result.context("Cannot persist streaming preferences")
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Settings fixture failures must fail the test immediately"
)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn saved_cache_size_survives_replacement_and_is_used_by_the_next_playback() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(load(root.path()).unwrap(), Preferences::default());
        for cache_gib in CACHE_SIZES_GIB {
            let preferences = Preferences {
                cache_gib: *cache_gib,
            };
            save(root.path(), &preferences).unwrap();
            assert_eq!(load(root.path()).unwrap(), preferences);
            assert_eq!(
                cache_budget(root.path().into()).await.unwrap(),
                cache_gib * 1024 * 1024 * 1024
            );
        }
    }

    #[test]
    fn invalid_sizes_cannot_replace_a_valid_saved_budget() {
        let root = tempfile::tempdir().unwrap();
        save(root.path(), &Preferences { cache_gib: 5 }).unwrap();
        for cache_gib in [0, 1, 3, 21, u64::MAX] {
            assert!(save(root.path(), &Preferences { cache_gib }).is_err());
            assert_eq!(load(root.path()).unwrap().cache_gib, 5);
        }
    }

    #[tokio::test]
    async fn malformed_or_oversized_settings_use_a_bounded_default_budget() {
        let root = tempfile::tempdir().unwrap();
        for bytes in [
            b"{\"cache_gib\":18446744073709551615}".to_vec(),
            vec![b' '; 4097],
            b"invalid".to_vec(),
        ] {
            std::fs::write(root.path().join("streaming-settings.json"), bytes).unwrap();
            assert!(load(root.path()).is_err());
            assert_eq!(
                cache_budget(root.path().into()).await.unwrap(),
                2 * 1024 * 1024 * 1024
            );
        }
    }
}
