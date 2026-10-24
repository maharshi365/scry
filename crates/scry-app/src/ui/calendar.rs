//! Release calendar. Each date owns a separate scroll viewport.
use super::{
    State,
    components::{action, button, heading, label, panel, scroll},
};
use chrono::{Datelike, Duration, Local, NaiveDate};
use rune::{
    El, col, row, text,
    tok::{self, TextStyle},
};
use scry_library::Media;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Mode {
    Week,
    Month,
}

#[derive(Clone)]
pub(super) struct Release {
    pub date: NaiveDate,
    pub media: Media,
    pub episode_id: String,
    pub title: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub poster: Option<String>,
}

pub(super) struct CalendarUi {
    pub mode: Mode,
    pub anchor: NaiveDate,
    pub offset: u32,
    pub series: Vec<Media>,
    pub releases: Vec<Release>,
    pub generation: u64,
    pub loading: bool,
    pub failed: usize,
    pub artwork_rects: std::collections::BTreeMap<String, [f32; 4]>,
    pub day_viewports: std::collections::BTreeMap<NaiveDate, [f32; 4]>,
    pub viewport: Option<[f32; 4]>,
}

impl Default for CalendarUi {
    fn default() -> Self {
        Self {
            mode: Mode::Week,
            anchor: Local::now().date_naive(),
            offset: 0,
            series: Vec::new(),
            releases: Vec::new(),
            generation: 0,
            loading: false,
            failed: 0,
            artwork_rects: std::collections::BTreeMap::new(),
            day_viewports: std::collections::BTreeMap::new(),
            viewport: None,
        }
    }
}

pub(super) fn dates(anchor: NaiveDate, mode: Mode) -> Vec<NaiveDate> {
    let first = if mode == Mode::Month {
        anchor.with_day(1).unwrap_or(anchor)
    } else {
        anchor
    };
    let start = first - Duration::days(i64::from(first.weekday().num_days_from_monday()));
    let count = if mode == Mode::Week { 7 } else { 42 };
    (0..count).map(|i| start + Duration::days(i)).collect()
}

pub(super) fn release_date(value: &str) -> Option<NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|d| d.with_timezone(&Local).date_naive())
        .or_else(|| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
}

impl State {
    pub(super) fn fetch_calendar(&mut self) {
        self.calendar.generation += 1;
        self.calendar.loading = !self.calendar.series.is_empty();
        self.calendar.failed = 0;
        if self.calendar.series.is_empty() {
            self.calendar.releases.clear();
        }
        if let Some(services) = &mut self.services {
            services.calendar(
                self.calendar.generation,
                self.addons.clone(),
                self.calendar.series.clone(),
            );
        }
    }

    pub(super) fn apply_calendar(
        &mut self,
        generation: u64,
        found: Vec<(Media, scry_addons::Meta)>,
        mut failed: usize,
    ) {
        if generation != self.calendar.generation {
            return;
        }
        let mut releases = Vec::new();
        for (media, meta) in found {
            if meta.id != media.key.id || meta.kind != "series" {
                failed += 1;
                continue;
            }
            for video in meta.videos {
                let Some(date) = video.released.as_deref().and_then(release_date) else {
                    continue;
                };
                releases.push(Release {
                    date,
                    media: media.clone(),
                    episode_id: video.id,
                    title: video.title,
                    season: video.season,
                    episode: video.episode,
                    poster: meta.poster.clone(),
                });
            }
        }
        releases.sort_by(|a, b| {
            (a.date, &a.media.title, a.season, a.episode, &a.episode_id).cmp(&(
                b.date,
                &b.media.title,
                b.season,
                b.episode,
                &b.episode_id,
            ))
        });
        releases.dedup_by(|a, b| a.media.key == b.media.key && a.episode_id == b.episode_id);
        self.calendar.releases = releases;
        self.calendar.artwork_rects.clear();
        self.calendar.day_viewports.clear();
        self.calendar.failed = failed;
        self.calendar.loading = false;
    }

