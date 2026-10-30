//! Mechanical precision in Scry's original blue/teal palette.
#![expect(
    clippy::disallowed_methods,
    reason = "This module owns raw visual values"
)]
use rune::{
    Color, oklch, oklcha,
    tok::{self, TextStyle},
};

pub(in crate::ui) mod fonts {
    // Unmodified IBM Plex fonts from google/fonts at
    // 9862abc066c9b83bf84c3e8ed1fc2d8d48f9bbf0, ofl/ibmplex{sans,mono}.
    // The accompanying OFL is included in each deployed copy.
    pub const FAMILIES: (&str, &str) = ("IBM Plex Sans", "IBM Plex Mono");
    pub const FILES: &[&[u8]] = &[
        include_bytes!("../../../assets/fonts/IBMPlexSans.ttf"),
        include_bytes!("../../../assets/fonts/IBMPlexMono-Regular.ttf"),
        include_bytes!("../../../assets/fonts/IBMPlexMono-Medium.ttf"),
    ];
}

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
    tok::Theme {
        bg: oklch(0.185, 0.004, 265.0),
        card: oklch(0.232, 0.005, 265.0),
        card_border: oklcha(1.0, 0.0, 0.0, 0.05),
        text: oklch(0.955, 0.003, 265.0),
        text_secondary: oklch(0.72, 0.008, 265.0),
        separator: oklcha(1.0, 0.0, 0.0, 0.055),
        control_track: oklcha(1.0, 0.0, 0.0, 0.05),
        control_off: oklcha(1.0, 0.0, 0.0, 0.085),
        indicator: oklch(0.43, 0.006, 265.0),
        knob: oklch(0.985, 0.0, 0.0),
        hover: oklcha(1.0, 0.0, 0.0, 0.016),
        press: oklcha(1.0, 0.0, 0.0, 0.03),
        shadow: oklcha(0.0, 0.0, 0.0, 0.42),
        shadow_soft: oklcha(0.0, 0.0, 0.0, 0.28),
        edge: oklcha(0.0, 0.0, 0.0, 0.3),
        knob_shadow: oklcha(0.0, 0.0, 0.0, 0.3),
    }
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
pub(in crate::ui) fn primary_text() -> Color {
    theme().text
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
    pub const WIDTH: f32 = 1.0;
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
    pub const SM: f32 = 2.0;
    pub const LG: f32 = 3.0;
    pub const CHIP: f32 = SM;
    pub const SEGMENTED: f32 = SM;
    pub const PROVIDER: f32 = LG;
    pub const CALENDAR: f32 = SM;
    pub const EPISODE: f32 = SM;
}

pub(in crate::ui) mod opacity {
    pub const DISABLED: f32 = 0.5;
    pub const DETAIL_BACKDROP: f32 = 0.38;
    pub const PLAYER_BACKDROP: f32 = 0.45;
    pub const BUFFERED: f32 = 0.22;
}

pub(in crate::ui) mod motion {
    pub const PLAYBACK_FEEDBACK: std::time::Duration = std::time::Duration::from_millis(500);
    pub const PLAYBACK_FRAME: std::time::Duration = std::time::Duration::from_millis(16);
    pub const PLAYBACK_GROWTH: f32 = 0.25;
    pub const SPINNER_PERIOD: f64 = 0.8;
    pub const SURFACE_SCALE: f32 = 0.99;
    pub const NOTIFICATION_OFFSET: f32 = -24.0;
    pub const HOVER: rune::Anim = rune::Anim::INSTANT;
    pub const POSTER_ZOOM: f32 = 1.04;
    pub const POSTER_ZOOM_CURVE: rune::Spring = rune::Spring::new(0.3, 0.0);
}

pub(in crate::ui) mod player {
    // Keep the intro prompt above the fullscreen controls, even while they are hidden.
    pub const INTRO_FULLSCREEN_BOTTOM: f32 = 128.0;
    pub const INTRO_PROMPT_WIDTH: f32 = 160.0;
    pub const INTRO_LEAD_SECONDS: f64 = 2.0;
    pub const CONTROLS_INSET: f32 = 24.0;
    pub const CONTROLS_MAX_WIDTH: f32 = 1200.0;
    pub const FEEDBACK_SIZE: f32 = 88.0;
    pub const OSD_HEIGHT: f32 = 720.0;
    // ASS uses BGR colors and inverted alpha (00 is opaque).
    pub const OSD_FOREGROUND: &str = "FFFFFF";
    pub const OSD_BACKGROUND: &str = "000000";
    pub const OSD_FOREGROUND_ALPHA: f32 = 32.0;
    pub const OSD_BACKGROUND_ALPHA: f32 = 144.0;
}

