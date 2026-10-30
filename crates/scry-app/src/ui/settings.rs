//! App preferences. Guide policies keep their existing storage and scope.
pub(super) mod streaming;
use super::player::subtitles::Slot;
use super::{
    State,
    design::tokens,
    primitives::{
        button::{Button, DropdownVariant, Size, dropdown_trigger, menu_choice},
        feedback::{self, Tone},
        form::{choices, option_row},
        pattern::{page, section, settings_group},
    },
};
use rune::{El, col};
use scry_library::guide::{EpisodeOrder, SourcePolicy};
use scry_playback::subtitles::{LANGUAGES, Preferences, SIZES, language_label};

#[derive(Default)]
pub(super) struct SettingsState {
    pub subtitle_preferences: Preferences,
    pub subtitles: super::player::subtitles::SettingsUi,
    pub streaming: streaming::SettingsUi,
}

pub(super) fn view(s: &State) -> El {
    let mut content = col()
        .w_full()
        .gap(tokens::layout::SECTION_GAP)
        .child(streaming::view(s))
        .child(subtitle_settings(s))
        .child(section(
            "guide-preferences",
            "Guide",
            "Applies to Guide channels. Published programs stay in place when settings change.",
            col()
                .gap(tokens::layout::SECTION_GAP)
                .child(source_settings(s))
                .child(programming_settings(s)),
        ));
    if let Some(error) = &s.guide.error {
        content = content.child(feedback::message(
            "guide-settings-error",
            "Cannot update Guide settings",
            error,
            Tone::Danger,
            rune::row(),
        ));
    }
    page("settings-page", "Settings", "", rune::row(), content)
}

struct SubtitleControls {
    sizes: El,
    styles: El,
    fonts: El,
    weight: El,
    position: El,
}

fn subtitle_controls(s: &State) -> SubtitleControls {
    let preferences = &s.settings.subtitle_preferences;
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
    SubtitleControls {
        sizes,
        styles,
        fonts,
        weight,
        position,
    }
}

fn subtitle_settings(s: &State) -> El {
    let SubtitleControls {
        sizes,
        styles,
        fonts,
        weight,
        position,
    } = subtitle_controls(s);
    let mut panel = settings_group(
        "subtitle-settings",
        vec![
            option_row(
                "subtitle-primary",
                "Primary language",
                "Select automatically when available. English is the default.",
                choices().child(language_trigger(s, Slot::Primary)),
            ),
            option_row(
                "subtitle-secondary",
                "Secondary language",
                "Show a second language at the top of the video. Off by default.",
                choices().child(language_trigger(s, Slot::Secondary)),
            ),
            option_row(
                "subtitle-default-size",
                "Text size",
                "Applies to both text subtitle tracks.",
                sizes,
            ),
            option_row(
                "subtitle-default-position",
                "Primary position",
                "Secondary subtitles start at the top.",
                position,
            ),
            option_row(
                "subtitle-font",
                "Font",
                "For plain text or when original styles are disabled.",
                fonts,
            ),
            option_row(
                "subtitle-weight",
                "Text weight",
                "Make plain-text subtitles easier to read.",
                weight,
            ),
            option_row(
                "subtitle-text-color",
                "Text color",
                "Choose a high-contrast text color.",
                color_choices(s, "text"),
            ),
            option_row(
                "subtitle-background-color",
                "Background",
                "A transparent or shaded box behind the text.",
                color_choices(s, "background"),
            ),
            option_row(
                "subtitle-outline-color",
                "Outline",
                "Separate the text from light or dark scenes.",
                color_choices(s, "outline"),
            ),
            option_row(
                "subtitle-ass-styles",
                "ASS / SSA styles",
                "Original styles preserve the track’s fonts, colors, and positions. Bitmap subtitles have a fixed appearance.",
                styles,
            ),
            Button::new("subtitle-default-reset", "Restore subtitle defaults")
                .enabled(!s.settings.subtitles.busy)
                .on_press(|s: &mut State| s.save_subtitle_preferences(Preferences::default())),
        ],
    );
    if let Some(error) = &s.settings.subtitles.error {
        panel = panel.child(feedback::message(
            "subtitle-settings-error",
            "Cannot save subtitle defaults",
            error,
            Tone::Danger,
            rune::row().child(
                Button::new("subtitle-settings-retry", "Retry saving")
                    .enabled(!s.settings.subtitles.busy)
                    .on_press(|s: &mut State| {
                        s.save_subtitle_preferences(s.settings.subtitle_preferences.clone());
                    }),
            ),
        ));
    }
    section(
        "subtitle-section",
        "Subtitles",
        "Defaults for new playback, including Guide. Player adjustments do not change these settings.",
        panel,
    )
}

