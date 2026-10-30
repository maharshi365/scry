//! Bounded provider grace and first-frame failover for the tuned channel only.
use super::{Page, State};
use crate::ui::source_ranking::{is_torrent, select_sources};
use scry_addons::Stream;
use std::time::{Duration, Instant};

const PROVIDER_GRACE: Duration = Duration::from_secs(1);
const JOIN_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_ATTEMPTS: u8 = 3;

pub(super) struct Pending {
    pub deadline: Instant,
    sources: Vec<Stream>,
}

#[derive(Default)]
pub(super) struct Tuning {
    pub pending: Option<Pending>,
    pub joining: Option<Instant>,
    pub attempts: u8,
}

impl State {
    pub(in crate::ui) fn apply_guide_streams(
        &mut self,
        streams: Vec<(String, Stream)>,
        done: bool,
    ) {
        if !self.guide.resolving || self.guide.tuned.is_none() {
            return;
        }
        let sources = select_sources(streams, &self.guide.data.policy);
        if !done {
            let Some(first) = sources.first() else {
                return;
            };
            if is_torrent(first) {
                let deadline = self.guide.tuning.pending.as_ref().map_or_else(
                    || Instant::now() + PROVIDER_GRACE,
                    |pending| pending.deadline,
                );
                self.guide.tuning.pending = Some(Pending { deadline, sources });
                return;
            }
        }
        self.start_guide_sources(sources);
    }

    fn start_guide_sources(&mut self, sources: Vec<Stream>) {
        self.guide.resolving = false;
        self.guide.tuning.pending = None;
        self.guide.tuning.attempts = 0;
        if let Some(services) = &mut self.runtime.services {
            services.cancel_streams();
        }
        self.guide.alternatives = sources
            .into_iter()
            .take(usize::from(MAX_ATTEMPTS))
            .collect();
        self.play_guide_alternative();
    }

    pub(super) fn guide_join_started(&mut self) {
        self.guide.tuning.attempts = self.guide.tuning.attempts.saturating_add(1);
        self.guide.tuning.joining = self.player.playing.then(|| Instant::now() + JOIN_TIMEOUT);
    }

    pub(in crate::ui) fn guide_join_failed(&mut self) {
        if self.navigation.page != Page::Guide || self.guide.tuning.joining.take().is_none() {
            return;
        }
        if self.guide.tuning.attempts < MAX_ATTEMPTS && !self.guide.alternatives.is_empty() {
            self.play_guide_alternative();
        } else {
            self.stop();
            self.guide.error = Some("Channel could not start after trying the available sources. Retry or change provider.".into());
        }
    }

    pub(super) fn poll_guide_tuning(&mut self, now: Instant, tx: &mut rune::Tx) -> bool {
        let mut changed = false;
        if let Some(pending) = &self.guide.tuning.pending {
            if now >= pending.deadline {
                if let Some(pending) = self.guide.tuning.pending.take() {
                    self.start_guide_sources(pending.sources);
                }
                changed = true;
            } else {
                tx.wake_after(pending.deadline.saturating_duration_since(now));
            }
        }
        if let Some(deadline) = self.guide.tuning.joining {
            if self.player.ready
                && self.player.sample.position_s.is_some()
                && !self.player.sample.buffering
            {
                self.guide.tuning.joining = None;
            } else if now >= deadline {
                self.guide_join_failed();
                changed = true;
            } else {
                tx.wake_after(deadline.saturating_duration_since(now));
            }
        }
        changed
    }
}
