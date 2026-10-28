//! Release calendar. Each date owns a separate scroll viewport.
use super::{
    State,
    controls::{action, button, icon_action},
    design::{
        icons,
        tokens::{accent, calendar, theme, type_},
    },
    primitives::{
        button::{Button, Size, Variant},
        surface::scroll,
        text::{heading, label},
    },
};
use chrono::{Datelike, Duration, Local, NaiveDate};
use rune::{El, col, el, row, text};
use scry_library::{Media, MediaKind};

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
    pub anchor: NaiveDate,
    pub series: Vec<Media>,
    pub releases: Vec<Release>,
    pub release_times: std::collections::HashMap<(scry_library::MediaKey, String), i64>,
    pub generation: u64,
    pub loading: bool,
    pub failed: usize,
    pub artwork_rects: std::collections::BTreeMap<String, [f32; 4]>,
    pub day_viewports: std::collections::BTreeMap<NaiveDate, [f32; 4]>,
    pub viewport: Option<[f32; 4]>,
    pub expanded_day: Option<NaiveDate>,
    pub dialog_artwork_rects: std::collections::BTreeMap<(NaiveDate, usize), [f32; 4]>,
    pub dialog_viewport: Option<[f32; 4]>,
}

impl Default for CalendarUi {
    fn default() -> Self {
        Self {
            anchor: Local::now().date_naive(),
            series: Vec::new(),
            releases: Vec::new(),
            release_times: std::collections::HashMap::new(),
            generation: 0,
            loading: false,
            failed: 0,
            artwork_rects: std::collections::BTreeMap::new(),
            day_viewports: std::collections::BTreeMap::new(),
            viewport: None,
            expanded_day: None,
            dialog_artwork_rects: std::collections::BTreeMap::new(),
            dialog_viewport: None,
        }
    }
}

pub(super) fn dates(anchor: NaiveDate) -> Vec<NaiveDate> {
    let first = anchor.with_day(1).unwrap_or(anchor);
    let start = first - Duration::days(i64::from(first.weekday().num_days_from_monday()));
    (0..42).map(|i| start + Duration::days(i)).collect()
}

pub(super) fn release_date(value: &str) -> Option<NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|d| d.with_timezone(&Local).date_naive())
        .or_else(|| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
}

impl State {
    pub(super) fn refresh_calendar(&mut self) {
        if self.calendar.loading {
            return;
        }
        self.calendar.loading = true;
        self.refresh_library();
    }

