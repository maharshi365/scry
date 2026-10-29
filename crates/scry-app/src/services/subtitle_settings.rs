use super::{Event, Inbox};
use anyhow::{Context, Result, ensure};
use scry_playback::subtitles::Preferences;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tokio::{runtime::Runtime, sync::mpsc};

pub(super) fn start(runtime: &Runtime, data: PathBuf, inbox: Inbox) -> mpsc::Sender<Preferences> {
    let (tx, mut rx) = mpsc::channel::<Preferences>(1);
    runtime.spawn(async move {
        let folder = data.clone();
        let loaded = tokio::task::spawn_blocking(move || load(&folder)).await;
        let loaded =
            loaded.unwrap_or_else(|_| Err(anyhow::anyhow!("Cannot load subtitle settings")));
        inbox
            .send(Event::SubtitlePreferences(loaded.map_err(|_| {
                "Cannot load subtitle settings; defaults are in use".into()
            })))
            .await;
        while let Some(preferences) = rx.recv().await {
            let folder = data.clone();
            let saved = tokio::task::spawn_blocking(move || save(&folder, &preferences)).await;
            let saved =
                saved.unwrap_or_else(|_| Err(anyhow::anyhow!("Cannot save subtitle settings")));
            inbox
                .send(Event::SubtitlePreferencesSaved(saved.map_err(|_| {
                    "Cannot save subtitle settings; try again".into()
                })))
                .await;
        }
    });
    tx
}

fn load(data: &Path) -> Result<Preferences> {
    let file = match std::fs::File::open(data.join("subtitle-settings.json")) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Preferences::default());
        }
        Err(error) => return Err(error.into()),
    };
    let mut text = String::new();
    file.take(16 * 1024 + 1).read_to_string(&mut text)?;
    ensure!(text.len() <= 16 * 1024, "Subtitle settings are too large");
    let preferences: Preferences = serde_json::from_str(&text)?;
    preferences.validate()?;
    Ok(preferences)
}

fn save(data: &Path, preferences: &Preferences) -> Result<()> {
    preferences.validate()?;
    std::fs::create_dir_all(data)?;
    let pending = data.join(format!(
        "subtitle-settings-{}.pending",
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
        std::fs::rename(&pending, data.join("subtitle-settings.json"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(pending);
    }
    result.context("Cannot persist subtitle preferences")
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Settings fixtures must persist successfully"
)]
mod tests {
    use super::*;

    #[test]
    fn oversized_settings_do_not_accept_a_valid_truncated_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let mut bytes = serde_json::to_vec(&Preferences::default()).unwrap();
        bytes.resize(16 * 1024, b' ');
        bytes.extend_from_slice(b"private trailing garbage");
        std::fs::write(dir.path().join("subtitle-settings.json"), bytes).unwrap();
        assert!(load(dir.path()).is_err());
    }

    #[test]
    fn preferences_survive_restart_and_replacement() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()).unwrap(), Preferences::default());
        let mut preferences = Preferences {
            primary: Some("spa".into()),
            secondary: Some("eng".into()),
            ..Preferences::default()
        };
        save(dir.path(), &preferences).unwrap();
        assert_eq!(load(dir.path()).unwrap(), preferences);
        preferences.size = 150;
        save(dir.path(), &preferences).unwrap();
        assert_eq!(load(dir.path()).unwrap().size, 150);
        preferences.position = 101;
        assert!(save(dir.path(), &preferences).is_err());
        assert_eq!(load(dir.path()).unwrap().position, 95);
    }
}