fn subtitle_choice(
    s: &State,
    key: String,
    title: &str,
    selected: bool,
    preferences: Preferences,
) -> El {
    Button::new(key, title)
        .size(Size::Setting)
        .selected(selected)
        .enabled(!s.settings.subtitles.busy)
        .on_press(move |s: &mut State| s.save_subtitle_preferences(preferences.clone()))
        .grow(1.0)
}

fn color_choices(s: &State, kind: &'static str) -> El {
    let colors: &[(&str, &str)] = match kind {
        "text" => tokens::subtitle::TEXT_COLORS,
        "background" => tokens::subtitle::BACKGROUND_COLORS,
        _ => tokens::subtitle::OUTLINE_COLORS,
    };
    let mut options = choices();
    for (index, &(title, color)) in colors.iter().enumerate() {
        let mut next = s.settings.subtitle_preferences.clone();
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
        (
            "subtitle-primary-language",
            &s.settings.subtitle_preferences.primary,
        )
    } else {
        (
            "subtitle-secondary-language",
            &s.settings.subtitle_preferences.secondary,
        )
    };
    dropdown_trigger(
        key,
        language
            .as_deref()
            .map_or_else(|| "Off".into(), language_label),
        DropdownVariant::Framed,
    )
    .size(Size::Setting)
    .enabled(!s.settings.subtitles.busy)
    .on_press(move |s: &mut State| s.settings.subtitles.language_menu = Some(slot))
    .w_full()
}

pub(super) fn language_menu(s: &State) -> Option<El> {
    let slot = s
        .settings
        .subtitles
        .language_menu
        .filter(|_| s.navigation.page == super::Page::Settings)?;
    let anchor = if slot == Slot::Primary {
        "subtitle-primary-language"
    } else {
        "subtitle-secondary-language"
    };
    let current = if slot == Slot::Primary {
        &s.settings.subtitle_preferences.primary
    } else {
        &s.settings.subtitle_preferences.secondary
    };
    let menu = super::primitives::surface::dropdown(
        "subtitle-language-menu",
        anchor,
        280.0,
        300.0,
        |s: &mut State| s.settings.subtitles.language_menu = None,
    );
    let mut options = col().w_full().gap(4.0);
    for (code, title) in std::iter::once(("", "Off")).chain(LANGUAGES.iter().copied()) {
        let language = (!code.is_empty()).then(|| code.to_owned());
        options = options.child(menu_choice(
            format!("subtitle-language-{code}"),
            title,
            *current == language,
            move |s: &mut State| {
                let mut next = s.settings.subtitle_preferences.clone();
                if slot == Slot::Primary {
                    next.primary.clone_from(&language);
                } else {
                    next.secondary.clone_from(&language);
                }
                s.settings.subtitles.language_menu = None;
                s.save_subtitle_preferences(next);
            },
        ));
    }
    Some(menu.child(options))
}

fn policy_button(s: &State, key: String, title: &str, selected: bool, policy: SourcePolicy) -> El {
    Button::new(key, title)
        .size(Size::Setting)
        .selected(selected)
        .enabled(!s.guide.busy)
        .on_press(move |s: &mut State| {
            s.update_guide_policy(policy.clone());
        })
        .w(0.0)
        .grow(1.0)
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
    section(
        "guide-source-section",
        "Sources",
        "Choose how Guide finds streams for your channels.",
        settings_group(
            "guide-source-settings",
            [
                option_row(
                    "guide-provider",
                    "Preferred provider",
                    "Auto uses any enabled stream provider. Manage providers on the Add-ons page.",
                    provider_choices(s),
                ),
                option_row(
                    "guide-quality",
                    "Preferred quality",
                    "Prefer this resolution when available. Quality is not guaranteed.",
                    qualities,
                ),
                option_row(
                    "guide-sources",
                    "Stream types",
                    "Torrents can upload data and expose your IP. Direct only keeps torrents off.",
                    sources,
                ),
            ],
        ),
    )
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
    section(
        "guide-programming-section",
        "Programming",
        "Set the rules for future channel schedules.",
        settings_group(
            "guide-programming-settings",
            [
                option_row(
                    "guide-order",
                    "Episode order",
                    "Shuffle mixes episodes. In order follows the episode sequence.",
                    order,
                ),
                option_row(
                    "guide-episodes",
                    "Episode eligibility",
                    "Include all released episodes, or only episodes you have already watched.",
                    episodes,
                ),
            ],
        ),
    )
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
        .installed
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