/// Shared composition measurements; feature-specific geometry stays in features.
pub(in crate::ui) mod layout {
    pub const PAGE_GAP: f32 = 12.0;
    pub const SECTION_GAP: f32 = 16.0;
    pub const COPY_GAP: f32 = 4.0;
    pub const ACTION_GAP: f32 = 8.0;
    pub const PANEL_INSET: f32 = 16.0;
    pub const ROW_GAP: f32 = 16.0;
    pub const ROW_INSET: f32 = 10.0;
    pub const COPY_MIN_WIDTH: f32 = 200.0;
    pub const CONTROL_MIN_WIDTH: f32 = 240.0;
}

pub(in crate::ui) mod shell {
    /// Keep native traffic lights centered in the integrated header.
    pub const CHROME: rune::Chrome = rune::Chrome {
        full_size_content: true,
        traffic_lights: Some((14.0, 17.0)),
        drag_regions: true,
    };
    pub const TITLEBAR_INSET_GAP: f32 = 14.0;
    pub const SIDEBAR_WIDTH: f32 = 188.0;
    pub const TOPBAR_HEIGHT: f32 = 48.0;
    pub const SIDEBAR_INSET: f32 = 10.0;
    pub const NAV_HEIGHT: f32 = 34.0;
    pub const NAV_GAP: f32 = 4.0;
    pub const NAV_MARKER: f32 = 2.0;
    pub const NAV_ICON: f32 = 16.0;
}

pub(in crate::ui) mod detail {
    pub const SOURCE_WIDTH: f32 = 340.0;
    pub const PREVIEW_WIDTH: f32 = 380.0;
    pub const PREVIEW_INSET: f32 = 16.0;
}

pub(in crate::ui) mod addon {
    pub const TILE_HEIGHT: f32 = 220.0;
    pub const DIALOG_WIDTH: f32 = 560.0;
    pub const FEEDBACK_HEIGHT: f32 = 48.0;
    pub const NOTICE_HEIGHT: f32 = 32.0;
}

pub(in crate::ui) mod guide {
    pub const CHANNEL_WIDTH: f32 = 132.0;
    pub const ROW_HEIGHT: f32 = 52.0;
    pub const PREVIEW_HEIGHT: f32 = 196.0;
    pub const PLAYER_WIDTH: f32 = 300.0;
}

pub(in crate::ui) mod poster {
    pub const HEIGHT: f32 = 190.0;
    pub const CARD_HEIGHT: f32 = 252.0;
    pub const GAP: f32 = 14.0;
    pub const ARTWORK_GAP: f32 = 10.0;
    pub const TITLE_HEIGHT: f32 = 32.0;
    pub const SUBTITLE_GAP: f32 = 5.0;
    pub const MIN_WIDTH: f32 = 124.0;
    pub const RADIUS: f32 = super::radius::SM;
    pub const THUMBNAIL_WIDTH: f32 = 32.0;
    pub const THUMBNAIL_HEIGHT: f32 = 48.0;
    pub const EPISODE_WIDTH: f32 = 96.0;
    pub const EPISODE_HEIGHT: f32 = 60.0;
}

pub(in crate::ui) mod badge {
    pub const PLAY_SIZE: f32 = 28.0;
    pub const PLAY_ICON: f32 = 14.0;
    pub const MEDALLION_SIZE: f32 = 40.0;
    pub const MEDALLION_ICON: f32 = 22.0;
}

pub(in crate::ui) mod progress {
    pub const HEIGHT: f32 = 3.0;
    pub const RADIUS: f32 = 0.0;
    pub const SLIDER_HEIGHT: f32 = 24.0;
    pub const RAIL_HEIGHT: f32 = 4.0;
    pub const THUMB_SIZE: f32 = 8.0;
    pub const COMPACT_HEIGHT: f32 = 3.0;
    pub const SPLIT_HEIGHT: f32 = 6.0;
    pub const SPLIT_RADIUS: f32 = 0.0;
}

