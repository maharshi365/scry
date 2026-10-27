use super::{Event, Services};
use anyhow::{Result, ensure};
use scry_addons::{Addon, Stream, Subtitle};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[derive(Clone, Debug)]
pub struct ExternalSubtitle {
    pub provider: String,
    pub subtitle: Subtitle,
}

#[derive(Debug)]
pub struct SubtitleFile {
    pub path: PathBuf,
}

impl Drop for SubtitleFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Services {
    pub fn cancel_subtitles(&mut self) {
        for task in [&mut self.subtitle_task, &mut self.subtitle_file_task] {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
    }

    pub fn subtitles(
        &mut self,
        generation: u64,
        identity: Option<(String, String)>,
        stream: Option<Stream>,
        addons: Vec<Addon>,
    ) {
        self.cancel_subtitles();
        let (client, inbox) = (self.client.clone(), self.inbox.clone());
        self.subtitle_task = Some(self.runtime.spawn(async move {
            let supplied = stream
                .as_ref()
                .map(|stream| {
                    stream
                        .subtitles
                        .iter()
                        .take(100)
                        .filter(|s| s.validate_url().is_ok())
                        .map(|s| ExternalSubtitle {
                            provider: "Stream".into(),
                            subtitle: s.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            inbox
                .send(Event::SubtitleSources(generation, supplied, false, false))
                .await;
            let Some((kind, id)) = identity else {
                inbox
                    .send(Event::SubtitleSources(generation, vec![], true, false))
                    .await;
                return;
            };
            let hints = stream.map_or(serde_json::Value::Null, |s| s.behavior_hints);
            let mut providers = addons
                .into_iter()
                .filter(|a| !a.disabled && a.supports("subtitles", &kind, &id));
            let mut tasks = tokio::task::JoinSet::new();
            let mut failed = false;
            loop {
                while tasks.len() < 4 {
                    let Some(addon) = providers.next() else {
                        break;
                    };
                    let (client, kind, id, hints) =
                        (client.clone(), kind.clone(), id.clone(), hints.clone());
                    tasks.spawn(async move {
                        let result = tokio::time::timeout(
                            Duration::from_secs(15),
                            client.subtitles(&addon, &kind, &id, &hints),
                        )
                        .await;
                        (addon.manifest.name, result)
                    });
                }
                let Some(result) = tasks.join_next().await else {
                    break;
                };
                match result {
                    Ok((provider, Ok(Ok(subtitles)))) => {
                        let found = subtitles
                            .into_iter()
                            .map(|subtitle| ExternalSubtitle {
                                provider: provider.clone(),
                                subtitle,
                            })
                            .collect();
                        inbox
                            .send(Event::SubtitleSources(generation, found, false, false))
                            .await;
                    }
                    _ => failed = true,
                }
            }
            inbox
                .send(Event::SubtitleSources(generation, vec![], true, failed))
                .await;
        }));
    }

    pub fn subtitle_file(&mut self, generation: u64, request: u64, subtitle: Subtitle) {
        if let Some(task) = self.subtitle_file_task.take() {
            task.abort();
        }
        let (client, inbox, data) = (self.client.clone(), self.inbox.clone(), self.data.clone());
        self.subtitle_file_task = Some(self.runtime.spawn(async move {
            let result: Result<Arc<SubtitleFile>> = async {
                let bytes =
                    tokio::time::timeout(Duration::from_secs(20), client.subtitle_bytes(&subtitle))
                        .await??;
                tokio::task::spawn_blocking(move || {
                    let extension = subtitle_extension(&bytes)?;
                    let directory = data.join("subtitles");
                    std::fs::create_dir_all(&directory)?;
                    let file = Arc::new(SubtitleFile {
                        path: directory.join(format!("{}.{}", uuid::Uuid::new_v4(), extension)),
                    });
                    std::fs::write(&file.path, bytes.as_slice())?;
                    Ok(file)
                })
                .await?
            }
            .await;
            inbox
                .send(Event::SubtitleFile(
                    generation,
                    request,
                    result.map_err(|_| "Cannot load this subtitle; try another variant".into()),
                ))
                .await;
        }));
    }
}

fn subtitle_extension(bytes: &[u8]) -> Result<&'static str> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= 4 * 1024 * 1024,
        "Invalid subtitle size"
    );
    let prefix = String::from_utf8_lossy(bytes.get(..bytes.len().min(65536)).unwrap_or_default());
    if prefix.contains("[Script Info]") {
        return Ok("ass");
    }
    if prefix
        .trim_start_matches(['\u{feff}', '\r', '\n', ' '])
        .starts_with("WEBVTT")
    {
        return Ok("vtt");
    }
    ensure!(prefix.contains("-->"), "Unsupported subtitle data");
    Ok("srt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_subtitle_files_reject_playlists_and_keep_ass_styles() {
        assert_eq!(
            subtitle_extension(b"[Script Info]\n[Events]").ok(),
            Some("ass")
        );
        assert_eq!(
            subtitle_extension(b"WEBVTT\n\n00:00.000 --> 00:01.000\nHello").ok(),
            Some("vtt")
        );
        assert!(subtitle_extension(b"#EXTM3U\nhttps://private.example/movie").is_err());
        assert!(subtitle_extension(b"").is_err());
    }
}
