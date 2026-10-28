use super::super::design::tokens::{accent, focus};
use rune::El;

/// Make caller-owned card contents a keyboard-accessible action.
pub(in crate::ui) fn action(content: El, selected: bool) -> El {
    focus_ring(
        content
            .role("button")
            .focusable(true)
            .focus_selected(selected)
            .hover(super::super::design::tokens::theme().hover),
    )
}

pub(in crate::ui) fn focus_ring(content: El) -> El {
    super::motion::hover(content).focus_ring(focus::WIDTH, focus::OFFSET, accent())
}
