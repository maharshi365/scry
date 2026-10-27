//! Lifetime dashboard state and achievement notifications.
mod achievements;
mod dashboard;

use std::time::{Duration, Instant};

use rune::{El, Tx, row, text};
use scry_library::{AchievementEntry, AchievementEvaluation, NerdStats};

use super::{
    Page, State,
    design::{
        icons,
        tokens::{self, type_},
    },
    primitives::{button::Button, surface::card, text::label},
};
use crate::library_worker::Command;

const TOAST_DURATION: Duration = Duration::from_secs(6);

#[derive(Default)]
pub(super) struct StatsUi {
    generation: u64,
    pending: Option<u64>,
    data: Option<NerdStats>,
    error: Option<&'static str>,
    entries: Vec<AchievementEntry>,
    tab: StatsTab,
    historical_pending: usize,
    live_pending: Vec<AchievementEntry>,
    notice: Option<Notice>,
    notice_since: Option<Instant>,
    notice_sequence: u64,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum StatsTab {
    #[default]
    Stats,
    Achievements,
}

struct Notice {
    title: String,
    description: String,
    achievement: Option<&'static str>,
}

impl State {
    pub(super) fn refresh_stats(&mut self) {
        self.stats.generation += 1;
        let generation = self.stats.generation;
        self.stats.pending = Some(generation);
        self.stats.error = None;
        let result = self
            .library
            .worker
            .as_ref()
            .map(|worker| worker.send(Command::NerdStats { generation }));
        if !matches!(result, Some(Ok(()))) {
            self.stats_failed(generation);
        }
    }

    pub(in crate::ui) fn apply_stats(&mut self, generation: u64, data: NerdStats) {
        if self.stats.pending == Some(generation) {
            self.stats.data = Some(data);
            self.stats.pending = None;
            self.stats.error = None;
        }
    }

    pub(in crate::ui) fn stats_failed(&mut self, generation: u64) {
        if self.stats.pending == Some(generation) {
            self.stats.pending = None;
            self.stats.error = Some("Cannot load stats. Use Refresh to try again.");
        }
    }

    pub(in crate::ui) fn apply_achievements(&mut self, result: AchievementEvaluation) {
        for entry in result.newly_unlocked {
            if entry.historical {
                self.stats.historical_pending += 1;
            } else {
                self.stats.live_pending.push(entry);
            }
        }
        self.stats.entries = result.entries;
        if self.navigation.page == Page::Stats {
            self.refresh_stats();
        }
    }

    pub(super) fn poll_achievement_toast(&mut self, tx: &mut Tx) -> bool {
        if self.navigation.page == Page::Player && self.player.playing {
            if self.stats.notice_since.take().is_some() {
                // Do not let a departing toast linger over newly started video.
                tx.instant();
                return true;
            }
            return false;
        }
        let mut changed = false;
        if self.stats.notice.is_some() {
            let since = *self.stats.notice_since.get_or_insert_with(Instant::now);
            let remaining = TOAST_DURATION.saturating_sub(since.elapsed());
            if remaining.is_zero() {
                self.dismiss_achievement_toast();
                changed = true;
            } else {
                tx.wake_after(remaining);
            }
        }
        if self.stats.notice.is_none() {
            let notice = if self.stats.historical_pending > 0 {
                let count = std::mem::take(&mut self.stats.historical_pending);
                Some(Notice {
                    title: format!(
                        "{count} {} unlocked",
                        if count == 1 {
                            "achievement"
                        } else {
                            "achievements"
                        }
                    ),
                    description: "We checked your history. It’s worse than we thought.".into(),
                    achievement: None,
                })
            } else if !self.stats.live_pending.is_empty() {
                let entries = std::mem::take(&mut self.stats.live_pending);
                match entries.as_slice() {
                    [entry] => Some(Notice {
                        title: entry.name.into(),
                        description: entry.description.into(),
                        achievement: Some(entry.id),
                    }),
                    _ => Some(Notice {
                        title: format!("{} achievements unlocked", entries.len()),
                        description: "Multiple questionable accomplishments. One notification."
                            .into(),
                        achievement: None,
                    }),
                }
            } else {
                None
            };
            if let Some(notice) = notice {
                self.stats.notice_sequence += 1;
                self.stats.notice = Some(notice);
                self.stats.notice_since = Some(Instant::now());
                tx.wake_after(TOAST_DURATION);
                changed = true;
            }
        }
        changed
    }

