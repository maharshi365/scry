use rune::{Color, El, col, el, icon, row, text};
use scry_library::NerdStats;

use super::{State, StatsTab, achievements};
use crate::ui::{
    design::{
        icons,
        tokens::{self, type_},
    },
    primitives::{
        button::tab as tab_button,
        pattern::detail_header,
        surface::{card, scroll},
        text::label,
    },
};

pub(super) fn view(s: &State) -> El {
    let mut content = col()
        .key("nerd-stats-content")
        .w_full()
        .gap(12.0)
        .child(detail_header(
            "stats-header",
            "Stats for Nerds",
            "Playback totals, library health, and achievements",
            row(),
        ));
    content = content.child(
        row()
            .key("stats-tabs")
            .self_start()
            .gap(4.0)
            .role("tablist")
            .label("Stats for Nerds")
            .child(tab(
                "stats-tab-stats",
                "Stats",
                s.stats.tab == StatsTab::Stats,
                StatsTab::Stats,
            ))
            .child(tab(
                "stats-tab-achievements",
                "Achievements",
                s.stats.tab == StatsTab::Achievements,
                StatsTab::Achievements,
            )),
    );
    if s.stats.tab == StatsTab::Achievements {
        return scroll("nerd-stats-scroll", content.child(achievements::view(s)));
    }
    if let Some(error) = s.stats.error {
        content = content.child(label(error));
    }
    if s.stats.pending.is_some() && s.stats.data.is_none() {
        content = content.child(label("Gathering the evidence…"));
    }
    if let Some(stats) = &s.stats.data {
        content = content
            .child(hero(s, stats))
            .child(
                el().equal_columns(2)
                    .w_full()
                    .gap(12.0)
                    .child(mix(stats))
                    .child(mastery(stats.watch_seconds)),
            )
            .child(
                el().equal_columns(2)
                    .w_full()
                    .gap(12.0)
                    .child(rankings(stats).self_stretch())
                    .child(
                        col()
                            .key("stats-records-column")
                            .self_stretch()
                            .min_w(0.0)
                            .gap(12.0)
                            .child(library_health(stats).grow(1.0))
                            .child(records(stats)),
                    ),
            );
    } else if s.stats.pending.is_none() && s.stats.error.is_none() {
        content = content.child(small(
            "Your questionable accomplishments will appear when the local library is ready.",
        ));
    }
    scroll("nerd-stats-scroll", content)
}

pub(super) fn small(value: impl Into<String>) -> El {
    crate::ui::primitives::text::Variant::Caption.render(value)
}

fn tab(key: &str, title: &str, selected: bool, target: StatsTab) -> El {
    tab_button(key, title, selected, move |s: &mut State| {
        s.stats.tab = target;
    })
}

pub(super) fn panel(key: &str, title: &str, glyph: &'static rune::Icon) -> El {
    card()
        .key(key)
        .min_w(0.0)
        .padding(tokens::layout::PAGE_GAP)
        .gap(8.0)
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(icon(glyph, 16.0).color(tokens::accent()))
                .child(
                    text(title)
                        .font(type_::TITLE)
                        .color(tokens::theme().text)
                        .truncate()
                        .w(0.0)
                        .min_w(0.0)
                        .grow(1.0),
                ),
        )
}

fn hero(s: &State, stats: &NerdStats) -> El {
    crate::ui::primitives::surface::Variant::Hero
        .apply(col())
        .key("stats-watch-time")
        .w_full()
        .padding(16.0)
        .gap(6.0)
        .child(
            text("TOTAL WATCH TIME")
                .font(type_::SECTION)
                .color(tokens::accent()),
        )
        .child(
            row()
                .w_full()
                .gap(14.0)
                .center_y()
                .flex_wrap()
                .child(
                    row()
                        .gap(8.0)
                        .center_y()
                        .child(
                            text(format!("{:.0}", stats.watch_seconds / 3600.0))
                                .font(type_::STATS_HERO)
                                .color(tokens::theme().text),
                        )
                        .child(label("hours watched")),
                )
                .child(el().grow(1.0))
                .child(
                    col()
                        .gap(2.0)
                        .child(
                            text(format!("{:.1} days", stats.watch_seconds / 86_400.0))
                                .font(type_::STATS_NUMBER)
                                .color(tokens::accent()),
                        )
                        .child(small("The couch considers you family.")),
                ),
        )
        .child(small(if stats.watch_seconds > 0.0 {
            "A remarkable commitment to other people’s problems."
        } else {
            "A suspiciously clean record. Enjoy it while it lasts."
        }))
        .child(crate::ui::primitives::surface::divider().mt(10.0).mb(8.0))
        .child(summary(s, stats))
}

