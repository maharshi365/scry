//! Shared screen composition. Features supply content and action policy.
use super::super::design::tokens::layout;
use super::{
    form::section_header,
    surface::{card, divider, scroll},
    text::{heading, label},
};
use rune::{El, col, row};

pub(in crate::ui) fn page(
    key: &str,
    title: &str,
    description: &str,
    actions: El,
    content: El,
) -> El {
    col()
        .key(key)
        .w_full()
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .gap(layout::PAGE_GAP)
        .child(detail_header(
            &format!("{key}-header"),
            title,
            description,
            actions,
        ))
        .child(scroll(format!("{key}-scroll"), content))
}

pub(in crate::ui) fn detail_header(key: &str, title: &str, description: &str, actions: El) -> El {
    let mut copy = col()
        .key(format!("{key}-copy"))
        .w(0.0)
        .grow(1.0)
        .min_w(layout::COPY_MIN_WIDTH)
        .gap(layout::COPY_GAP)
        .child(heading(title).key(format!("{key}-title")));
    if !description.is_empty() {
        copy = copy.child(label(description).key(format!("{key}-description")));
    }
    col()
        .key(key)
        .w_full()
        .min_w(0.0)
        .gap(layout::ACTION_GAP)
        .child(
            row()
                .w_full()
                .min_w(0.0)
                .center_y()
                .flex_wrap()
                .gap(layout::ACTION_GAP)
                .child(copy)
                .child(actions.flex_wrap().gap(layout::ACTION_GAP)),
        )
        .child(divider())
}

pub(in crate::ui) fn section(key: &str, title: &str, description: &str, content: El) -> El {
    col()
        .key(key)
        .w_full()
        .min_w(0.0)
        .gap(layout::PAGE_GAP)
        .child(section_header(title, description).pb(0.0))
        .child(content)
}

/// One outer surface, with flat rows and one separator between each pair.
pub(in crate::ui) fn settings_group(key: &str, rows: impl IntoIterator<Item = El>) -> El {
    let mut group = card()
        .key(key)
        .w_full()
        .min_w(0.0)
        .padding(layout::PANEL_INSET);
    for (index, item) in rows.into_iter().enumerate() {
        if index > 0 {
            group = group.child(divider().key(format!("{key}-separator-{index}")));
        }
        group = group.child(item);
    }
    group
}
