use rune::{El, Icon, col, el, icon, row, text};
use scry_library::{AchievementEntry, Evaluation};

use super::{
    State,
    dashboard::{rail, small},
};
use crate::ui::design::{
    icons,
    tokens::{self, type_},
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
        "double-feature" => &icons::MOVIE,
        "channel-surfer" => &icons::TV,
        "standing-appointment" => &icons::CALENDAR,
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

fn medallion(entry: &AchievementEntry) -> El {
    let hidden = entry.hidden && !entry.unlocked;
    crate::ui::primitives::status::medallion(
        format!("achievement-icon-{}", entry.id),
        if hidden {
            "Hidden achievement"
        } else {
            entry.name
        },
        if hidden { &icons::EYE } else { glyph(entry.id) },
        if entry.unlocked {
            crate::ui::primitives::status::MedallionVariant::Earned
        } else {
            crate::ui::primitives::status::MedallionVariant::Locked
        },
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
                        .font(type_::TITLE)
                        .color(tokens::theme().text),
                )
                .child(small(format!(
                    "{earned}/{} unlocked",
                    s.stats.entries.len()
                ))),
        );
    let mut list = col().role("list").label("Achievements").w_full();
    for entry in &s.stats.entries {
        list = list.child(achievement_row(entry));
    }
    section = section.child(list);
    section
}

fn achievement_row(entry: &AchievementEntry) -> El {
    let hidden = entry.hidden && !entry.unlocked;
    let id = entry.id;
    let name = if hidden {
        "Hidden achievement"
    } else {
        entry.name
    };
    let status = if entry.unlocked && entry.historical {
        "Unlocked from watch history".into()
    } else if entry.unlocked {
        entry
            .unlocked_at
            .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
            .map_or_else(
                || "Unlocked".into(),
                |date| format!("Unlocked {}", date.format("%d %b %Y")),
            )
    } else {
        "Locked".into()
    };
    let mut details = col().w_full().min_w(0.0).gap(5.0).child(
        row()
            .w_full()
            .gap(12.0)
            .flex_wrap()
            .child(
                text(name)
                    .font(type_::TITLE)
                    .color(if entry.unlocked {
                        tokens::theme().text
                    } else {
                        tokens::theme().text_secondary
                    })
                    .wrap(),
            )
            .child(el().grow(1.0))
            .child(small(status).key(format!("achievement-status-{id}"))),
    );
    if hidden {
        details = details.child(small("You’ll know what you did."));
    } else {
        details = details
            .child(small(entry.description))
            .child(small(entry.requirement).key(format!("achievement-requirement-{id}")));
        if !entry.unlocked
            && let Evaluation::Progress { current, target } = entry.progress
            && target > 0.0
        {
            details = details
                .child(small(format!(
                    "{current:.0} / {target:.0} · {:.0}%",
                    progress(entry) * 100.0
                )))
                .child(rail(
                    &format!("achievement-progress-{id}"),
                    progress(entry),
                    tokens::accent(),
                ));
        }
    }
    col()
        .key(format!("achievement-{id}"))
        .role("listitem")
        .label(name)
        .w_full()
        .child(
            row()
                .w_full()
                .py(16.0)
                .gap(16.0)
                .child(medallion(entry))
                .child(col().w(0.0).min_w(0.0).grow(1.0).child(details)),
        )
        .child(crate::ui::primitives::surface::divider())
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
