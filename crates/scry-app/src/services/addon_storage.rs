//! Add-on configuration encoding and commit-before-publish updates.
use anyhow::{Context, Result, bail};
use scry_addons::Addon;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

fn database(data: &Path) -> Result<scry_library::Library> {
    std::fs::create_dir_all(data)?;
    scry_library::Library::open(data.join(scry_library::DATABASE_NAME))
}

// Local preferences share the stored manifest document, not the remote manifest.
// The optional flag needs no database schema change or data migration.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredManifest {
    #[serde(flatten)]
    manifest: scry_addons::Manifest,
    #[serde(default, rename = "_scry_disabled")]
    disabled: bool,
}

pub(super) fn load(data: &Path, initialize: bool) -> Result<Vec<Addon>> {
    let records = database(data)?.load_addons()?;
    if initialize && records.is_empty() {
        let addons = vec![Addon {
            manifest_url: super::KITSU.into(),
            manifest: serde_json::from_str(include_str!("../../assets/kitsu-manifest.json"))
                .context("Cannot read the built-in Kitsu manifest")?,
            disabled: false,
        }];
        save(data, &addons)?;
        return Ok(addons);
    }
    if records.len() > 32 {
        bail!("At most 32 installed add-ons are supported");
    }
    records
        .into_iter()
        .map(|record| {
            let stored: StoredManifest = serde_json::from_str(&record.manifest_json)
                .context("Cannot read stored add-on manifest")?;
            if stored.manifest.id != record.id {
                bail!("Stored add-on identity does not match its manifest");
            }
            Ok(Addon {
                manifest_url: record.manifest_url,
                manifest: stored.manifest,
                disabled: stored.disabled,
            })
        })
        .collect()
}

pub(super) fn save(data: &Path, addons: &[Addon]) -> Result<()> {
    let records = addons
        .iter()
        .map(|addon| {
            Ok(scry_library::AddonRecord {
                id: addon.manifest.id.clone(),
                manifest_url: addon.manifest_url.clone(),
                manifest_json: serde_json::to_string(&StoredManifest {
                    manifest: addon.manifest.clone(),
                    disabled: addon.disabled,
                })?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    database(data)?
        .replace_addons(&records)
        .context("Cannot save add-ons configuration")
}

pub(super) async fn update(
    config: &Mutex<Vec<Addon>>,
    data: PathBuf,
    change: impl FnOnce(&mut Vec<Addon>) -> Result<()>,
) -> Result<Vec<Addon>> {
    let mut current = config.lock().await;
    let mut next = current.clone();
    change(&mut next)?;
    let stored = next.clone();
    tokio::task::spawn_blocking(move || save(&data, &stored))
        .await
        .context("Cannot save add-ons configuration")??;
    current.clone_from(&next);
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn updates_publish_only_after_a_successful_commit() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let original = load(directory.path(), true)?;
        let config = Mutex::new(original);
        let blocked = directory.path().join("not-a-directory");
        std::fs::write(&blocked, b"blocked")?;
        assert!(
            update(&config, blocked, |next| {
                next.clear();
                Ok(())
            })
            .await
            .is_err()
        );
        assert_eq!(config.lock().await.len(), 1);
        assert_eq!(load(directory.path(), false)?.len(), 1);

        assert!(
            update(&config, directory.path().into(), |next| {
                next.clear();
                bail!("invalid edit")
            })
            .await
            .is_err()
        );
        assert_eq!(config.lock().await.len(), 1);
        assert_eq!(load(directory.path(), false)?.len(), 1);

        let saved = update(&config, directory.path().into(), |next| {
            next.clear();
            Ok(())
        })
        .await?;
        assert!(saved.is_empty());
        assert!(config.lock().await.is_empty());
        assert!(load(directory.path(), false)?.is_empty());
        Ok(())
    }
}
