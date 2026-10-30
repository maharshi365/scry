//! Shared motion policy. Views choose intent, not animation parameters.
use super::super::design::tokens;
use rune::{El, Transition};

pub(in crate::ui) fn playback_feedback(elapsed: std::time::Duration) -> Option<(f32, f32)> {
    if elapsed >= tokens::motion::PLAYBACK_FEEDBACK {
        return None;
    }
    let progress = elapsed.as_secs_f32() / tokens::motion::PLAYBACK_FEEDBACK.as_secs_f32();
    let eased = 1.0 - (1.0 - progress).powi(3);
    Some((
        1.0 + tokens::motion::PLAYBACK_GROWTH * eased,
        1.0 - progress * progress,
    ))
}

/// Pages change as a unit. Do not tween old hit targets into unrelated content.
pub(in crate::ui) fn route(tx: &mut rune::Tx) {
    tx.instant();
}

pub(in crate::ui) fn hover(content: El) -> El {
    content.hover_motion(tokens::motion::HOVER, tokens::motion::HOVER)
}

#[derive(Clone, Copy)]
pub(in crate::ui) enum Zoom {
    Rest,
    Hovered,
}

pub(in crate::ui) fn poster_zoom(content: El, zoom: Zoom) -> El {
    content
        .scale(match zoom {
            Zoom::Rest => 1.0,
            Zoom::Hovered => tokens::motion::POSTER_ZOOM,
        })
        .scale_motion(tokens::motion::POSTER_ZOOM_CURVE)
}

pub(in crate::ui) fn surface(content: El) -> El {
    content.transition(Transition::scale(tokens::motion::SURFACE_SCALE))
}

pub(in crate::ui) fn fade() -> Transition {
    Transition::opacity()
}

pub(in crate::ui) fn notification() -> Transition {
    Transition::move_edge(rune::Edge::Top)
        .combined(Transition::offset(0.0, tokens::motion::NOTIFICATION_OFFSET))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_feedback_grows_and_fades_then_disappears() -> Result<(), &'static str> {
        let (start_size, start_opacity) = playback_feedback(std::time::Duration::ZERO)
            .ok_or("Feedback is missing at the start")?;
        let (end_size, end_opacity) = playback_feedback(tokens::motion::PLAYBACK_FEEDBACK / 2)
            .ok_or("Feedback is missing halfway through")?;
        assert!(end_size > start_size);
        assert!(end_opacity < start_opacity && end_opacity > 0.0);
        assert!(playback_feedback(tokens::motion::PLAYBACK_FEEDBACK).is_none());
        Ok(())
    }
}