pub(in crate::ui) mod calendar {
    pub const GAP: f32 = 4.0;
    pub const MIN_DAY_HEIGHT: f32 = 72.0;
}

pub(in crate::ui) mod control {
    pub const HEIGHT: f32 = 32.0;
    pub const PADDING_X: f32 = 10.0;
    pub const COMPACT_PADDING_X: f32 = 6.0;
    pub const ICON_SIZE: f32 = 16.0;
    pub const ICON_TARGET: f32 = HEIGHT;
    pub const COMPACT_HEIGHT: f32 = 24.0;
    pub const COMPACT_ICON_SIZE: f32 = super::type_::SMALL.size * 1.2;
    pub const RADIUS: f32 = super::radius::SM;
    pub const INPUT_HEIGHT: f32 = HEIGHT;
    pub const DIALOG_HEIGHT: f32 = 36.0;
    pub const DIALOG_ACTION_WIDTH: f32 = 128.0;
    pub const SETTING_HEIGHT: f32 = 34.0;
    pub const SETTING_MIN_WIDTH: f32 = 64.0;
    pub const PILL_HEIGHT: f32 = 36.0;
    pub const PILL_RADIUS: f32 = super::radius::SM;
    pub const CHEVRON_SIZE: f32 = 14.0;
    pub const STEPPER_WIDTH: f32 = 152.0;
}

pub(in crate::ui) mod type_ {
    use super::TextStyle;
    pub const BODY: TextStyle = TextStyle::new(12.0, 400.0);
    pub const LABEL: TextStyle = TextStyle::new(11.0, 500.0).mono();
    pub const SECTION: TextStyle = TextStyle::new(10.0, 500.0).mono();
    pub const TITLE: TextStyle = TextStyle::new(12.0, 600.0);
    pub const HEADING: TextStyle = TextStyle::new(20.0, 600.0);
    pub const BRAND: TextStyle = TextStyle::new(14.0, 600.0).mono();
    pub const EMPTY_TITLE: TextStyle = TextStyle::new(17.0, 600.0);
    pub const BADGE: TextStyle = TextStyle::new(10.0, 500.0).mono();
    pub const ITEM: TextStyle = TextStyle::new(12.0, 500.0);
    pub const POSTER_TITLE: TextStyle = TextStyle::new(12.0, 550.0);
    pub const SECTION_TITLE: TextStyle = TextStyle::new(12.0, 600.0).mono();
    pub const EMPHASIS: TextStyle = TextStyle::new(11.0, 500.0).mono();
    pub const CAPTION: TextStyle = TextStyle::new(10.0, 400.0).mono();
    pub const SMALL: TextStyle = TextStyle::new(10.0, 400.0);
    pub const SMALL_EMPHASIS: TextStyle = TextStyle::new(10.0, 500.0).mono();
    pub const CALENDAR_TITLE: TextStyle = TextStyle::new(11.0, 550.0);
    pub const PLAYER_TITLE: TextStyle = TextStyle::new(16.0, 550.0);
    pub const LOADING_TITLE: TextStyle = TextStyle::new(24.0, 550.0);
    pub const POSTER_FALLBACK: TextStyle = TextStyle::new(40.0, 400.0).mono();
    pub const THUMBNAIL_FALLBACK: TextStyle = TextStyle::new(16.0, 500.0);
    pub const STATS_HERO: TextStyle = TextStyle::new(36.0, 500.0).mono();
    pub const STATS_NUMBER: TextStyle = TextStyle::new(22.0, 500.0).mono();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_theme_preserves_scrys_original_palette() {
        for (actual, original) in [
            (accent(), oklch(0.79, 0.12, 195.0)),
            (canvas(), oklch(0.17, 0.018, 240.0)),
            (panel(), oklch(0.205, 0.018, 240.0)),
            (sidebar(), oklch(0.145, 0.018, 240.0)),
            (topbar(), oklch(0.15, 0.018, 240.0)),
            (selected(), oklch(0.29, 0.05, 220.0)),
        ] {
            assert_eq!(actual, original);
        }
        assert_eq!(primary_fill(), accent().alpha(0.18));
        assert_eq!(primary_text(), theme().text);
    }
}