fn summary(s: &State, stats: &NerdStats) -> El {
    let earned = s
        .stats
        .entries
        .iter()
        .filter(|entry| entry.unlocked)
        .count();
    let mut values = row().key("stats-summary").w_full().gap(10.0);
    for (key, title, value) in [
        ("stats-movies", "Movies", stats.movies_watched.to_string()),
        (
            "stats-episodes",
            "Episodes",
            stats.episodes_watched.to_string(),
        ),
        (
            "stats-series",
            "Shows finished",
            stats.series_completed.to_string(),
        ),
        (
            "stats-earned",
            "Achievements",
            format!("{earned}/{}", s.stats.entries.len()),
        ),
    ] {
        values = values.child(
            col()
                .key(key)
                .w(0.0)
                .min_w(0.0)
                .grow(1.0)
                .gap(2.0)
                .child(
                    text(value)
                        .font(type_::STATS_NUMBER)
                        .color(tokens::theme().text),
                )
                .child(small(title)),
        );
    }
    values
}

fn mix(stats: &NerdStats) -> El {
    let total = stats.tv_seconds + stats.movie_seconds;
    let tv = if total > 0.0 {
        stats.tv_seconds / total
    } else {
        0.0
    };
    let personality = if total <= 0.0 {
        "Your type? Still loading."
    } else if tv >= 0.7 {
        "Serial commitment"
    } else if tv <= 0.3 {
        "Feature-length attention span"
    } else {
        "Equal-opportunity escapist"
    };
    panel("stats-mix", "Your viewing DNA", &icons::TV)
        .min_h(160.0)
        .child(
            row()
                .w_full()
                .gap(10.0)
                .center_y()
                .child(
                    text(format!("{:.0}%", tv * 100.0))
                        .font(type_::STATS_NUMBER)
                        .color(tokens::accent()),
                )
                .child(small(if total > 0.0 {
                    "of your watch time is television"
                } else {
                    "No known watch time yet"
                })),
        )
        .child(split_bar("stats-mix-bar", tv, total > 0.0))
        .child(
            row()
                .w_full()
                .gap(10.0)
                .child(legend(&icons::TV, "TV", stats.tv_seconds, tokens::accent()))
                .child(legend(
                    &icons::MOVIE,
                    "Movies",
                    stats.movie_seconds,
                    tokens::stats_violet(),
                )),
        )
        .child(small(personality))
}

fn legend(glyph: &'static rune::Icon, name: &str, seconds: f64, color: Color) -> El {
    row()
        .grow(1.0)
        .gap(5.0)
        .center_y()
        .child(icon(glyph, 12.0).color(color))
        .child(small(format!("{name} · {}", hours(seconds))))
}

fn mastery(seconds: f64) -> El {
    let hours = seconds / 3600.0;
    let skills = (hours / 10_000.0).floor();
    let progress = (hours % 10_000.0) / 10_000.0;
    let headline = if skills < 1.0 {
        format!("{:.1}% of a skill", progress * 100.0)
    } else {
        format!(
            "{skills:.0} {} mastered",
            if skills < 2.0 { "skill" } else { "skills" }
        )
    };
    panel(
        "stats-mastery",
        "You could have done this instead",
        &icons::GRADUATION,
    )
    .min_h(160.0)
    .child(
        text(headline)
            .font(type_::STATS_NUMBER)
            .color(tokens::stats_gold()),
    )
    .child(rail(
        "stats-mastery-progress",
        progress,
        tokens::stats_gold(),
    ))
    .child(small(format!(
        "Or {:.0} eight-hour workdays. No benefits.",
        hours / 8.0
    )))
    .child(small("Instead, you memorized the dialogue."))
}