    pub(super) fn fetch_calendar(&mut self) {
        self.calendar.generation += 1;
        self.calendar.loading = !self.calendar.series.is_empty();
        self.calendar.failed = 0;
        if self.calendar.series.is_empty() {
            self.calendar.releases.clear();
            self.calendar.release_times.clear();
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
        let mut release_times = std::collections::HashMap::new();
        for (media, meta) in found {
            let kind = if media.key.kind == MediaKind::Movie {
                "movie"
            } else {
                "series"
            };
            if meta.id != media.key.id || meta.kind != kind {
                failed += 1;
                continue;
            }
            if kind == "movie" {
                if let Some(date) = crate::media_metadata::meta_release_timestamp(&meta) {
                    release_times.insert((media.key.clone(), media.key.id.clone()), date);
                }
                if let Some(date) = meta.released.as_deref().and_then(release_date) {
                    releases.push(Release {
                        date,
                        episode_id: media.key.id.clone(),
                        title: String::new(),
                        season: None,
                        episode: None,
                        poster: meta.poster.clone(),
                        media,
                    });
                }
                continue;
            }
            let meta = meta.into_data();
            for video in meta.videos {
                if let Some(date) = video
                    .released
                    .as_deref()
                    .and_then(crate::media_metadata::release_timestamp)
                {
                    release_times.insert((media.key.clone(), video.id.clone()), date);
                }
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
        self.calendar.release_times = release_times;
        self.calendar.releases = releases;
        self.calendar.artwork_rects.clear();
        self.calendar.dialog_artwork_rects.clear();
        self.calendar.day_viewports.clear();
        self.calendar.failed = failed;
        self.calendar.loading = false;
    }

    fn move_calendar(&mut self, direction: i32) {
        let c = &mut self.calendar;
        c.artwork_rects.clear();
        c.day_viewports.clear();
        if let Some(date) = if direction < 0 {
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
    let day_height = ((cx.layout("calendar-grid").map_or(0.0, |r| r[3]) - calendar::GAP * 5.0)
        / 6.0)
        .max(calendar::MIN_DAY_HEIGHT);
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
                .child(heading("Calendar").grow(1.0)),
        )
        .child(
            super::primitives::input_group::input_group()
                .w_full()
                .gap(8.0)
                .child(icon_action(
                    "calendar-prev",
                    "Previous calendar period",
                    &icons::LEFT,
                    |s| s.move_calendar(-1),
                ))
                .child(action("calendar-today", "Today", |s| {
                    s.calendar.anchor = Local::now().date_naive();
                }))
                .child(icon_action(
                    "calendar-next",
                    "Next calendar period",
                    &icons::RIGHT,
                    |s| s.move_calendar(1),
                ))
                .child(
                    label(c.anchor.format("%B %Y").to_string())
                        .w(0.0)
                        .grow(1.0)
                        .min_w(0.0)
                        .truncate(),
                ),
        );
    if c.failed > 0 {
        body = body.child(label(format!(
            "{} titles could not load. Refresh to retry.",
            c.failed
        )));
    }
    let dates = dates(c.anchor);
    let mut weeks = col().w_full().min_w(0.0).gap(calendar::GAP);
    for (week_index, week) in dates.chunks(7).enumerate() {
        let mut cells = col()
            .w_full()
            .min_w(0.0)
            .equal_columns(7)
            .gap(calendar::GAP);
        for (column, date) in week.iter().enumerate() {
            let slot = format!("calendar-slot-{}", week_index * 7 + column);
            if date.month() != c.anchor.month() {
                cells = cells.child(el().key(slot).w_full().h(day_height));
                continue;
            }
            let entries: Vec<_> = c.releases.iter().filter(|r| r.date == *date).collect();
            let mut list = col().w_full().gap(5.0);
            for (i, release) in entries.iter().enumerate() {
                let item = (**release).clone();
                let menu = release_media(release);
                let mut copy = col()
                    .w(0.0)
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(2.0)
                    .child(
                        text(release.media.title.clone())
                            .w_full()
                            .truncate()
                            .font(type_::CALENDAR_TITLE)
                            .color(theme().text),
                    )
                    .child(
                        text(release_caption(release))
                            .w_full()
                            .truncate()
                            .font(type_::SMALL_EMPHASIS)
                            .color(accent()),
                    );
                if !release.title.trim().is_empty() {
                    copy = copy.child(
                        text(release.title.clone())
                            .w_full()
                            .truncate()
                            .font(type_::SMALL)
                            .color(theme().text_secondary),
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
                .w(28.0)
                .h(42.0);
                list = list.child(
                    button(
                        entry_key,
                        format!(
                            "{} · {} · {}",
                            release.media.title,
                            release_caption(release),
                            release.title
                        ),
                        move |s| {
                            let media = release_media(&item);
                            s.open_library_media(&media);
                        },
                    )
                    .h(54.0)
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
                        s.open_title_menu(p, Some(menu.clone()));
                    }),
                );
            }
            let today = *date == Local::now().date_naive();
            let day = *date;
            let caption = if entries.is_empty() {
                date.format("%a %d").to_string()
            } else {
                format!(
                    "{} · {}{}",
                    date.format("%a %d"),
                    entries.len(),
                    if entries.len() > 1 { " ↕" } else { "" }
                )
            };
            let mut header = row().w_full().center_y().child(
                label(caption)
                    .font(type_::SMALL)
                    .w(0.0)
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            );
            if !entries.is_empty() {
                header = header.child(
                    Button::new(format!("calendar-expand-{date}"), "Expand day")
                        .variant(Variant::Ghost)
                        .size(Size::Compact)
                        .icon(&icons::EXPAND)
                        .on_press(move |s: &mut State| {
                            s.calendar.expanded_day = Some(day);
                        }),
                );
            }
            cells = cells.child(
                el().key(slot).w_full().min_w(0.0).child(
                    super::primitives::surface::Variant::Calendar
                        .apply_selected(col(), today)
                        .key(format!("calendar-day-{date}"))
                        .w_full()
                        .min_w(0.0)
                        .h(day_height)
                        .padding(4.0)
                        .gap(4.0)
                        .child(header)
                        .child(
                            scroll(format!("calendar-day-scroll-{date}"), list).on_layout(
                                move |s: &mut State, rect| {
                                    s.calendar.day_viewports.insert(day, rect);
                                    s.refresh_artwork();
                                },
                            ),
                        ),
                ),
            );
        }
        weeks = weeks.child(cells);
    }
    body.child(
        scroll("calendar-grid", weeks).on_layout(|s: &mut State, rect| {
            s.calendar.viewport = Some(rect);
            s.refresh_artwork();
        }),
    )
}

fn release_caption(release: &Release) -> String {
    if release.media.key.kind == MediaKind::Movie {
        "Movie".into()
    } else {
        format!(
            "{} · {}",
            super::episodes::season_label(release.season),
            release
                .episode
                .map_or_else(|| "Episode".into(), |n| format!("Episode {n}"))
        )
    }
}

fn release_media(release: &Release) -> Media {
    if release.media.key.kind == MediaKind::Movie {
        return release.media.clone();
    }
    Media {
        key: super::library_view::key(
            &release.media.key.namespace,
            MediaKind::Episode,
            &release.episode_id,
        ),
        title: format!("{} · {}", release.media.title, release.title),
        parent: Some(release.media.key.clone()),
    }
}

pub(super) fn day_dialog(s: &State, cx: &rune::Cx<State>) -> El {
    let Some(date) = s.calendar.expanded_day else {
        return el();
    };
    let mut list = col().w_full().gap(8.0);
    let entries: Vec<_> = s
        .calendar
        .releases
        .iter()
        .filter(|r| r.date == date)
        .collect();
    for (index, release) in entries.into_iter().enumerate() {
        let media = release_media(release);
        list = list.child(
            button(
                format!("calendar-dialog-release-{index}"),
                format!(
                    "{} · {} · {}",
                    release.media.title,
                    release_caption(release),
                    release.title
                ),
                move |s| {
                    s.calendar.expanded_day = None;
                    s.open_library_media(&media);
                },
            )
            .w_full()
            .h(100.0)
            .px(12.0)
            .gap(16.0)
            .justify_start()
            .on_layout(move |s: &mut State, rect| {
                s.calendar.dialog_artwork_rects.insert((date, index), rect);
                s.refresh_artwork();
            })
            .child(
                super::artwork::poster_thumbnail(
                    s,
                    &format!("calendar-dialog-poster-{index}"),
                    release.poster.as_deref(),
                    &release.media.title,
                )
                .w(48.0)
                .h(72.0),
            )
            .child(
                col()
                    .w(0.0)
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(6.0)
                    .child(
                        text(release.media.title.clone())
                            .font(type_::CALENDAR_TITLE)
                            .color(theme().text)
                            .truncate(),
                    )
                    .child(
                        label(format!(
                            "{}{}{}",
                            release_caption(release),
                            if release.title.is_empty() { "" } else { " · " },
                            release.title
                        ))
                        .truncate(),
                    ),
            ),
        );
    }
    let size = cx.layout("scry").unwrap_or([0.0, 0.0, 1280.0, 820.0]);
    super::primitives::dialog::layer(
        "calendar-dialog-layer",
        super::primitives::dialog::panel("calendar-day-dialog", |s: &mut State| {
            s.calendar.expanded_day = None;
        })
        .w((size[2] - 64.0).clamp(320.0, 840.0))
        .h((size[3] - 80.0).max(240.0))
        .padding(24.0)
        .gap(16.0)
        .child(day_dialog_header(date))
        .child(
            scroll("calendar-dialog-scroll", list).on_layout(|s: &mut State, rect| {
                s.calendar.dialog_viewport = Some(rect);
                s.refresh_artwork();
            }),
        ),
    )
}

fn day_dialog_header(date: NaiveDate) -> El {
    super::primitives::dialog::header(date.format("%A, %B %-d").to_string()).child(
        super::primitives::dialog::close("calendar-dialog-close", "Close day", |s: &mut State| {
            s.calendar.expanded_day = None;
        }),
    )
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Invalid calendar fixtures must fail the test"
)]
mod tests {
    use super::*;
    #[test]
    fn movies_use_exact_release_dates_and_preserve_movie_identity() {
        let mut s = super::super::app::init();
        let media = Media {
            key: scry_library::MediaKey::new("test", MediaKind::Movie, "movie"),
            title: "Movie".into(),
            parent: None,
        };
        let meta: scry_addons::Meta = serde_json::from_value(serde_json::json!({
            "id":"movie", "type":"movie", "name":"Movie", "released":"2026-10-24", "releaseInfo":"2026"
        })).unwrap();
        s.apply_calendar(0, vec![(media.clone(), meta.clone())], 0);
        assert_eq!(s.calendar.releases.len(), 1);
        assert_eq!(s.calendar.releases[0].date.to_string(), "2026-10-24");
        assert_eq!(release_media(&s.calendar.releases[0]), media);
        assert_eq!(release_caption(&s.calendar.releases[0]), "Movie");
        let mut undated = meta.clone();
        undated.released = None;
        s.apply_calendar(0, vec![(media.clone(), undated)], 0);
        assert!(
            s.calendar.releases.is_empty(),
            "a release year is not an exact date"
        );
        let mut wrong = meta;
        wrong.kind = "series".into();
        s.apply_calendar(0, vec![(media, wrong)], 0);
        assert!(s.calendar.releases.is_empty());
        assert_eq!(s.calendar.failed, 1);
    }

    #[test]
    fn month_grid_covers_leap_day_and_has_monday_columns() {
        let anchor = NaiveDate::from_ymd_opt(2024, 2, 15).unwrap();
        let days = dates(anchor);
        assert_eq!(days.len(), 42);
        assert_eq!(days[0].weekday(), chrono::Weekday::Mon);
        assert!(days.iter().any(|d| d.month() == 2 && d.day() == 29));
        assert!(days.iter().any(|d| d.month() == 3));
    }
    #[test]
    fn month_wraps_year_and_invalid_release_dates_are_omitted() {
        let anchor = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap();
        let days = dates(anchor);
        assert_eq!(days.len(), 42);
        assert_eq!(days[0].year(), 2026);
        assert_eq!(days[41].year(), 2027);
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
