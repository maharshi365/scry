//! Unframed gallery cards. Callers supply artwork, status, and actions.
use super::super::design::tokens::{self, theme, type_};
use rune::{El, col, text};

#[derive(Clone, Copy)]
pub(in crate::ui) enum Selection {
    Normal,
    Selected,
}

pub(in crate::ui) struct Content<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub subtitle_key: &'a str,
    pub artwork: El,
}

pub(in crate::ui) fn card(key: &str, content: Content<'_>, selection: Selection) -> El {
    super::interaction::focus_ring(
        super::surface::Variant::Poster
            .apply(col())
            .key(key)
            .role("button")
            .label(format!("Open {}", content.title))
            .focusable(true)
            .focus_selected(matches!(selection, Selection::Selected))
            .w_full()
            .min_w(0.0)
            .h(tokens::poster::CARD_HEIGHT)
            .gap(tokens::poster::ARTWORK_GAP)
            .child(content.artwork)
            .child(
                col()
                    .w_full()
                    .min_w(0.0)
                    .gap(tokens::poster::SUBTITLE_GAP)
                    .child(
                        text(content.title)
                            .key(format!("poster-title-{key}"))
                            .w_full()
                            .h(tokens::poster::TITLE_HEIGHT)
                            .wrap_anywhere()
                            .max_lines(2)
                            .font(type_::POSTER_TITLE)
                            .color(theme().text),
                    )
                    .child(
                        super::text::label(content.subtitle)
                            .key(content.subtitle_key)
                            .w_full()
                            .truncate()
                            .font(type_::CAPTION),
                    ),
            ),
    )
}
