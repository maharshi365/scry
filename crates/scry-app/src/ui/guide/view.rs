use super::super::{
    State,
    design::tokens::{self, type_},
    primitives::{
        button::Button,
        progress::bar,
        surface::scroll,
        text::{heading, label},
    },
};
use rune::{Cx, El, col, el, row, text};
use scry_app::platform::VideoHost;
use scry_library::{
    MediaKind,
    guide::{Channel, Slot},
};

const WINDOW: i64 = 7200;

pub(super) fn time(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0).map_or_else(
        || "—".into(),
        |time| {
            time.with_timezone(&chrono::Local)
                .format("%H:%M")
                .to_string()
        },
    )
}

pub(super) fn title(slot: &Slot) -> String {
    if let (Some(season), Some(episode)) = (slot.program.season, slot.program.episode) {
        format!("{} · S{season:02} E{episode:02}", slot.program.show_title)
    } else {
        slot.program.show_title.clone()
    }
}

pub(in crate::ui) fn view(s: &State, cx: &Cx<State>) -> El {
    if s.player.fullscreen {
        return preview(s, cx.env().scale, true);
    }
    if s.guide.settings {
        return super::settings::view(s);
    }
    let now = chrono::Utc::now().timestamp();
    let mut content = col().grow(1.0).min_h(0.0).gap(12.0).child(header(s)).child(
        row()
            .h(236.0)
            .min_h(236.0)
            .gap(16.0)
            .child(details(s, now))
            .child(preview(s, cx.env().scale, false).w(340.0)),
    );
    if let Some(error) = &s.guide.error {
        content = content.child(label(error.clone()).color(tokens::danger()));
    }
    content.child(grid(s,cx,now)).child(label("↑ / ↓ browse · Enter tune · F fullscreen · M mute · schedules continue while Scry is closed"))
}

fn details(s: &State, now: i64) -> El {
    let selected = s
        .guide
        .data
        .channels
        .iter()
        .find(|channel| Some(&channel.id) == s.guide.selected.as_ref());
    let mut details = col().grow(1.0).w(0.0).min_w(0.0).gap(8.0).child(heading(
        selected.map_or("Guide", |channel| channel.name.as_str()),
    ));
    if let Some(slot) = selected.and_then(|channel| channel.current(now)) {
        details = details
            .child(label(title(slot)))
            .child(row().h(6.0).w_full().child(bar(
                "guide-program-progress",
                f64::from(slot.elapsed_seconds(now)) / f64::from(slot.program.duration),
            )))
            .child(label(format!(
                "{} – {}{} · {} min into program",
                time(slot.starts_at),
                time(slot.ends_at()),
                if slot.program.estimated {
                    " · approximate"
                } else {
                    ""
                },
                now.saturating_sub(slot.starts_at) / 60
            )))
            .child(label(super::super::formatting::excerpt(
                &slot.program.description,
                190,
            )));
        let id = selected
            .map(|channel| channel.id.clone())
            .unwrap_or_default();
        let media = slot.program.media.clone();
        details = details.child(
            row()
                .flex_wrap()
                .gap(8.0)
                .child(
                    Button::new("guide-tune", "Tune in")
                        .on_press(move |s: &mut State| s.tune_guide(id.clone())),
                )
                .child(
                    Button::new("guide-regular", "Open in regular view").on_press(
                        move |s: &mut State| {
                            s.leave_guide();
                            s.stop();
                            s.open_library_media(&media);
                        },
                    ),
                ),
        );
    } else {
        details = details.child(label(
            "Add shows to a collection or save titles in your library to build channels.",
        ));
    }
    details
}

fn header(s: &State) -> El {
    let mut header = row().center_y().gap(8.0).child(heading("Guide").grow(1.0));
    for (key, name, movie) in [("guide-tv", "TV", false), ("guide-movies", "Movies", true)] {
        header = header.child(
            Button::new(key, name)
                .selected((s.guide.kind == MediaKind::Movie) == movie)
                .on_press(move |s: &mut State| {
                    s.select_guide_kind(if movie {
                        MediaKind::Movie
                    } else {
                        MediaKind::Series
                    });
                }),
        );
    }
    header = header.child(
        Button::new("guide-settings", "Settings").on_press(|s: &mut State| {
            s.guide.settings = true;
            s.host.hide();
        }),
    );
    header
}

