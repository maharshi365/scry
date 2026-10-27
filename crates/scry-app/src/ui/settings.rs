//! App preferences. Guide policies keep their existing storage and scope.
use super::player::subtitles::Slot;
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
use scry_playback::subtitles::{LANGUAGES, Preferences, SIZES, language_label};

pub(super) fn view(s: &State) -> El {
    let mut content = col()
        .w_full()
        .gap(24.0)
        .child(heading("Player"))
        .child(subtitle_settings(s))
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

fn subtitle_settings(s: &State) -> El {
    let preferences = &s.subtitle_preferences;
    let mut sizes = choices();
    for &size in SIZES {
        let mut next = preferences.clone();
        next.size = size;
        sizes = sizes.child(subtitle_choice(
            s,
            format!("subtitle-default-size-{size}"),
            &format!("{size}%"),
            preferences.size == size,
            next,
        ));
    }
    let mut styles = choices();
    for (title, value) in [("Original styles", true), ("Use my appearance", false)] {
        let mut next = preferences.clone();
        next.ass_styles = value;
        styles = styles.child(subtitle_choice(
            s,
            format!("subtitle-ass-{value}"),
            title,
            preferences.ass_styles == value,
            next,
        ));
    }
    let mut fonts = choices();
    for font in ["sans-serif", "serif", "monospace", "Arial", "Verdana"] {
        let mut next = preferences.clone();
        next.font = font.into();
        fonts = fonts.child(subtitle_choice(
            s,
            format!("subtitle-font-{font}"),
            font,
            preferences.font == font,
            next,
        ));
    }
    let mut weight = choices();
    for (title, bold) in [("Regular", false), ("Bold", true)] {
        let mut next = preferences.clone();
        next.bold = bold;
        weight = weight.child(subtitle_choice(
            s,
            format!("subtitle-bold-{bold}"),
            title,
            preferences.bold == bold,
            next,
        ));
    }
    let mut position = choices();
    for (title, value) in [("Bottom", 95), ("Raised", 85), ("Center", 50)] {
        let mut next = preferences.clone();
        next.position = value;
        position = position.child(subtitle_choice(
            s,
            format!("subtitle-default-position-{value}"),
            title,
            preferences.position == value,
            next,
        ));
    }
    let mut panel = card().key("subtitle-settings").padding(24.0).gap(0.0)
        .child(section_header("Subtitles", "Defaults for new playback, including Guide. Player adjustments do not change these settings."))
        .child(divider())
        .child(option_row("subtitle-primary", "Primary language", "Select automatically when available. English is the default.", choices().child(language_trigger(s, Slot::Primary))))
        .child(divider())
        .child(option_row("subtitle-secondary", "Secondary language", "Show a second language at the top of the video. Off by default.", choices().child(language_trigger(s, Slot::Secondary))))
        .child(divider())
        .child(option_row("subtitle-default-size", "Text size", "Applies to both text subtitle tracks.", sizes))
        .child(divider())
        .child(option_row("subtitle-default-position", "Primary position", "Secondary subtitles start at the top.", position))
        .child(divider())
        .child(option_row("subtitle-font", "Font", "For plain text or when original styles are disabled.", fonts))
        .child(divider())
        .child(option_row("subtitle-weight", "Text weight", "Make plain-text subtitles easier to read.", weight))
        .child(divider())
        .child(option_row("subtitle-text-color", "Text color", "Choose a high-contrast text color.", color_choices(s, "text")))
        .child(divider())
        .child(option_row("subtitle-background-color", "Background", "A transparent or shaded box behind the text.", color_choices(s, "background")))
        .child(divider())
        .child(option_row("subtitle-outline-color", "Outline", "Separate the text from light or dark scenes.", color_choices(s, "outline")))
        .child(divider())
        .child(option_row("subtitle-ass-styles", "ASS / SSA styles", "Original styles preserve the track’s fonts, colors, and positions. Bitmap subtitles have a fixed appearance.", styles))
        .child(divider())
        .child(Button::new("subtitle-default-reset", "Restore subtitle defaults").enabled(!s.subtitle_settings.busy)
            .on_press(|s: &mut State| s.save_subtitle_preferences(Preferences::default())));
    if let Some(error) = &s.subtitle_settings.error {
        panel = panel.child(label(error.clone())).child(
            Button::new("subtitle-settings-retry", "Retry saving")
                .enabled(!s.subtitle_settings.busy)
                .on_press(|s: &mut State| {
                    s.save_subtitle_preferences(s.subtitle_preferences.clone());
                }),
        );
    }
    panel
}

fn subtitle_choice(
    s: &State,
    key: String,
    title: &str,
    selected: bool,
    preferences: Preferences,
) -> El {
    Button::new(key, title)
        .selected(selected)
        .enabled(!s.subtitle_settings.busy)
        .on_press(move |s: &mut State| s.save_subtitle_preferences(preferences.clone()))
        .min_w(64.0)
        .grow(1.0)
        .h(46.0)
        .radius(8.0)
}

fn color_choices(s: &State, kind: &'static str) -> El {
    let colors: &[(&str, &str)] = match kind {
        "text" => tokens::subtitle::TEXT_COLORS,
        "background" => tokens::subtitle::BACKGROUND_COLORS,
        _ => tokens::subtitle::OUTLINE_COLORS,
    };
    let mut options = choices();
    for (index, &(title, color)) in colors.iter().enumerate() {
        let mut next = s.subtitle_preferences.clone();
        let target = match kind {
            "text" => &mut next.text_color,
            "background" => &mut next.background_color,
            _ => &mut next.outline_color,
        };
        let selected = target == color;
        *target = color.into();
        options = options.child(subtitle_choice(
            s,
            format!("subtitle-color-{kind}-{index}"),
            title,
            selected,
            next,
        ));
    }
    options
}

fn language_trigger(s: &State, slot: Slot) -> El {
    let (key, language) = if slot == Slot::Primary {
        ("subtitle-primary-language", &s.subtitle_preferences.primary)
    } else {
        (
            "subtitle-secondary-language",
            &s.subtitle_preferences.secondary,
        )
    };
    Button::new(
        key,
        language
            .as_deref()
            .map_or_else(|| "Off".into(), language_label),
    )
    .enabled(!s.subtitle_settings.busy)
    .on_press(move |s: &mut State| s.subtitle_settings.language_menu = Some(slot))
    .w_full()
    .h(46.0)
    .radius(8.0)
}

pub(super) fn language_menu(s: &State) -> Option<El> {
    let slot = s
        .subtitle_settings
        .language_menu
        .filter(|_| s.navigation.page == super::Page::Settings)?;
    let anchor = if slot == Slot::Primary {
        "subtitle-primary-language"
    } else {
        "subtitle-secondary-language"
    };
    let current = if slot == Slot::Primary {
        &s.subtitle_preferences.primary
    } else {
        &s.subtitle_preferences.secondary
    };
    let menu = super::primitives::surface::dropdown(
        "subtitle-language-menu",
        anchor,
        280.0,
        300.0,
        |s: &mut State| s.subtitle_settings.language_menu = None,
    );
    let mut options = col().w_full().gap(4.0);
    for (code, title) in std::iter::once(("", "Off")).chain(LANGUAGES.iter().copied()) {
        let language = (!code.is_empty()).then(|| code.to_owned());
        options = options.child(
            Button::new(format!("subtitle-language-{code}"), title)
                .selected(*current == language)
                .on_press(move |s: &mut State| {
                    let mut next = s.subtitle_preferences.clone();
                    if slot == Slot::Primary {
                        next.primary.clone_from(&language);
                    } else {
                        next.secondary.clone_from(&language);
                    }
                    s.subtitle_settings.language_menu = None;
                    s.save_subtitle_preferences(next);
                })
                .w_full()
                .min_h(38.0)
                .justify_start(),
        );
    }
    Some(menu.child(options))
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
