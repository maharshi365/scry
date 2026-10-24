//! Semantic colors and named text roles for the dark native interface.
#![expect(
    clippy::disallowed_methods,
    reason = "This module owns raw visual values"
)]
use rune::{
    Color,
    tok::{self, TextStyle},
};

pub(in crate::ui) fn theme() -> tok::Theme {
    tok::theme(true)
}
pub(in crate::ui) fn accent() -> Color {
    rune::oklch(0.79, 0.12, 195.0)
}
pub(in crate::ui) fn panel() -> Color {
    rune::oklch(0.205, 0.018, 240.0)
}
pub(in crate::ui) fn edge() -> Color {
    rune::oklch(0.32, 0.025, 230.0)
}
pub(in crate::ui) fn canvas() -> Color {
    rune::oklch(0.17, 0.018, 240.0)
}
pub(in crate::ui) fn sidebar() -> Color {
    rune::oklch(0.145, 0.018, 240.0)
}
pub(in crate::ui) fn topbar() -> Color {
    rune::oklch(0.15, 0.018, 240.0)
}
pub(in crate::ui) fn selected() -> Color {
    rune::oklch(0.29, 0.05, 220.0)
}
pub(in crate::ui) fn disabled() -> Color {
    rune::oklch(0.4, 0.01, 275.0)
}
pub(in crate::ui) fn success() -> Color {
    rune::oklch(0.80, 0.10, 155.0)
}
pub(in crate::ui) fn warning() -> Color {
    rune::oklch(0.8, 0.12, 70.0)
}
pub(in crate::ui) fn danger() -> Color {
    rune::oklch(0.78, 0.14, 25.0)
}
pub(in crate::ui) fn video() -> Color {
    rune::oklch(0.08, 0.0, 0.0)
}
pub(in crate::ui) fn scrim() -> Color {
    rune::oklch(0.05, 0.0, 0.0).alpha(0.7)
}
pub(in crate::ui) fn poster(index: usize) -> Color {
    rune::oklch(0.25 + [0.0, 0.025, 0.05][index % 3], 0.035, 230.0)
}

pub(in crate::ui) mod control {
    pub const HEIGHT: f32 = 32.0;
    pub const ICON_SIZE: f32 = 22.0;
    pub const ICON_TARGET: f32 = 36.0;
    pub const RADIUS: f32 = 4.0;
    pub const INPUT_HEIGHT: f32 = 38.0;
}

pub(in crate::ui) mod type_ {
    use super::TextStyle;
    pub use rune::tok::type_::{BODY, LABEL, SECTION, TITLE};
    pub const HEADING: TextStyle = TextStyle::new(25.0, 650.0);
    pub const EMPTY_TITLE: TextStyle = TextStyle::new(22.0, 600.0);
    pub const BADGE: TextStyle = TextStyle::new(10.0, 600.0);
    pub const ITEM: TextStyle = TextStyle::new(14.0, 500.0);
    pub const ITEM_TITLE: TextStyle = TextStyle::new(14.0, 600.0);
    pub const POSTER_TITLE: TextStyle = TextStyle::new(14.0, 550.0);
    pub const SECTION_TITLE: TextStyle = TextStyle::new(18.0, 600.0);
    pub const EMPHASIS: TextStyle = TextStyle::new(13.0, 500.0);
    pub const CAPTION: TextStyle = TextStyle::new(11.0, 400.0);
    pub const SMALL: TextStyle = TextStyle::new(10.0, 400.0);
    pub const SMALL_EMPHASIS: TextStyle = TextStyle::new(10.0, 500.0);
    pub const CALENDAR_TITLE: TextStyle = TextStyle::new(12.0, 550.0);
    pub const PLAYER_TITLE: TextStyle = TextStyle::new(20.0, 550.0);
    pub const LOADING_TITLE: TextStyle = TextStyle::new(30.0, 550.0);
    pub const POSTER_FALLBACK: TextStyle = TextStyle::new(52.0, 300.0);
    pub const THUMBNAIL_FALLBACK: TextStyle = TextStyle::new(16.0, 500.0);
}