fn rankings(stats: &NerdStats) -> El {
    let mut content = panel("stats-top-titles", "Your usual suspects", &icons::CHART);
    let largest = stats.top_titles.first().map_or(0.0, |title| title.seconds);
    if largest <= 0.0 {
        return content.child(small("Watch something. We’ll establish a pattern."));
    }
    for (index, title) in stats.top_titles.iter().take(5).enumerate() {
        content = content.child(
            row()
                .w_full()
                .gap(10.0)
                .center_y()
                .child(
                    text(format!("{:02}", index + 1))
                        .font(type_::SMALL)
                        .color(tokens::accent())
                        .w(18.0),
                )
                .child(
                    col()
                        .w(0.0)
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(5.0)
                        .child(
                            row()
                                .w_full()
                                .gap(8.0)
                                .child(
                                    text(title.title.clone())
                                        .font(type_::TITLE)
                                        .color(tokens::theme().text)
                                        .truncate()
                                        .w(0.0)
                                        .min_w(0.0)
                                        .grow(1.0),
                                )
                                .child(small(hours(title.seconds))),
                        )
                        .child(rail(
                            &format!("stats-rank-{index}"),
                            title.seconds / largest,
                            if index == 0 {
                                tokens::accent()
                            } else {
                                tokens::stats_violet()
                            },
                        )),
                ),
        );
    }
    content.child(small("Your emotional support television, ranked."))
}

fn library_health(stats: &NerdStats) -> El {
    let fraction = count_share(stats.saved_watched, stats.saved_titles);
    let mut content = panel(
        "stats-library-health",
        "Unfinished business",
        &icons::LIBRARY,
    )
    .w_full()
    .child(
        row()
            .w_full()
            .gap(8.0)
            .center_y()
            .child(
                text(format!("{:.0}%", fraction * 100.0))
                    .font(type_::STATS_NUMBER)
                    .color(tokens::theme().text),
            )
            .child(small("of your saved library watched")),
    )
    .child(rail("stats-library-progress", fraction, tokens::accent()));
    content = content.child(small(if stats.saved_titles == 0 {
        "No library yet. No promises broken.".into()
    } else {
        format!(
            "{} watched · {} still waiting",
            stats.saved_watched,
            stats.saved_titles.saturating_sub(stats.saved_watched)
        )
    }));
    content
}

fn records(stats: &NerdStats) -> El {
    let replay = if stats.recorded_seconds > 0.0 {
        stats.rewatch_seconds / stats.recorded_seconds
    } else {
        0.0
    };
    card()
        .key("stats-records")
        .w_full()
        .padding(14.0)
        .gap(8.0)
        .child(
            row()
                .w_full()
                .gap(12.0)
                .child(
                    col()
                        .grow(1.0)
                        .gap(3.0)
                        .child(
                            row()
                                .gap(5.0)
                                .center_y()
                                .child(icon(&icons::CLOCK, 13.0).color(tokens::stats_gold()))
                                .child(small("Longest session")),
                        )
                        .child(
                            text(if stats.session_count > 0 {
                                hours(stats.longest_session_seconds)
                            } else {
                                "—".into()
                            })
                            .font(type_::TITLE)
                            .color(tokens::theme().text),
                        ),
                )
                .child(
                    col()
                        .grow(1.0)
                        .gap(3.0)
                        .child(
                            row()
                                .gap(5.0)
                                .center_y()
                                .child(icon(&icons::REPEAT, 13.0).color(tokens::stats_violet()))
                                .child(small("Rewatch habit")),
                        )
                        .child(
                            text(if stats.recorded_seconds > 0.0 {
                                format!("{:.0}%", replay * 100.0)
                            } else {
                                "—".into()
                            })
                            .font(type_::TITLE)
                            .color(tokens::theme().text),
                        ),
                ),
        )
        .child(small("Comfort zone? You’ve furnished it."))
}

pub(super) fn hours(seconds: f64) -> String {
    if seconds > 0.0 && seconds < 3600.0 {
        format!("{:.0}m", (seconds / 60.0).max(1.0))
    } else if (3600.0..36_000.0).contains(&seconds) {
        format!("{:.1}h", seconds / 3600.0)
    } else {
        format!("{:.0}h", seconds / 3600.0)
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "Display-only ratios may round at huge counts; displayed integer totals stay exact"
)]
fn count_share(part: i64, total: i64) -> f64 {
    if total > 0 {
        part as f64 / total as f64
    } else {
        0.0
    }
}

pub(super) fn rail(key: &str, value: f64, color: Color) -> El {
    crate::ui::primitives::progress::indicator(
        key,
        value,
        crate::ui::primitives::progress::Variant::Compact,
        color,
    )
}

fn split_bar(key: &str, tv: f64, has_data: bool) -> El {
    crate::ui::primitives::progress::split(
        key,
        tv,
        [tokens::accent(), tokens::stats_violet()],
        has_data,
        format!(
            "Television {:.0}%, movies {:.0}%",
            tv * 100.0,
            if has_data { (1.0 - tv) * 100.0 } else { 0.0 }
        ),
    )
}
