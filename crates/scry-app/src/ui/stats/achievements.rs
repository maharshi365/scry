use rune::{El, Icon, col, el, icon, row, text};
use scry_library::{AchievementEntry, Evaluation};

use super::{
    State,
    dashboard::{panel, rail, small},
};
use crate::ui::{
    design::{
        icons,
        tokens::{self, type_},
    },
    primitives::{
        button::{Button, Variant},
        surface::card,
    },
};

pub(super) fn glyph(id: &str) -> &'static Icon {
    match id {
        "pilot-error" => &icons::PLAY,
        "seasoned-professional" => &icons::LAYERS,
        "closure" => &icons::FLAG,
        "a-day-well-spent" => &icons::SUN,
        "centurion" => &icons::SHIELD,
        "thousand-yard-stare" => &icons::SCAN_EYE,
        "expert" => &icons::GRADUATION,
        "just-browsing" => &icons::LIBRARY,
        "comfort-zone" => &icons::REPEAT,
        "one-more-episode" => &icons::FORWARD,
        "night-shift" => &icons::MOON,
        "full-time-viewer" => &icons::BRIEFCASE,
        _ => &icons::TROPHY,
    }
}

fn progress(entry: &AchievementEntry) -> f64 {
    match entry.progress {
        Evaluation::Unlocked => 1.0,
        Evaluation::Progress { current, target } if target > 0.0 => {
            (current / target).clamp(0.0, 1.0)
        }
        _ => 0.0,
    }
}

fn medallion(entry: &AchievementEntry, size: f32) -> El {
    let hidden = entry.hidden && !entry.unlocked;
    el().key(format!("achievement-icon-{}", entry.id))
        .role("img")
        .label(if hidden {
            "Hidden achievement"
        } else {
            entry.name
        })
        .w(size)
        .h(size)
        .radius(size / 3.0)
        .fill(tokens::stats_badge_fill(entry.unlocked))
        .border(
            1.0,
            if entry.unlocked {
                tokens::stats_gold()
            } else {
                tokens::edge()
            },
        )
        .center()
        .child(
            icon(
                if hidden { &icons::EYE } else { glyph(entry.id) },
                size * 0.5,
            )
            .color(if entry.unlocked {
                tokens::stats_gold()
            } else {
                tokens::theme().text_secondary
            }),
        )
}

pub(super) fn view(s: &State) -> El {
    let earned = s
        .stats
        .entries
        .iter()
        .filter(|entry| entry.unlocked)
        .count();
    let mut section = col()
        .key("achievement-collection")
        .w_full()
        .gap(10.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(icon(&icons::TROPHY, 16.0).color(tokens::stats_gold()))
                .child(
                    text("Questionable accomplishments")
                        .font(type_::ITEM_TITLE)
                        .color(tokens::theme().text),
                )
                .child(small(format!("{earned}/{}", s.stats.entries.len())))
                .child(el().grow(1.0))
                .child(
                    Button::new(
                        "achievements-toggle",
                        if s.stats.collection_open {
                            "Hide collection"
                        } else {
                            "View all"
                        },
                    )
                    .variant(Variant::Ghost)
                    .on_press(|s: &mut State| s.stats.collection_open = !s.stats.collection_open)
                    .h(28.0),
                ),
        );
    let featured = s
        .stats
        .entries
        .iter()
        .filter(|entry| entry.unlocked)
        .max_by_key(|entry| entry.unlocked_at);
    let next = s
        .stats
        .entries
        .iter()
        .filter(|entry| !entry.unlocked && !entry.hidden)
        .max_by(|a, b| progress(a).total_cmp(&progress(b)));
    let mut highlights = row().w_full().gap(12.0).flex_wrap();
    if let Some(entry) = featured {
        highlights = highlights.child(featured_card(entry));
    }
    if let Some(entry) = next {
        highlights = highlights.child(next_card(entry));
    }
    section = section.child(highlights);
    if s.stats.collection_open {
        let mut grid = row().w_full().gap(8.0).flex_wrap();
        for entry in &s.stats.entries {
            grid = grid.child(tile(entry, s.stats.selected_achievement == Some(entry.id)));
        }
        section = section.child(grid);
        if let Some(entry) = s
            .stats
            .entries
            .iter()
            .find(|entry| Some(entry.id) == s.stats.selected_achievement)
        {
            let hidden = entry.hidden && !entry.unlocked;
            let date = if entry.unlocked && entry.historical {
                "Unlocked from watch history".into()
            } else if let Some(at) = entry.unlocked_at {
                chrono::DateTime::from_timestamp(at, 0).map_or_else(
                    || "Unlocked".into(),
                    |date| format!("Unlocked {}", date.format("%d %b %Y")),
                )
            } else {
                "Not unlocked yet".into()
            };
            section = section.child(
                card()
                    .key("achievement-detail")
                    .w_full()
                    .padding(12.0)
                    .gap(5.0)
                    .child(small(if hidden {
                        "You’ll know what you did."
                    } else {
                        entry.requirement
                    }))
                    .child(small(date)),
            );
        }
    }
    section
}