fn grid(s: &State, cx: &Cx<State>, now: i64) -> El {
    let width = cx
        .layout("guide-grid")
        .map_or(660.0, |rect| (rect[2] - 160.0).max(200.0));
    let start = now - now.rem_euclid(1800);
    let mut rows = col().w_full().gap(4.0).child(time_header(start, width));
    let mut count = 0;
    for channel in s
        .guide
        .data
        .channels
        .iter()
        .filter(|c| c.kind == s.guide.kind)
    {
        count += 1;
        let id = channel.id.clone();
        let channel_name = channel.name.clone();
        let mut programs = row().w(width).min_w(0.0).h(64.0).gap(0.0);
        let mut cursor = start;
        for slot in channel
            .slots
            .iter()
            .filter(|slot| slot.ends_at() > start && slot.starts_at < start + WINDOW)
        {
            let left = slot.starts_at.max(start);
            if left > cursor {
                programs = programs.child(el().w(cell_width(left - cursor, width)));
            }
            let seconds = slot.ends_at().min(start + WINDOW) - left;
            let cell = program_cell(s, channel, slot, now, cell_width(seconds, width));
            programs = programs.child(cell);
            cursor = slot.ends_at().min(start + WINDOW);
        }
        if channel.slots.iter().all(|slot| !slot.contains(now)) {
            programs = programs.child(label("No eligible programs"));
        }
        rows = rows.child(
            row()
                .h(64.0)
                .child(
                    Button::new(
                        format!("guide-channel-{}", channel.id),
                        channel_name.clone(),
                    )
                    .selected(Some(&channel.id) == s.guide.selected.as_ref())
                    .frame()
                    .w(160.0)
                    .h(64.0)
                    .child(
                        text(channel_name)
                            .truncate()
                            .font(type_::LABEL)
                            .color(tokens::theme().text),
                    )
                    .on_press(move |s: &mut State| s.tune_guide(id.clone())),
                )
                .child(programs),
        );
    }
    if count == 0 {
        rows = rows.child(label(
            "No channels yet. Open Channels & collections to create your lineup.",
        ));
    }
    let marker = 160.0 + cell_width(now - start, width);
    scroll(
        "guide-grid",
        rows.child(
            el().key("guide-now-marker")
                .absolute(marker, 0.0)
                .w(2.0)
                .h_full()
                .fill(tokens::accent()),
        ),
    )
}

fn program_cell(s: &State, channel: &Channel, slot: &Slot, now: i64, width: f32) -> El {
    let current = slot.contains(now);
    let mut cell = Button::new(
        format!("guide-program-{}-{}", channel.id, slot.starts_at),
        format!(
            "{} · {}",
            if current { "Watch now" } else { "Upcoming" },
            title(slot)
        ),
    )
    .enabled(current)
    .selected(
        current
            && s.guide
                .tuned
                .as_ref()
                .is_some_and(|(id, _)| id == &channel.id),
    )
    .frame()
    .w(width)
    .h(64.0)
    .min_w(0.0)
    .padding(8.0)
    .child(
        col()
            .min_w(0.0)
            .grow(1.0)
            .gap(4.0)
            .child(
                text(title(slot))
                    .truncate()
                    .font(type_::LABEL)
                    .color(tokens::theme().text),
            )
            .child(
                label(format!(
                    "{}{}",
                    time(slot.starts_at),
                    if slot.program.estimated { " ≈" } else { "" }
                ))
                .truncate(),
            ),
    );
    if current {
        let id = channel.id.clone();
        cell = cell.on_press(move |s: &mut State| s.tune_guide(id.clone()));
    }
    cell
}

fn time_header(start: i64, width: f32) -> El {
    let mut times = row().h(30.0).child(label("CHANNEL").w(160.0));
    for offset in [0, 1800, 3600, 5400] {
        times = times.child(label(time(start + offset)).w(width / 4.0));
    }
    times
}

