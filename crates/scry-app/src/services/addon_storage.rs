//! Add-on configuration encoding and commit-before-publish updates.
use anyhow::{Context, Result, bail};
use scry_addons::Addon;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

pub(super) async fn refresh(client: &scry_addons::Client, addons: Vec<Addon>) -> Vec<Addon> {
    let mut remaining = addons.into_iter().enumerate();
    let mut tasks = tokio::task::JoinSet::new();
    let mut refreshed = std::collections::BTreeMap::new();
    loop {
        while tasks.len() < 4 {
            let Some((index, mut addon)) = remaining.next() else {
                break;
            };
            let client = client.clone();
            tasks.spawn(async move {
                if !addon.disabled
                    && let Ok(fresh) = client.install(&addon.manifest_url).await
                    && fresh.manifest.id == addon.manifest.id
                {
                    addon.manifest = fresh.manifest;
                }
                (index, addon)
            });
        }
        let Some(result) = tasks.join_next().await else {
            break;
        };
        if let Ok((index, addon)) = result {
            refreshed.insert(index, addon);
        }
    }
    refreshed.into_values().collect()
}

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
    #[expect(
        clippy::unwrap_used,
        reason = "Manifest refresh fixtures must fail tests on error"
    )]
    async fn refresh_keeps_disabled_failed_and_wrong_identity_manifests() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 4096];
                let length = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..length]);
                let valid = request.contains("/valid/");
                let failed = request.contains("/failed/");
                let body = serde_json::json!({"id": if valid { "valid" } else { "unexpected" }, "name":"Updated", "catalogs":[{"id":"new", "type":"movie"}]}).to_string();
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            if failed { "503 Unavailable" } else { "200 OK" },
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let addons: Vec<Addon> = ["valid", "failed", "wrong", "disabled"].into_iter().map(|id| serde_json::from_value(serde_json::json!({
            "manifest_url":format!("http://{address}/{id}/manifest.json"), "disabled":id == "disabled", "manifest":{"id":id,"name":"Original"}
        })).unwrap()).collect();
        let refreshed = refresh(&scry_addons::Client::new().unwrap(), addons).await;
        assert_eq!(refreshed[0].manifest.name, "Updated");
        assert_eq!(refreshed[0].manifest.catalogs[0].id, "new");
        assert!(
            refreshed.iter().skip(1).all(
                |addon| addon.manifest.name == "Original" && addon.manifest.catalogs.is_empty()
            )
        );
        assert!(refreshed[3].disabled);
        assert_eq!(refreshed[2].manifest.id, "wrong");
        server.await.unwrap();
    }

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