    fn move_calendar(&mut self, direction: i32) {
        let c = &mut self.calendar;
        c.artwork_rects.clear();
        c.day_viewports.clear();
        if c.mode == Mode::Week {
            c.anchor += Duration::days(i64::from(direction) * 7);
        } else if let Some(date) = if direction < 0 {
            c.anchor.checked_sub_months(chrono::Months::new(1))
        } else {
            c.anchor.checked_add_months(chrono::Months::new(1))
        } {
            c.anchor = date;
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Calendar grid geometry and per-day scroll bindings are kept together"
)]
pub(super) fn view(s: &State, cx: &rune::Cx<State>) -> El {
    let c = &s.calendar;
    let month = c.mode == Mode::Month;
    let day_height = if month {
        ((cx.layout("content").map_or(752.0, |r| r[3]) - 175.0) / 6.0).max(82.0)
    } else {
        430.0
    };
    let mut body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(12.0)
        .child(
            row()
                .w_full()
                .min_w(0.0)
                .gap(8.0)
                .center_y()
                .child(heading("Calendar").grow(1.0))
                .child(action(
                    "calendar-week",
                    if c.mode == Mode::Week {
                        "Week ✓"
                    } else {
                        "Week"
                    },
                    |s| s.calendar.mode = Mode::Week,
                ))
                .child(action(
                    "calendar-month",
                    if c.mode == Mode::Month {
                        "Month ✓"
                    } else {
                        "Month"
                    },
                    |s| s.calendar.mode = Mode::Month,
                ))
                .child(action(
                    "calendar-refresh",
                    "Refresh",
                    State::refresh_library,
                )),
        )
        .child(
            row()
                .w_full()
                .min_w(0.0)
                .gap(8.0)
                .center_y()
                .child(action("calendar-prev", "←", |s| s.move_calendar(-1)))
                .child(action("calendar-today", "Today", |s| {
                    s.calendar.anchor = Local::now().date_naive();
                }))
                .child(action("calendar-next", "→", |s| s.move_calendar(1)))
                .child(
                    label(c.anchor.format("%B %Y").to_string())
                        .w(0.0)
                        .grow(1.0)
                        .min_w(0.0)
                        .truncate(),
                )
                .child(series_pages(s)),
        );
    if c.loading {
        body = body.child(label("Loading episode release dates…"));
    }
    if c.failed > 0 {
        body = body.child(label(format!(
            "{} series could not load. Refresh to retry.",
            c.failed
        )));
    }
    if c.series.is_empty() {
        body = body.child(label(
            "No library series. Add a series to To watch to see its releases.",
        ));
    }
    let dates = dates(c.anchor, c.mode);
    let mut weeks = col().w_full().min_w(0.0).gap(8.0);
    for week in dates.chunks(7) {
        let mut cells = col().w_full().min_w(0.0).equal_columns(7).gap(8.0);
        for date in week {
            let entries: Vec<_> = c.releases.iter().filter(|r| r.date == *date).collect();
            let mut list = col().w_full().gap(5.0);
            if entries.is_empty() {
                list = list.child(label("—"));
            }
            for (i, release) in entries.iter().enumerate() {
                let item = (**release).clone();
                let menu = Media {
                    key: super::library_view::key(
                        &release.media.key.namespace,
                        scry_library::MediaKind::Episode,
                        &release.episode_id,
                    ),
                    title: format!("{} · {}", release.media.title, release.title),
                    parent: Some(release.media.key.clone()),
                };
                let mut copy = col()
                    .w(0.0)
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(2.0)
                    .child(
                        text(release.media.title.clone())
                            .w_full()
                            .truncate()
                            .font(TextStyle::new(12.0, 550.0))
                            .color(tok::theme(true).text),
                    )
                    .child(
                        text(format!(
                            "S{} E{}",
                            release.season.unwrap_or(0),
                            release.episode.unwrap_or(0)
                        ))
                        .w_full()
                        .truncate()
                        .font(TextStyle::new(10.0, 500.0))
                        .color(super::components::accent()),
                    );
                if !release.title.trim().is_empty() {
                    copy = copy.child(
                        text(release.title.clone())
                            .w_full()
                            .truncate()
                            .font(TextStyle::new(10.0, 400.0))
                            .color(tok::theme(true).text_secondary),
                    );
                }
                let entry_key = format!("calendar-episode-{date}-{i}");
                let layout_key = entry_key.clone();
                let thumbnail = super::artwork::poster_thumbnail(
                    s,
                    &format!("calendar-poster-{date}-{i}"),
                    release.poster.as_deref(),
                    &release.media.title,
                )
                .w(if month { 28.0 } else { 40.0 })
                .h(if month { 42.0 } else { 60.0 });
                list = list.child(
                    button(
                        entry_key,
                        format!(
                            "{} S{} E{} · {}",
                            release.media.title,
                            release.season.unwrap_or(0),
                            release.episode.unwrap_or(0),
                            release.title
                        ),
                        move |s| {
                            let media = Media {
                                key: super::library_view::key(
                                    &item.media.key.namespace,
                                    scry_library::MediaKind::Episode,
                                    &item.episode_id,
                                ),
                                title: item.media.title.clone(),
                                parent: Some(item.media.key.clone()),
                            };
                            s.open_library_media(&media);
                        },
                    )
                    .h(if month { 54.0 } else { 88.0 })
                    .w_full()
                    .min_w(0.0)
                    .px(6.0)
                    .gap(7.0)
                    .justify_start()
                    .child(thumbnail)
                    .child(copy)
                    .on_layout(move |s: &mut State, rect| {
                        s.calendar.artwork_rects.insert(layout_key.clone(), rect);
                        s.refresh_artwork();
                    })
                    .on_secondary_press(move |s: &mut State, p, _| {
                        s.context_menu = Some((p, Some(menu.clone())));
                    }),
                );
            }
            let today = *date == Local::now().date_naive();
            let day = *date;
            cells = cells.child(
                col()
                    .key(format!("calendar-day-{date}"))
                    .w_full()
                    .min_w(0.0)
                    .h(day_height)
                    .padding(if month { 4.0 } else { 6.0 })
                    .gap(if month { 4.0 } else { 5.0 })
                    .radius(7.0)
                    .fill(panel())
                    .border(
                        1.0,
                        if today {
                            super::components::accent()
                        } else {
                            super::components::edge()
                        },
                    )
                    .child(
                        label(format!(
                            "{} · {}{}",
                            date.format("%a %d"),
                            entries.len(),
                            if entries.len() > 1 { " ↕" } else { "" }
                        ))
                        .font(rune::tok::TextStyle::new(
                            if month { 10.0 } else { 11.0 },
                            400.0,
                        )),
                    )
                    .child(
                        scroll(format!("calendar-day-scroll-{date}"), list).on_layout(
                            move |s: &mut State, rect| {
                                s.calendar.day_viewports.insert(day, rect);
                                s.refresh_artwork();
                            },
                        ),
                    ),
            );
        }
        weeks = weeks.child(cells);
    }
    body.child(
        scroll(format!("calendar-grid-{:?}", c.mode), weeks).on_layout(|s: &mut State, rect| {
            s.calendar.viewport = Some(rect);
            s.refresh_artwork();
        }),
    )
}

fn series_pages(s: &State) -> El {
    let c = &s.calendar;
    let mut controls = row().gap(5.0).center_y();
    if c.offset == 0 && c.series.len() < 20 {
        return controls;
    }
    let caption = if c.series.is_empty() {
        "No more series".into()
    } else {
        format!(
            "Series {}–{}",
            c.offset.saturating_add(1),
            c.offset
                .saturating_add(u32::try_from(c.series.len()).unwrap_or(20))
        )
    };
    controls = controls.child(label(caption).font(TextStyle::new(11.0, 400.0)));
    if c.offset > 0 {
        controls = controls.child(
            action("calendar-series-prev", "‹", |s| {
                s.calendar.offset = s.calendar.offset.saturating_sub(20);
                s.refresh_library();
            })
            .w(28.0)
            .h(28.0)
            .label("Previous 20 library series"),
        );
    }
    if c.series.len() == 20 {
        controls = controls.child(
            action("calendar-series-next", "›", |s| {
                s.calendar.offset = s.calendar.offset.saturating_add(20);
                s.refresh_library();
            })
            .w(28.0)
            .h(28.0)
            .label("Next 20 library series"),
        );
    }
    controls
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Invalid calendar fixtures must fail the test"
)]
mod tests {
    use super::*;
    #[test]
    fn month_grid_covers_leap_day_and_has_monday_columns() {
        let anchor = NaiveDate::from_ymd_opt(2024, 2, 15).unwrap();
        let days = dates(anchor, Mode::Month);
        assert_eq!(days.len(), 42);
        assert_eq!(days[0].weekday(), chrono::Weekday::Mon);
        assert!(days.iter().any(|d| d.month() == 2 && d.day() == 29));
        assert!(days.iter().any(|d| d.month() == 3));
    }
    #[test]
    fn week_wraps_year_and_invalid_release_dates_are_omitted() {
        let anchor = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap();
        let days = dates(anchor, Mode::Week);
        assert_eq!(days.len(), 7);
        assert_eq!(days[0].year(), 2026);
        assert_eq!(days[6].year(), 2027);
        assert!(release_date("not a date").is_none());
        assert!(release_date("2026-02-30").is_none());
    }

