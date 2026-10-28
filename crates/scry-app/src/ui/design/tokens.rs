//! Semantic colors and named text roles for the dark native interface.
#![expect(
    clippy::disallowed_methods,
    reason = "This module owns raw visual values"
)]
use rune::{
    Color,
    tok::{self, TextStyle},
};

pub(in crate::ui) mod subtitle {
    // libmpv colors use AARRGGBB, not CSS RRGGBBAA.
    pub const TEXT_COLORS: &[(&str, &str)] = &[
        ("White", "#FFFFFFFF"),
        ("Yellow", "#FFFFFF00"),
        ("Green", "#FF00FF00"),
    ];
    pub const BACKGROUND_COLORS: &[(&str, &str)] = &[
        ("Transparent", "#00000000"),
        ("50% black", "#80000000"),
        ("Black", "#FF000000"),
    ];
    pub const OUTLINE_COLORS: &[(&str, &str)] = &[
        ("Black", "#FF000000"),
        ("White", "#FFFFFFFF"),
        ("None", "#00000000"),
    ];
}

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
pub(in crate::ui) fn stats_hero() -> Color {
    rune::oklch(0.225, 0.035, 215.0)
}
pub(in crate::ui) fn stats_violet() -> Color {
    rune::oklch(0.73, 0.14, 295.0)
}
pub(in crate::ui) fn stats_gold() -> Color {
    rune::oklch(0.81, 0.13, 80.0)
}
pub(in crate::ui) fn stats_badge_fill(earned: bool) -> Color {
    if earned {
        stats_gold().alpha(0.12)
    } else {
        edge().alpha(0.3)
    }
}
pub(in crate::ui) fn stats_rail() -> Color {
    edge().alpha(0.5)
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

pub(in crate::ui) fn primary_fill() -> Color {
    accent().alpha(0.18)
}
pub(in crate::ui) fn primary_edge() -> Color {
    accent().alpha(0.5)
}
pub(in crate::ui) fn danger_fill() -> Color {
    danger().alpha(0.12)
}
pub(in crate::ui) fn danger_edge() -> Color {
    danger().alpha(0.4)
}
pub(in crate::ui) fn transparent() -> Color {
    panel().alpha(0.0)
}
pub(in crate::ui) fn transparent_edge() -> Color {
    edge().alpha(0.0)
}
pub(in crate::ui) fn preview_panel() -> Color {
    panel().alpha(0.94)
}
pub(in crate::ui) fn chip_fill() -> Color {
    panel().alpha(0.8)
}
pub(in crate::ui) fn subtle_edge() -> Color {
    edge().alpha(0.6)
}
pub(in crate::ui) fn source_fill() -> Color {
    panel().alpha(0.4)
}
pub(in crate::ui) fn source_edge() -> Color {
    edge().alpha(0.3)
}
pub(in crate::ui) fn play_fill() -> Color {
    accent().alpha(0.12)
}
pub(in crate::ui) fn empty_icon_fill() -> Color {
    accent().alpha(0.08)
}

pub(in crate::ui) mod focus {
    pub const WIDTH: f32 = 2.0;
    pub const OFFSET: f32 = 2.0;
}

pub(in crate::ui) mod overlay {
    pub const RADIUS: f32 = super::radius::LG;
}

pub(in crate::ui) mod border {
    pub const NONE: f32 = 0.0;
    pub const WIDTH: f32 = 1.0;
}

pub(in crate::ui) mod radius {
    pub const NONE: f32 = 0.0;
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 6.0;
    pub const MD: f32 = 8.0;
    pub const LG: f32 = 12.0;
    pub const XL: f32 = 16.0;
    pub const CHIP: f32 = 18.0;
    pub const SEGMENTED: f32 = 10.0;
    pub const PROVIDER: f32 = 14.0;
    pub const CALENDAR: f32 = 7.0;
    pub const EPISODE: f32 = 9.0;
}

pub(in crate::ui) mod opacity {
    pub const DISABLED: f32 = 0.5;
    pub const DETAIL_BACKDROP: f32 = 0.38;
    pub const PLAYER_BACKDROP: f32 = 0.45;
    pub const BUFFERED: f32 = 0.22;
}

pub(in crate::ui) mod motion {
    pub const SPINNER_PERIOD: f64 = 0.8;
}

pub(in crate::ui) mod poster {
    pub const HEIGHT: f32 = 246.0;
    pub const CARD_HEIGHT: f32 = 302.0;
    pub const GAP: f32 = 18.0;
    pub const MIN_WIDTH: f32 = 156.0;
    pub const RADIUS: f32 = super::radius::XS;
    pub const THUMBNAIL_WIDTH: f32 = 32.0;
    pub const THUMBNAIL_HEIGHT: f32 = 48.0;
    pub const EPISODE_WIDTH: f32 = 96.0;
    pub const EPISODE_HEIGHT: f32 = 60.0;
}

pub(in crate::ui) mod badge {
    pub const PLAY_SIZE: f32 = 28.0;
    pub const PLAY_ICON: f32 = 14.0;
    pub const MEDALLION_SIZE: f32 = 56.0;
    pub const MEDALLION_ICON: f32 = 28.0;
}

pub(in crate::ui) mod progress {
    pub const HEIGHT: f32 = 6.0;
    pub const RADIUS: f32 = HEIGHT / 2.0;
    pub const SLIDER_HEIGHT: f32 = 24.0;
    pub const RAIL_HEIGHT: f32 = 4.0;
    pub const THUMB_SIZE: f32 = 10.0;
    pub const COMPACT_HEIGHT: f32 = 5.0;
    pub const SPLIT_HEIGHT: f32 = 10.0;
    pub const SPLIT_RADIUS: f32 = 3.0;
}

pub(in crate::ui) mod calendar {
    pub const GAP: f32 = 8.0;
    pub const MIN_DAY_HEIGHT: f32 = 82.0;
}

pub(in crate::ui) mod control {
    pub const HEIGHT: f32 = 38.0;
    pub const ICON_SIZE: f32 = 22.0;
    pub const ICON_TARGET: f32 = HEIGHT;
    pub const COMPACT_HEIGHT: f32 = 24.0;
    pub const COMPACT_ICON_SIZE: f32 = super::type_::SMALL.size * 1.2;
    pub const RADIUS: f32 = super::radius::XS;
    pub const INPUT_HEIGHT: f32 = HEIGHT;
    pub const SETTING_HEIGHT: f32 = 46.0;
    pub const SETTING_MIN_WIDTH: f32 = 64.0;
    pub const PILL_HEIGHT: f32 = 52.0;
    pub const PILL_RADIUS: f32 = PILL_HEIGHT / 2.0;
    pub const CHEVRON_SIZE: f32 = 14.0;
    pub const STEPPER_WIDTH: f32 = 152.0;
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
    pub const STATS_HERO: TextStyle = TextStyle::new(48.0, 650.0);
    pub const STATS_NUMBER: TextStyle = TextStyle::new(26.0, 600.0);
    pub const STATS_KICKER: TextStyle = TextStyle::new(10.0, 650.0);
}