fn featured_card(entry: &AchievementEntry) -> El {
    panel("achievement-featured", "Badge of dishonor", &icons::TROPHY).child(
        row()
            .gap(12.0)
            .center_y()
            .child(medallion(entry, 42.0))
            .child(
                col()
                    .w(0.0)
                    .min_w(0.0)
                    .grow(1.0)
                    .gap(4.0)
                    .child(
                        text(entry.name)
                            .font(type_::ITEM_TITLE)
                            .color(tokens::theme().text),
                    )
                    .child(small(entry.description)),
            ),
    )
}

fn next_card(entry: &AchievementEntry) -> El {
    panel(
        "achievement-next",
        "Your next bad decision",
        &icons::HOURGLASS,
    )
    .child(
        row()
            .gap(10.0)
            .center_y()
            .child(medallion(entry, 32.0))
            .child(
                col()
                    .w(0.0)
                    .min_w(0.0)
                    .grow(1.0)
                    .gap(4.0)
                    .child(
                        text(entry.name)
                            .font(type_::ITEM_TITLE)
                            .color(tokens::theme().text),
                    )
                    .child(small(entry.description)),
            ),
    )
    .child(rail(
        "achievement-next-progress",
        progress(entry),
        tokens::stats_gold(),
    ))
}

fn tile(entry: &AchievementEntry, selected: bool) -> El {
    let hidden = entry.hidden && !entry.unlocked;
    let status = if entry.unlocked {
        "Unlocked".into()
    } else if hidden {
        "Hidden".into()
    } else {
        format!("{:.0}%", progress(entry) * 100.0)
    };
    let id = entry.id;
    Button::new(
        format!("achievement-{id}"),
        if hidden {
            "Hidden achievement"
        } else {
            entry.name
        },
    )
    .selected(selected)
    .frame()
    .w(184.0)
    .min_w(170.0)
    .h(84.0)
    .padding(10.0)
    .gap(8.0)
    .radius(10.0)
    .justify_start()
    .child(medallion(entry, 30.0))
    .child(
        col()
            .w(0.0)
            .min_w(0.0)
            .grow(1.0)
            .gap(5.0)
            .child(
                text(if hidden { "???" } else { entry.name })
                    .font(type_::ITEM_TITLE)
                    .color(tokens::theme().text)
                    .wrap(),
            )
            .child(small(status)),
    )
    .on_press(move |s: &mut State| {
        s.stats.selected_achievement = if s.stats.selected_achievement == Some(id) {
            None
        } else {
            Some(id)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_current_achievement_has_a_distinct_glyph() -> anyhow::Result<()> {
        let mut db = scry_library::Library::memory()?;
        let entries = db.evaluate_achievements()?.entries;
        let names: std::collections::HashSet<_> =
            entries.iter().map(|entry| glyph(entry.id).name).collect();
        assert_eq!(names.len(), entries.len());
        assert!(
            !names.contains("trophy"),
            "Current achievements must not use the generic fallback"
        );
        Ok(())
    }
}
