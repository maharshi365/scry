//! Captured playback progress and the shutdown write barrier.
use crate::{
    library_worker::{Command, Event as DbEvent},
    ui::State,
};
use scry_library::MediaKind;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

impl State {
    pub(in crate::ui) fn observe_library_progress(&mut self) {
        if let (Some(position), Some(duration)) =
            (self.player.sample.position_s, self.player.sample.duration_s)
            && position.is_finite()
            && duration.is_finite()
            && duration > 0.0
        {
            self.library.last_sample = Some((position.max(0.0).min(duration), duration));
        }
        if self.library.last_save.elapsed() >= Duration::from_secs(12) {
            self.save_library_progress();
        }
    }

    pub(in crate::ui) fn save_library_progress(&mut self) {
        let Some(media) = self.library.playing.clone() else {
            return;
        };
        // A parent series has no playback duration; only the selected episode does.
        if media.key.kind == MediaKind::Series {
            return;
        }
        let Some((position, duration)) = self.library.last_sample else {
            return;
        };
        if self.library.saved_sample == Some((position, duration)) {
            return;
        }
        if self.db(Command::SaveProgress(media, position, duration), false) {
            self.library.saved_sample = Some((position, duration));
            self.library.last_save = Instant::now();
        }
    }

    pub(in crate::ui) fn finish_library_progress(&mut self, successful: bool) {
        if successful
            && self.player.playing
            && self.library.playing.as_ref().is_some_and(|media| {
                matches!(media.key.kind, MediaKind::Movie | MediaKind::Episode)
            })
            && let Some((_, duration)) = self.library.last_sample
            && duration.is_finite()
            && duration > 0.0
        {
            // EOF may clear time-pos before the last metrics event. Keep telemetry
            // unchanged; only the captured library progress records completion.
            self.library.last_sample = Some((duration, duration));
        }
        self.save_library_progress();
    }

    pub(in crate::ui) fn take_library_shutdown(&mut self) -> impl FnOnce() + Send + 'static {
        let worker = self.library.worker.take();
        let final_save = self
            .library
            .playing
            .clone()
            .filter(|m| m.key.kind != MediaKind::Series)
            .zip(self.library.last_sample)
            .map(|(m, (p, d))| Command::SaveProgress(m, p, d));
        move || {
            let Some(worker) = worker else {
                return;
            };
            // This runs on the shutdown thread, while Rune keeps pumping native
            // messages. Drain completions to release worker backpressure.
            let mut commands = VecDeque::new();
            if let Some(command) = final_save {
                commands.push_back(command);
            }
            commands.push_back(Command::Flush(1));
            while let Some(command) = commands.front() {
                let retry = match command {
                    Command::SaveProgress(m, p, d) => Command::SaveProgress(m.clone(), *p, *d),
                    _ => Command::Flush(1),
                };
                if worker.send(retry).is_ok() {
                    commands.pop_front();
                } else if matches!(
                    worker.try_recv(),
                    Err(std::sync::mpsc::TryRecvError::Disconnected)
                ) {
                    return;
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            loop {
                match worker.try_recv() {
                    Ok(DbEvent::Flushed(1)) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        return;
                    }
                    Ok(_) => {}
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        }
    }
}
