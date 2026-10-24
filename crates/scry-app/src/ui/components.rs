//! Shared controls, colors, and display formatting.
use super::{State, app::on_input_key};
use rune::tok::{self, TextStyle, type_};
use rune::{El, ScrollSpec, TextEdit, Tx, col, el, row, text, text_input};

pub(super) fn accent() -> rune::Color {
    rune::oklch(0.79, 0.12, 195.0)
}
pub(super) fn panel() -> rune::Color {
    rune::oklch(0.205, 0.018, 240.0)
}
pub(super) fn edge() -> rune::Color {
    rune::oklch(0.32, 0.025, 230.0)
}

pub(super) fn label(value: impl Into<String>) -> El {
    text(value)
        .font(TextStyle::new(13.0, 400.0))
        .wrap()
        .color(tok::theme(true).text_secondary)
}

pub(super) fn heading(value: impl Into<String>) -> El {
    text(value)
        .wrap()
        .font(TextStyle::new(25.0, 650.0))
        .color(tok::theme(true).text)
}

pub(super) fn badge(key: impl Into<String>, value: impl Into<String>, color: rune::Color) -> El {
    row()
        .key(key)
        .h(22.0)
        .px(8.0)
        .center()
        .radius(11.0)
        .fill(color.alpha(0.12))
        .border(1.0, color.alpha(0.25))
        .child(text(value).font(TextStyle::new(10.0, 600.0)).color(color))
}

pub(super) fn watched_badge(key: impl Into<String>) -> El {
    badge(key, "✓ Watched", rune::oklch(0.80, 0.10, 155.0))
}

pub(super) fn empty_state(
    key: &str,
    title: &str,
    description: &str,
    icon: &str,
    actions: El,
) -> El {
    col().key(key).w_full().grow(1.0).min_h(0.0).center().child(
        col()
            .w(440.0)
            .padding(32.0)
            .gap(18.0)
            .center()
            .radius(16.0)
            .fill(panel())
            .border(1.0, edge())
            .child(
                el().w(64.0)
                    .h(64.0)
                    .radius(18.0)
                    .fill(accent().alpha(0.08))
                    .center()
                    .child(text(icon).font(TextStyle::new(26.0, 500.0)).color(accent())),
            )
            .child(
                text(title)
                    .font(TextStyle::new(22.0, 600.0))
                    .color(tok::theme(true).text),
            )
            .child(label(description))
            .child(actions),
    )
}

pub(super) fn button(
    key: impl Into<String>,
    title: impl Into<String>,
    action: impl Fn(&mut State) + 'static,
) -> El {
    let t = tok::theme(true);
    row()
        .key(key)
        .role("button")
        .label(title.into())
        .h(32.0)
        .px(10.0)
        .center()
        .radius(4.0)
        .border(1.0, edge())
        .fill(panel())
        .hover(t.hover)
        .on_press(action)
}

pub(super) fn action(
    key: impl Into<String>,
    title: impl Into<String>,
    f: impl Fn(&mut State) + 'static,
) -> El {
    let title = title.into();
    button(key, title.clone(), f).child(
        text(title)
            .truncate()
            .font(type_::LABEL)
            .color(tok::theme(true).text),
    )
}

pub(super) fn input(
    key: &'static str,
    edit: &TextEdit,
    placeholder: &'static str,
    get: fn(&mut State) -> &mut TextEdit,
) -> El {
    let t = tok::theme(true);
    el().key(format!("{key}-frame"))
        .h(38.0)
        .grow(1.0)
        .px(12.0)
        .center_y()
        .radius(4.0)
        .fill(panel())
        .border(1.0, edge())
        .child(
            text_input(edit, type_::BODY)
                .key(key)
                .w_full()
                .color(t.text)
                .placeholder(placeholder, t.text_secondary)
                .single_line()
                .on_input_key(on_input_key)
                .on_submit(move |s: &mut State, _: &mut Tx| match key {
                    "search" => s.submit_search(),
                    "manifest" => s.install(s.install.text.clone()),
                    "direct-url" => s.play_direct(),
                    _ => {}
                })
                .on_edit(get),
        )
}

pub(super) fn scroll(key: impl Into<String>, content: El) -> El {
    col()
        .key(key)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(content)
}

pub(super) fn excerpt(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let mut result: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

pub(super) fn metric(value: Option<f64>) -> String {
    value.map_or_else(|| "—".into(), |n| format!("{n:.1}"))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Display whole nonnegative seconds with saturating float-to-integer conversion"
)]
pub(super) fn clock(value: Option<f64>) -> String {
    value.map_or_else(
        || "—:—".into(),
        |n| {
            let s = n.max(0.0) as u64;
            format!("{}:{:02}", s / 60, s % 60)
        },
    )
}
