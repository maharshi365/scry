//! App preferences. Guide policies keep their existing storage and scope.
use super::{
    State,
    design::tokens::{self, type_},
    primitives::{
        button::Button,
        surface::{card, scroll},
        text::{heading, label},
    },
};
use rune::{El, col, el, row, text};
use scry_library::guide::{EpisodeOrder, SourcePolicy};

pub(super) fn view(s: &State) -> El {
    let mut content = col()
        .w_full()
        .gap(24.0)
        .child(super::local_media::settings(s))
        .child(heading("Guide"))
        .child(label("Applies to every Guide channel, not regular playback. Published programs stay in place when settings change."))
        .child(source_settings(s))
        .child(programming_settings(s));
    if let Some(error) = &s.guide.error {
        content = content.child(label(error.clone()));
    }
    col()
        .key("settings-page")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(16.0)
        .child(heading("Settings"))
        .child(scroll("settings-scroll", content))
}

fn choices() -> El {
    row().min_w(0.0).gap(8.0).center_y().flex_wrap()
}

fn option_row(key: &str, title: &str, description: &str, options: El) -> El {
    row()
        .key(format!("{key}-row"))
        .gap(24.0)
        .py(20.0)
        .center_y()
        .flex_wrap()
        .w_full()
        .min_w(0.0)
        .child(
            col()
                .w(0.0)
                .min_w(200.0)
                .grow(1.0)
                .gap(6.0)
                .child(
                    text(title)
                        .key(format!("{key}-label"))
                        .wrap()
                        .font(type_::ITEM_TITLE)
                        .color(tokens::theme().text),
                )
                .child(label(description).key(format!("{key}-description"))),
        )
        .child(
            options
                .key(format!("{key}-choices"))
                .w(0.0)
                .min_w(280.0)
                .grow(1.0),
        )
}

fn divider() -> El {
    el().w_full().h(1.0).fill(tokens::subtle_edge())
}

fn section_header(title: &str, description: &str) -> El {
    col()
        .w_full()
        .gap(6.0)
        .pb(20.0)
        .child(
            text(title)
                .font(type_::SECTION_TITLE)
                .color(tokens::theme().text),
        )
        .child(label(description))
}

fn policy_button(s: &State, key: String, title: &str, selected: bool, policy: SourcePolicy) -> El {
    Button::new(key, title)
        .selected(selected)
        .enabled(!s.guide.busy)
        .on_press(move |s: &mut State| {
            s.update_guide_policy(policy.clone());
        })
        .w(0.0)
        .min_w(64.0)
        .grow(1.0)
        .h(46.0)
        .radius(8.0)
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
    card()
        .key("guide-source-settings")
        .padding(24.0)
        .gap(0.0)
        .child(section_header(
            "Sources",
            "Choose how Guide finds streams for your channels.",
        ))
        .child(divider())
        .child(option_row(
            "guide-provider",
            "Preferred provider",
            "Auto uses any enabled stream provider. Manage providers on the Add-ons page.",
            provider_choices(s),
        ))
        .child(divider())
        .child(option_row(
            "guide-quality",
            "Preferred quality",
            "Prefer this resolution when available. Quality is not guaranteed.",
            qualities,
        ))
        .child(divider())
        .child(option_row(
            "guide-sources",
            "Stream types",
            "Torrents can upload data and expose your IP. Direct only keeps torrents off.",
            sources,
        ))
}

fn programming_settings(s: &State) -> El {
    let policy = &s.guide.data.policy;
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
    card()
        .key("guide-programming-settings")
        .padding(24.0)
        .gap(0.0)
        .child(section_header(
            "Programming",
            "Set the rules for future channel schedules.",
        ))
        .child(divider())
        .child(option_row(
            "guide-order",
            "Episode order",
            "Shuffle mixes episodes. In order follows the episode sequence.",
            order,
        ))
        .child(divider())
        .child(option_row(
            "guide-episodes",
            "Episode eligibility",
            "Include all released episodes, or only episodes you have already watched.",
            episodes,
        ))
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
        providers = providers.child(
            policy_button(
                s,
                format!("guide-provider-{index}"),
                name,
                policy.provider == *name,
                next,
            )
            .min_w(120.0),
        );
    }
    providers
}
