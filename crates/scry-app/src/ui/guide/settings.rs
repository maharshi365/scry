use super::super::{
    State,
    controls::input,
    primitives::{
        button::{Button, Variant},
        surface::{card, scroll},
        text::{heading, label},
    },
};
use rune::{El, col, row};
use scry_library::{
    Media, MediaKind,
    guide::{ChannelSource, Collection, EpisodeOrder, GuideAction, SourcePolicy},
};

pub(super) fn view(s: &State) -> El {
    let mut content = col()
        .gap(12.0)
        .w_full()
        .child(
            row()
                .center_y()
                .gap(12.0)
                .child(heading("Guide settings").grow(1.0))
                .child(
                    Button::new("guide-close-settings", "Done").on_press(|s: &mut State| {
                        s.guide.settings = false;
                        s.autoplay_guide();
                    }),
                ),
        )
        .child(source_settings(s))
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
    scroll("guide-settings-scroll", content)
}

fn choices() -> El {
    row().min_w(0.0).gap(6.0).center_y().flex_wrap()
}

fn option_row(key: &str, title: &str, options: El) -> El {
    row()
        .gap(12.0)
        .center_y()
        .w_full()
        .min_w(0.0)
        .child(label(title).key(format!("{key}-label")).w(76.0).min_w(76.0))
        .child(options.grow(1.0))
}

fn policy_button(s: &State, key: String, title: &str, selected: bool, policy: SourcePolicy) -> El {
    Button::new(key, title)
        .selected(selected)
        .enabled(!s.guide.busy)
        .on_press(move |s: &mut State| {
            s.update_guide_policy(policy.clone());
        })
}

fn source_settings(s: &State) -> El {
    let policy = &s.guide.data.policy;
    let mut qualities = choices();
    for (quality, title) in [
        ("", "Auto"),
        ("720", "720p"),
        ("1080", "1080p"),
        ("2160", "4K"),
    ] {
        let mut next = policy.clone();
        next.quality = quality.into();
        qualities = qualities.child(policy_button(
            s,
            format!("guide-quality-{quality}"),
            title,
            policy.quality == quality,
            next,
        ));
    }
    let mut order = choices();
    for (key, title, value) in [
        ("shuffle", "Shuffle", EpisodeOrder::Shuffle),
        ("sequential", "In order", EpisodeOrder::Sequential),
    ] {
        let mut next = policy.clone();
        next.order = value;
        order = order.child(policy_button(
            s,
            format!("guide-order-{key}"),
            title,
            policy.order == value,
            next,
        ));
    }
    let mut episodes = choices();
    for (key, title, value) in [
        ("all", "All released", false),
        ("watched", "Watched only", true),
    ] {
        let mut next = policy.clone();
        next.watched_only = value;
        episodes = episodes.child(policy_button(
            s,
            format!("guide-episodes-{key}"),
            title,
            policy.watched_only == value,
            next,
        ));
    }
    let mut sources = choices();
    for (key, title, allow) in [
        ("direct", "Direct only", false),
        ("torrents", "Allow torrents", true),
    ] {
        let mut next = policy.clone();
        next.allow_torrents = allow;
        sources = sources.child(policy_button(
            s,
            format!("guide-sources-{key}"),
            title,
            policy.allow_torrents == allow,
            next,
        ));
    }
    card().key("guide-shared-settings").padding(12.0).gap(8.0)
        .child(label("Applies to every channel · playback starts automatically"))
        .child(option_row("guide-provider","Provider",provider_choices(s)))
        .child(option_row("guide-quality","Quality",qualities))
        .child(option_row("guide-programming","Programs",choices().gap(16.0).child(order).child(episodes)))
        .child(option_row("guide-sources","Sources",sources))
        .child(label("Quality is a preference. Torrents can upload data and expose your IP. Current programs stay in place when settings change."))
}

fn provider_choices(s: &State) -> El {
    let policy = &s.guide.data.policy;
    let mut any = policy.clone();
    any.provider.clear();
    let mut providers = choices().child(policy_button(
        s,
        "guide-provider-any".into(),
        "Auto",
        policy.provider.is_empty(),
        any,
    ));
    for (index, addon) in s
        .addons
        .iter()
        .filter(|addon| {
            !addon.disabled
                && addon.manifest.resources.iter().any(|resource| {
                    resource.as_str() == Some("stream")
                        || resource.get("name").and_then(serde_json::Value::as_str)
                            == Some("stream")
                })
        })
        .enumerate()
    {
        let name = &addon.manifest.name;
        let mut next = policy.clone();
        next.provider.clone_from(name);
        providers = providers.child(policy_button(
            s,
            format!("guide-provider-{index}"),
            name,
            policy.provider == *name,
            next,
        ));
    }
    providers
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
    if !s.guide.busy {
        s.guide_action(GuideAction::CreateCollection(s.guide.name.text.clone()));
    }
}