fn cell_width(seconds: i64, width: f32) -> f32 {
    f32::from(u16::try_from(seconds.clamp(0, WINDOW)).unwrap_or_default()) / 7200.0 * width
}

fn preview(s: &State, scale: f32, fullscreen: bool) -> El {
    let mut video = el()
        .key("guide-video")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(tokens::video());
    if s.player.playing {
        video = video.on_layout(move |s: &mut State, rect| {
            if s.navigation.page == super::super::Page::Guide
                && !s.guide.settings
                && s.player.playing
            {
                s.host.place(rect, scale);
                if s.player.ready {
                    s.host.show();
                }
            }
        });
    }
    if !s.player.ready {
        if let Some(image) = s
            .guide_artwork()
            .first()
            .and_then(|url| s.artwork.as_ref()?.get(url))
        {
            video = video.child(
                el().absolute(0.0, 0.0)
                    .w_full()
                    .h_full()
                    .pixels_fit(&image, rune::SurfaceFit::Fit),
            );
        }
        video = video.child(label(preview_status(s)).padding(12.0));
    }
    let mut buttons = row().gap(6.0).center_y();
    for (key, name, direction) in [
        ("guide-channel-down", "CH −", -1),
        ("guide-channel-up", "CH +", 1),
    ] {
        buttons = buttons.child(
            Button::new(key, name).on_press(move |s: &mut State| s.guide_step(direction, true)),
        );
    }
    buttons = buttons.child(
        Button::new(
            "guide-fullscreen",
            if fullscreen {
                "Exit fullscreen"
            } else {
                "Fullscreen"
            },
        )
        .enabled(s.player.playing || fullscreen)
        .on_press(|s: &mut State| s.set_fullscreen(!s.player.fullscreen)),
    );
    if fullscreen {
        buttons = buttons.child(Button::new("guide-mute", "Mute / unmute").on_press(
            |s: &mut State| {
                s.player_action(super::super::player::actions::PlayerAction::ToggleMute);
            },
        ));
    }
    if let Some((id, _)) = &s.guide.tuned {
        let id = id.clone();
        buttons = buttons.child(
            Button::new("guide-retry", "Retry")
                .on_press(move |s: &mut State| s.tune_guide(id.clone())),
        );
    }
    let mut preview = col()
        .key("guide-preview")
        .min_h(0.0)
        .gap(6.0)
        .child(video)
        .child(buttons);
    preview = preview.children(torrent_retry(s));
    if fullscreen && let Some(error) = &s.guide.error {
        preview = preview.child(label(error.clone()));
    }
    if !s.guide.alternatives.is_empty() && !s.player.playing {
        preview = preview.child(
            Button::new("guide-alternative", "Try another source")
                .on_press(State::play_guide_alternative),
        );
    }
    if fullscreen {
        preview.w_full().h_full()
    } else {
        preview
    }
}

fn preview_status(s: &State) -> &'static str {
    if s.guide.error.is_some() || s.error.is_some() {
        "Channel unavailable · retry or choose another channel"
    } else if s.guide.resolving {
        "Finding a source…"
    } else if s.player.playing {
        "Joining channel…"
    } else if s.guide.tuned.is_some() {
        "Channel break · next program starts on schedule"
    } else if s.guide.selected.is_none() {
        "Add titles to your library or a collection to build channels"
    } else {
        "Starting the first available channel…"
    }
}

fn torrent_retry(s: &State) -> Option<El> {
    (s.guide.error.is_some() && !s.guide.data.policy.allow_torrents).then(|| {
        col()
            .gap(6.0)
            .child(label("Torrents can upload data and expose your IP."))
            .child(
                Button::new("guide-enable-torrents", "Allow torrents & retry")
                    .enabled(!s.guide.busy)
                    .on_press(|s: &mut State| {
                        let mut policy = s.guide.data.policy.clone();
                        policy.allow_torrents = true;
                        s.update_guide_policy(policy);
                    }),
            )
    })
}
