use super::{Event, Services, metadata};
use crate::history_import::{self, Preview};
use scry_addons::Addon;
use std::{path::PathBuf, sync::Arc};

impl Services {
    pub(crate) fn preview_history(&mut self, generation: u64, path: PathBuf, addons: Vec<Addon>) {
        self.cancel_history_preview();
        let inbox = self.inbox.clone();
        let client = self.client.clone();
        self.history_task = Some(self.runtime.spawn(async move {
            let loaded = tokio::task::spawn_blocking(move || history_import::read(&path)).await;
            let result: Result<Preview, String> = match loaded {
                Ok(Ok((items, mut preview))) => {
                    let addons = Arc::new(addons);
                    let total = items.len();
                    inbox
                        .send(Event::HistoryProgress(generation, 0, total))
                        .await;
                    for (index, item) in items.into_iter().enumerate() {
                        let meta = if history_import::needs_metadata(&item) {
                            metadata::fetch(
                                &client,
                                &addons,
                                "com.linvo.cinemeta",
                                &item.kind,
                                &item.id,
                                |meta| {
                                    (meta.id == item.id && meta.kind == item.kind).then_some(meta)
                                },
                            )
                            .await
                        } else {
                            None
                        };
                        history_import::append(&mut preview, &item, meta.as_ref());
                        inbox
                            .send(Event::HistoryProgress(generation, index + 1, total))
                            .await;
                    }
                    Ok(preview)
                }
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err("Cannot read the selected history file".into()),
            };
            inbox.send(Event::HistoryPreview(generation, result)).await;
        }));
    }

    pub(crate) fn cancel_history_preview(&mut self) {
        if let Some(task) = self.history_task.take() {
            task.abort();
        }
    }
}