    fn dismiss_achievement_toast(&mut self) {
        self.stats.notice = None;
        self.stats.notice_since = None;
    }
}

pub(super) fn toast(s: &State) -> Option<El> {
    if s.navigation.page == Page::Player && s.player.playing {
        return None;
    }
    let notice = s.stats.notice.as_ref()?;
    let glyph = notice
        .achievement
        .map_or(&icons::TROPHY, achievements::glyph);
    Some(
        card()
            .key(format!("achievement-toast-{}", s.stats.notice_sequence))
            .role("status")
            .label(notice.title.clone())
            .w(320.0)
            .padding(14.0)
            .gap(8.0)
            .pin(None, Some(24.0), Some(24.0), None)
            .border(1.0, tokens::stats_gold())
            .child(
                row()
                    .gap(10.0)
                    .center_y()
                    .child(rune::icon(glyph, 22.0).color(tokens::stats_gold()))
                    .child(
                        text(notice.title.clone())
                            .font(type_::ITEM_TITLE)
                            .color(tokens::theme().text)
                            .wrap(),
                    ),
            )
            .child(label(notice.description.clone()))
            .child(
                row()
                    .gap(6.0)
                    .child(
                        Button::new("achievement-view", "View achievements").on_press(
                            |s: &mut State| {
                                s.dismiss_achievement_toast();
                                s.stats.tab = StatsTab::Achievements;
                                s.navigate(Page::Stats);
                                s.navigation.reveal = Some("achievement-collection".into());
                            },
                        ),
                    )
                    .child(
                        Button::new("achievement-dismiss", "Dismiss")
                            .on_press(State::dismiss_achievement_toast),
                    ),
            )
            .transition(
                rune::Transition::move_edge(rune::Edge::Top)
                    .combined(rune::Transition::offset(0.0, -24.0)),
            ),
    )
}

pub(super) fn view(s: &State) -> El {
    dashboard::view(s)
}

pub(super) fn scene(s: &mut State) {
    s.config_busy = false;
    s.navigation.page = Page::Stats;
    s.stats.data = Some(NerdStats {
        started_at: 1_760_000_000,
        watch_seconds: 2_460.0 * 3600.0,
        movie_seconds: 740.0 * 3600.0,
        tv_seconds: 1_720.0 * 3600.0,
        movies_watched: 382,
        episodes_watched: 2_186,
        series_completed: 47,
        unfinished_shows: 18,
        recorded_completions: 2_640,
        recorded_seconds: 900.0 * 3600.0,
        rewatch_seconds: 108.0 * 3600.0,
        saved_titles: 200,
        saved_watched: 134,
        session_count: 320,
        longest_session_seconds: 8.5 * 3600.0,
        most_session_episodes: 11,
        top_titles: [
            ("The Office", 310.0),
            ("Breaking Bad", 144.0),
            ("Community", 98.0),
            ("Severance", 72.0),
            ("Dune: Part Two", 18.0),
        ]
        .into_iter()
        .map(|(title, hours)| scry_library::TopTitle {
            title: title.into(),
            seconds: hours * 3600.0,
        })
        .collect(),
        ..NerdStats::default()
    });
    if let Ok(mut db) = scry_library::Library::memory()
        && let Ok(result) = db.evaluate_achievements()
    {
        s.stats.entries = result.entries;
        for entry in &mut s.stats.entries {
            if matches!(
                entry.id,
                "pilot-error"
                    | "seasoned-professional"
                    | "closure"
                    | "a-day-well-spent"
                    | "centurion"
                    | "thousand-yard-stare"
                    | "comfort-zone"
                    | "one-more-episode"
            ) {
                entry.unlocked = true;
                entry.historical = true;
                entry.progress = scry_library::Evaluation::Unlocked;
            }
            if entry.id == "expert" {
                entry.progress = scry_library::Evaluation::Progress {
                    current: 2_460.0,
                    target: 10_000.0,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn refresh_preserves_dashboard_card_bounds() -> anyhow::Result<()> {
        let keys = [
            "stats-mix",
            "stats-mastery",
            "stats-top-titles",
            "stats-library-health",
            "stats-records",
        ];
        for width in [880.0, 1280.0] {
            let mut state = crate::ui::app::init();
            scene(&mut state);
            let mut rt = rune::Runtime::new(
                state,
                crate::ui::shell::view,
                (width, 820.0),
                rune::Env::default(),
            );
            rt.frame(0.0);
            let bounds = |rt: &rune::Runtime<State>| -> anyhow::Result<Vec<[f32; 4]>> {
                let elements = rt.elements();
                keys.iter()
                    .map(|key| {
                        elements
                            .iter()
                            .find(|element| element.key.as_deref() == Some(*key))
                            .map(|element| element.rect)
                            .with_context(|| format!("Missing {key}"))
                    })
                    .collect()
            };
            let before = bounds(&rt)?;
            rt.update(|state, _| {
                state.stats.pending = Some(1);
                state.apply_stats(1, NerdStats::default());
            });
            rt.frame(0.0);
            for (old, new) in before.iter().zip(bounds(&rt)?) {
                for (a, b) in old.iter().zip(new) {
                    assert!((a - b).abs() < 0.5, "Card moved after stats refresh");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn elapsed_toast_is_automatically_dismissed() -> anyhow::Result<()> {
        let mut state = crate::ui::app::init();
        state.stats.notice = Some(Notice {
            title: "Test".into(),
            description: "Test".into(),
            achievement: None,
        });
        state.stats.notice_since = Some(
            Instant::now()
                .checked_sub(TOAST_DURATION + Duration::from_secs(1))
                .context("test clock")?,
        );
        assert!(state.poll_achievement_toast(&mut Tx::default()));
        assert!(state.stats.notice.is_none());
        Ok(())
    }
}
