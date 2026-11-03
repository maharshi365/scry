//! Non-interactive status and recoverable messages, separate from action styling.
use super::super::design::tokens::{self, layout, type_};
use super::text::label;
use rune::{Color, El, col, row, text};

pub(in crate::ui) fn source_recovery<S: 'static>(
    key: &str,
    choose: impl Fn(&mut S) + Clone + 'static,
) -> El {
    super::dialog::layer(
        format!("{key}-layer"),
        super::dialog::panel(key, choose.clone())
            .role("dialog")
            .label("This link could not be played.")
            .w(tokens::recovery::DIALOG_WIDTH)
            .padding(layout::PANEL_INSET)
            .gap(layout::COPY_GAP)
            .fill(tokens::danger())
            .child(
                text("This link could not be played.")
                    .font(type_::TITLE)
                    .color(tokens::on_danger())
                    .wrap(),
            )
            .child(
                super::button::Button::new("player-choose-source", "Choose source")
                    .on_press(choose),
            ),
    )
}

pub(in crate::ui) fn loading_indicator(
    key: &str,
    title: &str,
    logo: Option<&rune::Pixels>,
    motion: super::motion::LoadingMotion,
) -> El {
    let (scale, opacity, reveal) = super::motion::loading_pose(motion);
    let artwork = |key: String| {
        let frame = rune::el()
            .key(key)
            .w_full()
            .h(tokens::player::LOADING_HEIGHT)
            .center();
        if let Some(logo) = logo {
            frame.pixels_fit(logo, rune::SurfaceFit::Fit)
        } else {
            frame.child(
                text(title)
                    .font(type_::LOADING_TITLE)
                    .color(tokens::theme().text)
                    .wrap(),
            )
        }
    };
    rune::el()
        .key(format!("{key}-indicator"))
        .role("status")
        .label(format!("Loading {title}"))
        .w_full()
        .max_w(tokens::player::LOADING_WIDTH)
        .h(tokens::player::LOADING_HEIGHT)
        .scale(scale)
        .child(artwork(format!("{key}-dim")).opacity(tokens::player::LOADING_DIM))
        .child(
            rune::el()
                .key(format!("{key}-reveal"))
                .absolute(0.0, 0.0)
                .w_frac(reveal)
                .h(tokens::player::LOADING_HEIGHT)
                .clip_rounded(tokens::radius::NONE)
                .opacity(opacity)
                .child(
                    artwork(format!("{key}-bright"))
                        .absolute(0.0, 0.0)
                        .w_frac(1.0 / reveal.max(f32::EPSILON)),
                ),
        )
}

#[derive(Clone, Copy)]
pub(in crate::ui) enum VideoCue {
    Play,
    Pause,
    Buffering,
}

pub(in crate::ui) fn video_cue(cue: VideoCue, elapsed: std::time::Duration, width: f32) -> String {
    use super::super::design::icons;
    let (scale, opacity) = if matches!(cue, VideoCue::Buffering) {
        (1.0, 1.0)
    } else {
        super::motion::playback_feedback(elapsed).unwrap_or((1.0, 0.0))
    };
    let size = tokens::player::FEEDBACK_SIZE * scale;
    let x = (width - size) / 2.0;
    let y = (tokens::player::OSD_HEIGHT - size) / 2.0;
    let background_alpha = ass_alpha(tokens::player::OSD_BACKGROUND_ALPHA, opacity);
    let foreground_alpha = ass_alpha(tokens::player::OSD_FOREGROUND_ALPHA, opacity);
    let background = tokens::player::OSD_BACKGROUND;
    let foreground = tokens::player::OSD_FOREGROUND;
    let glyph = match cue {
        VideoCue::Play => icons::OSD_PLAY,
        VideoCue::Pause => icons::OSD_PAUSE,
        VideoCue::Buffering => icons::OSD_SPINNER,
    };
    let rotation = if matches!(cue, VideoCue::Buffering) {
        -360.0 * (elapsed.as_secs_f64() / tokens::motion::SPINNER_PERIOD).fract()
    } else {
        0.0
    };
    let center_x = width / 2.0;
    let center_y = tokens::player::OSD_HEIGHT / 2.0;
    // Each drawing is a separate ASS event so its position and opacity are independent.
    format!(
        "{{\\an7\\pos({x:.2},{y:.2})\\bord0\\shad0\\fscx{size:.2}\\fscy{size:.2}\\1c&H{background}&\\1a&H{background_alpha}&\\p1}}{}\n{{\\an7\\pos({x:.2},{y:.2})\\org({center_x:.2},{center_y:.2})\\frz{rotation:.2}\\bord0\\shad0\\fscx{size:.2}\\fscy{size:.2}\\1c&H{foreground}&\\1a&H{foreground_alpha}&\\p1}}{glyph}",
        icons::OSD_DISC,
    )
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "ASS alpha is a rounded byte, clamped to its unsigned range"
)]
fn ass_alpha(base: f32, opacity: f32) -> String {
    format!(
        "{:02X}",
        (255.0 - (255.0 - base) * opacity).round().clamp(0.0, 255.0) as u8
    )
}

#[derive(Clone, Copy)]
pub(in crate::ui) enum Tone {
    Neutral,
    Success,
    Warning,
    Danger,
}

impl Tone {
    fn color(self) -> Color {
        match self {
            Self::Neutral => tokens::theme().text_secondary,
            Self::Success => tokens::success(),
            Self::Warning => tokens::warning(),
            Self::Danger => tokens::danger(),
        }
    }
}

pub(in crate::ui) fn status(key: &str, title: &str, tone: Tone) -> El {
    text(title)
        .key(key)
        .role("status")
        .label(title)
        .wrap()
        .font(type_::LABEL)
        .color(tone.color())
}

/// Flat content: suitable inside a page, card, or dialog without nesting cards.
pub(in crate::ui) fn message(
    key: &str,
    title: &str,
    description: &str,
    tone: Tone,
    actions: El,
) -> El {
    col()
        .key(key)
        .w_full()
        .min_w(0.0)
        .gap(layout::COPY_GAP)
        .child(
            text(title)
                .key(format!("{key}-title"))
                .role("status")
                .wrap()
                .font(type_::TITLE)
                .color(tone.color()),
        )
        .child(label(description).key(format!("{key}-description")))
        .child(
            row()
                .w_full()
                .gap(layout::ACTION_GAP)
                .flex_wrap()
                .child(actions),
        )
}
