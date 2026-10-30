//! Intro timing is independent of the fullscreen control bar's idle state.
use super::{PlayerAction, State};
use crate::ui::{
    Page,
    design::tokens,
    primitives::{
        button::Button,
        feedback::{self, Tone},
        surface::Variant,
    },
};
use rune::{El, el, row};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Upcoming,
    Active,
}

pub(super) fn phase(s: &State) -> Option<Phase> {
    // Player menus take priority over the prompt in the native floating-overlay slot.
    if !s.player.playing
        || !s.player.ready
        || s.player.seek_preview.is_some()
        || s.navigation.page != Page::Player
        || s.player.subtitles.overlay.is_open()
        || s.player.audio.overlay.is_open()
        || s.player.stats_overlay.is_open()
    {
        return None;
    }
    let intro = s.player.intro.as_ref()?;
    let position = s.player.sample.position_s?;
    let duration = s.player.sample.duration_s?;
    let start = milliseconds(intro.start_ms);
    let end = milliseconds(intro.end_ms);
    if !position.is_finite()
        || !duration.is_finite()
        || start >= end
        || end > duration
        || position < 0.0
        || position >= end
    {
        return None;
    }
    if position >= start {
        Some(Phase::Active)
    } else if position >= (start - tokens::player::INTRO_LEAD_SECONDS).max(0.0) {
        Some(Phase::Upcoming)
    } else {
        None
    }
}

fn milliseconds(value: u32) -> f64 {
    f64::from(value) / 1000.0
}

pub(super) fn prompt(s: &State) -> Option<El> {
    let phase = phase(s)?;
    let content = match phase {
        Phase::Upcoming => Variant::Plain
            .apply(el())
            .w_full()
            .h_full()
            .px(tokens::layout::ROW_INSET)
            .center()
            .child(feedback::status(
                "player-intro-upcoming",
                "Intro in 2s",
                Tone::Neutral,
            )),
        Phase::Active => Button::new("player-skip-intro", "Skip Intro")
            .on_press(skip)
            .w_full(),
    };
    let prompt = el()
        .key("player-intro-prompt")
        .w(tokens::player::INTRO_PROMPT_WIDTH)
        .h(tokens::control::HEIGHT)
        .child(content)
        .transition(crate::ui::primitives::motion::fade())
        .on_layout(|s: &mut State, rect| {
            s.player.intro_rect = Some(rect);
            s.cover_player();
        });
    let mut lane = row().w_full();
    if s.player.fullscreen {
        lane = lane.max_w(tokens::player::CONTROLS_MAX_WIDTH);
    }
    let inset = if s.player.fullscreen {
        tokens::player::CONTROLS_INSET
    } else {
        tokens::layout::PANEL_INSET
    };
    let bottom = if s.player.fullscreen {
        tokens::player::INTRO_FULLSCREEN_BOTTOM
    } else {
        tokens::layout::PANEL_INSET
    };
    Some(
        el().pin(Some(inset), None, Some(inset), Some(bottom))
            .center()
            .child(lane.child(el().grow(1.0)).child(prompt)),
    )
}

fn skip(s: &mut State) {
    if phase(s) == Some(Phase::Active)
        && let Some(intro) = &s.player.intro
    {
        s.player_action(PlayerAction::SeekTo(milliseconds(intro.end_ms)));
    }
}