    #[test]
    fn calendar_omits_undated_episodes_and_rejects_stale_or_wrong_series_results() {
        let mut s = super::super::app::init();
        s.calendar.generation = 2;
        let media = Media {
            key: scry_library::MediaKey::new("test", scry_library::MediaKind::Series, "show"),
            title: "Show".into(),
            parent: None,
        };
        let meta: scry_addons::Meta = serde_json::from_value(serde_json::json!({
            "id":"show","type":"series","name":"Show","videos":[
                {"id":"late","season":1,"episode":2,"released":"2026-10-24"},
                {"id":"early","season":1,"episode":1,"released":"2026-10-23"},
                {"id":"unknown"}
            ]
        }))
        .unwrap();
        s.apply_calendar(1, vec![(media.clone(), meta.clone())], 0);
        assert!(s.calendar.releases.is_empty());
        s.apply_calendar(2, vec![(media.clone(), meta.clone())], 0);
        assert_eq!(
            s.calendar
                .releases
                .iter()
                .map(|r| r.episode_id.as_str())
                .collect::<Vec<_>>(),
            vec!["early", "late"]
        );
        let mut wrong = meta;
        wrong.id = "another-show".into();
        s.apply_calendar(2, vec![(media, wrong)], 0);
        assert!(s.calendar.releases.is_empty());
        assert_eq!(s.calendar.failed, 1);
    }
}
