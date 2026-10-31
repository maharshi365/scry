use crate::{
    library_worker::{Command, Event},
    ui::{Page, State},
};
use scry_addons::Stream;
use scry_library::{Media, Progress};

pub(in crate::ui) struct ResumeRequest {
    generation: u64,
    media: Media,
    progress: Progress,
    stream: StreamLookup,
}

enum StreamLookup {
    Pending,
    Ready(Option<Box<Stream>>),
}

pub(in crate::ui) struct StreamCandidate {
    pub generation: u64,
    pub media: Media,
    pub stream: Stream,
}

impl State {
    pub(in crate::ui) fn resume_library_media(&mut self, media: &Media) {
        let progress = self
            .library
            .watching
            .iter()
            .find(|(item, _)| item.key == media.key)
            .map(|(_, progress)| *progress);
        self.open_library_media(media);
        let Some(progress) = progress else {
            return;
        };
        let generation = self.detail.generation;
        self.library.resume_request = Some(ResumeRequest {
            generation,
            media: media.clone(),
            progress,
            stream: StreamLookup::Pending,
        });
        if let Some(worker) = &self.library.worker {
            if worker
                .send(Command::SavedStream {
                    generation,
                    key: media.key.clone(),
                })
                .is_err()
            {
                self.library.resume_request = None;
            }
        } else {
            self.library.resume_request = None;
        }
    }

    pub(super) fn apply_saved_stream(&mut self, event: Event) {
        let Event::SavedStream { generation, stream } = event else {
            return;
        };
        if let Some(request) = &mut self.library.resume_request
            && request.generation == generation
        {
            request.stream = StreamLookup::Ready(
                stream
                    .and_then(|json| serde_json::from_str::<Stream>(&json).ok())
                    .filter(|stream| stream.validate_playback().is_ok())
                    .map(Box::new),
            );
        }
        self.try_resume_stream();
    }

    pub(in crate::ui) fn try_resume_stream(&mut self) {
        let Some(request) = &self.library.resume_request else {
            return;
        };
        if self.navigation.page != Page::Detail || request.generation != self.detail.generation {
            self.library.resume_request = None;
            return;
        }
        if self.detail.request.loading || matches!(request.stream, StreamLookup::Pending) {
            return;
        }
        let Some(request) = self.library.resume_request.take() else {
            return;
        };
        let StreamLookup::Ready(Some(stream)) = request.stream else {
            return;
        };
        self.detail.episodes.selected = request
            .media
            .parent
            .as_ref()
            .map(|_| request.media.key.id.clone());
        self.runtime.streams.invalidate();
        if let Some(services) = &mut self.runtime.services {
            services.cancel_streams();
        }
        self.detail.sources.request.loading = false;
        let generation = self.player.generation;
        self.play(*stream, request.media.title.clone());
        if let Some(candidate) = &mut self.library.stream_candidate {
            candidate.media.clone_from(&request.media);
        }
        if self.player.generation != generation {
            self.library.playing = Some(request.media.clone());
            self.library.resume_position = Some(request.progress.position_seconds);
            self.library.replaying = Some(request.media);
        }
    }
}

#[cfg(test)]
#[path = "resume_tests.rs"]
mod tests;
