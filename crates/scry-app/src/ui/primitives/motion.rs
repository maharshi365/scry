//! Shared motion policy. Views choose intent, not animation parameters.
use super::super::design::tokens;
use rune::{El, Transition};

/// Pages change as a unit. Do not tween old hit targets into unrelated content.
pub(in crate::ui) fn route(tx: &mut rune::Tx) {
    tx.instant();
}

pub(in crate::ui) fn hover(content: El) -> El {
    content.hover_motion(tokens::motion::HOVER, tokens::motion::HOVER)
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
