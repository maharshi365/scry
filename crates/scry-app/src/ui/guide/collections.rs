use super::super::{
    State,
    controls::input,
    primitives::{
        button::{Button, Variant},
        surface::scroll,
        text::{heading, label},
    },
};
use rune::{El, col, row};
use scry_library::{
    Media, MediaKind,
    guide::{ChannelSource, Collection, GuideAction},
};

pub(super) fn view(s: &State) -> El {
    let mut content = col()
        .gap(12.0)
        .w_full()
        .child(
            row()
                .center_y()
                .gap(12.0)
                .child(heading("Manage collections").grow(1.0))
                .child(
                    Button::new("guide-close-collections", "Done").on_press(|s: &mut State| {
                        s.guide.collections_open = false;
                        s.autoplay_guide();
                    }),
                ),
        )
        .child(collection_editor(s));
    if let Some(error) = &s.guide.error {
        content = content.child(label(error.clone()));
    }
    if let Some(media) = &s.guide.adding {
        content = content.child(label(format!("Add {} to a collection below", media.title)));
    }
    for group in &s.guide.data.collections {
        content = content.child(collection(s, group));
    }
    scroll("guide-collections-scroll", content)
}

fn choices() -> El {
    row().min_w(0.0).gap(6.0).center_y().flex_wrap()
}

fn collection_editor(s: &State) -> El {
    col()
        .gap(8.0)
        .child(label(
            "Collections · group shows or movies into your own channels",
        ))
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(
                    input(
                        "guide-collection-name",
                        &s.guide.name,
                        "Collection name",
                        |s| &mut s.guide.name,
                        create_collection,
                    )
                    .grow(1.0)
                    .max_w(300.0),
                )
                .child(
                    Button::new("guide-create-collection", "Create")
                        .enabled(!s.guide.busy && !s.guide.name.text.trim().is_empty())
                        .on_press(create_collection),
                ),
        )
}

fn membership_button(s: &State, key: String, collection: i64, media: Media, included: bool) -> El {
    let title = format!("{} {}", if included { "+" } else { "−" }, media.title);
    Button::new(key, title)
        .enabled(!s.guide.busy)
        .on_press(move |s: &mut State| {
            s.guide_action(GuideAction::Membership {
                collection,
                media: media.clone(),
                included,
            });
        })
        .max_w(240.0)
}

fn collection(s: &State, group: &Collection) -> El {
    let id = group.playlist.id;
    let active = s.guide.collection == Some(id);
    let enabled = s
        .guide
        .data
        .channels
        .iter()
        .any(|c| c.source == ChannelSource::Collection(id));
    let header = choices()
        .child(
            Button::new(
                format!("guide-collection-{id}"),
                format!("{} · {} titles", group.playlist.name, group.members.len()),
            )
            .selected(active)
            .on_press(move |s: &mut State| {
                s.guide.collection = if active { None } else { Some(id) };
            }),
        )
        .child(
            Button::new(
                format!("guide-enable-{id}"),
                if enabled {
                    "On guide"
                } else {
                    "Use as channel"
                },
            )
            .enabled(!enabled && !group.members.is_empty() && !s.guide.busy)
            .on_press(move |s: &mut State| {
                s.guide_action(GuideAction::EnableCollection(id));
            }),
        )
        .child(
            Button::new(format!("guide-rename-{id}"), "Rename")
                .variant(Variant::Ghost)
                .enabled(!s.guide.name.text.trim().is_empty() && !s.guide.busy)
                .on_press(move |s: &mut State| {
                    s.guide_action(GuideAction::RenameCollection(id, s.guide.name.text.clone()));
                }),
        )
        .child(
            Button::new(format!("guide-delete-{id}"), "Delete")
                .variant(Variant::Ghost)
                .enabled(!s.guide.busy)
                .on_press(move |s: &mut State| {
                    s.guide_action(GuideAction::DeleteCollection(id));
                }),
        );
    let mut content = col().gap(8.0).child(header);
    if let Some(media) = &s.guide.adding
        && group
            .members
            .iter()
            .all(|member| member.key != media.key && member.key.kind == media.key.kind)
    {
        content = content.child(choices().child(membership_button(
            s,
            format!("guide-add-selected-{id}"),
            id,
            media.clone(),
            true,
        )));
    }
    if active {
        content = content.child(collection_members(s, group));
    }
    content
}

fn collection_members(s: &State, group: &Collection) -> El {
    let id = group.playlist.id;
    let mut members = choices();
    for (index, media) in group.members.iter().enumerate() {
        members = members.child(membership_button(
            s,
            format!("guide-remove-{id}-{index}"),
            id,
            media.clone(),
            false,
        ));
    }
    let mut available = choices();
    for (index, media) in s
        .calendar
        .series
        .iter()
        .filter(|m| {
            matches!(m.key.kind, MediaKind::Series | MediaKind::Movie)
                && group
                    .members
                    .iter()
                    .all(|member| member.key != m.key && member.key.kind == m.key.kind)
        })
        .take(200)
        .enumerate()
    {
        available = available.child(membership_button(
            s,
            format!("guide-add-library-{id}-{index}"),
            id,
            media.clone(),
            true,
        ));
    }
    col().gap(8.0).child(members).child(available)
}

fn create_collection(s: &mut State) {
    if !s.guide.busy && !s.guide.name.text.trim().is_empty() {
        s.guide_action(GuideAction::CreateCollection(s.guide.name.text.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submitting_blank_collection_name_does_not_attempt_a_library_action() {
        let mut state = crate::ui::app::init();
        state.guide.name = rune::TextEdit::new(" \t\u{2003}");
        create_collection(&mut state);
        assert!(state.guide.error.is_none());
        assert!(!state.guide.busy);
    }
}
