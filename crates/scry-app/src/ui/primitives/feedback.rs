//! Non-interactive status and recoverable messages, separate from action styling.
use super::super::design::tokens::{self, layout, type_};
use super::text::label;
use rune::{Color, El, col, row, text};

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
                .font(type_::ITEM_TITLE)
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
