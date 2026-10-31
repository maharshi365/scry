use super::super::design::tokens::{self, accent, edge, panel, theme, type_};
use super::text::label;
use rune::{Align, El, Icon, PointerPress, ScrollSpec, Side, col, el, icon, text};
use rune_kit::popover;

pub(in crate::ui) fn card() -> El {
    surface(col())
}

pub(in crate::ui) fn surface(content: El) -> El {
    Variant::Card.apply(content)
}

#[derive(Clone, Copy)]
pub(in crate::ui) enum Variant {
    Card,
    Menu,
    Plain,
    Source,
    Provider,
    Episode,
    Calendar,
    Poster,
    Hero,
    Notification,
    Chrome,
}

impl Variant {
    pub fn apply(self, content: El) -> El {
        self.apply_selected(content, false)
    }

    pub fn apply_selected(self, content: El, selected: bool) -> El {
        let (radius, fill, border) = match self {
            Self::Card => (tokens::radius::LG, panel(), edge()),
            Self::Menu => (tokens::radius::SM, panel(), edge()),
            Self::Plain => (tokens::radius::SM, panel(), tokens::transparent_edge()),
            Self::Source => (
                tokens::radius::SM,
                tokens::source_fill(),
                tokens::source_edge(),
            ),
            Self::Provider => (tokens::radius::PROVIDER, panel(), edge()),
            Self::Episode => (
                tokens::radius::EPISODE,
                if selected {
                    tokens::selected()
                } else {
                    panel()
                },
                if selected {
                    tokens::primary_edge()
                } else {
                    tokens::subtle_edge()
                },
            ),
            Self::Calendar => (
                tokens::radius::CALENDAR,
                panel(),
                if selected { accent() } else { edge() },
            ),
            Self::Poster => (
                tokens::poster::RADIUS,
                tokens::transparent(),
                tokens::transparent_edge(),
            ),
            Self::Hero => (tokens::radius::LG, tokens::stats_hero(), edge()),
            Self::Notification => (tokens::radius::LG, panel(), tokens::stats_gold()),
            Self::Chrome => (tokens::radius::NONE, tokens::topbar(), edge()),
        };
        content.radius(radius).fill(fill).border(
            if matches!(self, Self::Plain | Self::Poster) {
                tokens::border::NONE
            } else {
                tokens::border::WIDTH
            },
            border,
        )
    }
}

/// Unframed inset content inside a menu or panel. Never adds another card.
pub(in crate::ui) fn list() -> El {
    col().w_full().padding(4.0)
}

pub(in crate::ui) fn divider() -> El {
    el().w_full()
        .h(tokens::border::WIDTH)
        .fill(tokens::subtle_edge())
}

pub(in crate::ui) fn empty_state(
    key: &str,
    title: &str,
    description: &str,
    glyph: &'static Icon,
    actions: El,
) -> El {
    col().key(key).w_full().grow(1.0).min_h(0.0).center().child(
        card()
            .key(format!("{key}-panel"))
            .w(400.0)
            .padding(tokens::layout::PANEL_INSET)
            .gap(tokens::layout::PAGE_GAP)
            .center()
            .radius(tokens::radius::LG)
            .child(
                el().w(tokens::badge::MEDALLION_SIZE)
                    .h(tokens::badge::MEDALLION_SIZE)
                    .radius(tokens::radius::CHIP)
                    .fill(tokens::empty_icon_fill())
                    .center()
                    .child(icon(glyph, tokens::badge::MEDALLION_ICON).color(accent())),
            )
            .child(text(title).font(type_::EMPTY_TITLE).color(theme().text))
            .child(label(description))
            .child(actions),
    )
}

pub(in crate::ui) fn scroll(key: impl Into<String>, content: El) -> El {
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

/// Attach at the window root, after the trigger, to escape page clipping.
pub(in crate::ui) fn dropdown<S: 'static>(
    key: &str,
    trigger: &str,
    width: f32,
    content: El,
    dismiss: impl Fn(&mut S) + 'static,
) -> El {
    Variant::Menu
        .apply(col())
        .key(key)
        .role("menu")
        .w(width)
        .min_h(0.0)
        .shrink()
        .padding(6.0)
        .anchored_fit(trigger, Side::Bottom, Align::Start, 6.0)
        .modal(true, dismiss)
        .focus_group()
        .focus_return(trigger)
        .child(popover::fit_scroll(content))
}

/// A pointer-anchored menu with the same fitting and scrolling as dropdowns.
pub(in crate::ui) fn context_menu<S: 'static>(
    key: &str,
    point: PointerPress,
    width: f32,
    content: El,
    dismiss: impl Fn(&mut S) + 'static,
) -> El {
    let anchor = format!("{key}-point");
    el().key(format!("{key}-layer"))
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .child(el().key(&anchor).absolute(point.x, point.y).size(0.0, 0.0))
        .child(
            dropdown(key, &anchor, width, content, dismiss).anchored_fit(
                anchor,
                Side::Bottom,
                Align::Start,
                0.0,
            ),
        )
}
