use super::super::design::tokens::{accent, focus};
use rune::El;

pub(in crate::ui) fn focus_ring(content: El) -> El {
    content.focus_ring(focus::WIDTH, focus::OFFSET, accent())
}
