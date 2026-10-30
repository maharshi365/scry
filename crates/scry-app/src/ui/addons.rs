//! Built-in providers install directly; custom URLs use the installation dialog.
use super::{
    State,
    app::on_input_key,
    design::{icons, tokens},
    primitives::{
        button::{Button, Size, Variant},
        dialog,
        feedback::{self, Tone},
        input::dialog_input,
        pattern::detail_header,
        status::chip,
        surface::scroll,
        text::label,
    },
};
use crate::services::{CINEMETA, KITSU};
use rune::{El, col, el, icon, row, text};

pub(super) struct AddonsState {
    pub error: Option<String>,
    pub status: String,
    pub installed: Vec<scry_addons::Addon>,
    pub busy: bool,
    pub pending: Option<AddonAction>,
    pub install: rune::TextEdit,
    pub direct: rune::TextEdit,
    pub install_open: bool,
    pub install_complete: bool,
    pub layout: AddonLayout,
}

impl Default for AddonsState {
    fn default() -> Self {
        Self {
            error: None,
            status: String::new(),
            installed: Vec::new(),
            busy: true,
            pending: None,
            install: rune::TextEdit::new(""),
            direct: rune::TextEdit::new(""),
            install_open: false,
            install_complete: false,
            layout: AddonLayout::default(),
        }
    }
}

pub(super) enum AddonAction {
    Install(String),
    Toggle(String),
    Remove(String),
}

struct Supported {
    id: &'static str,
    name: &'static str,
    url: &'static str,
    description: &'static str,
    logo: Option<&'static str>,
}

const SUPPORTED: [Supported; 3] = [
    Supported {
        id: "com.linvo.cinemeta",
        name: "Cinemeta",
        url: CINEMETA,
        description: "Movie and TV catalogs, details, and release dates.",
        logo: None,
    },
    Supported {
        id: "community.anime.kitsu",
        name: "Anime Kitsu",
        url: KITSU,
        description: "Anime catalogs, search, and episode details.",
        logo: Some("https://i.imgur.com/7N6XGoO.png"),
    },
    Supported {
        id: "com.stremio.torrentio.addon",
        name: "Torrentio",
        url: "https://torrentio.strem.fun/manifest.json",
        description: "Torrent streams and configured debrid providers.",
        logo: Some("https://torrentio.strem.fun/images/logo_v1.png"),
    },
];

#[derive(Default)]
pub(super) struct AddonLayout {
    rects: std::collections::BTreeMap<usize, [f32; 4]>,
    viewport: Option<[f32; 4]>,
}

fn available(s: &State) -> impl Iterator<Item = (usize, &Supported)> {
    SUPPORTED.iter().enumerate().filter(|(_, provider)| {
        !s.addons
            .installed
            .iter()
            .any(|addon| addon.manifest.id == provider.id)
    })
}

pub(super) fn artwork_urls(s: &State) -> Vec<String> {
    let installed = s
        .addons
        .installed
        .iter()
        .enumerate()
        .map(|(index, addon)| (index, addon.manifest.logo.as_deref()));
    let available =
        available(s).map(|(index, provider)| (s.addons.installed.len() + index, provider.logo));
    let mut urls = Vec::new();
    for (index, logo) in installed.chain(available) {
        let visible = s.addons.layout.rects.get(&index).is_none_or(|rect| {
            s.addons
                .layout
                .viewport
                .is_none_or(|viewport| super::artwork::intersects(*rect, viewport))
        });
        if visible
            && let Some(url) = logo
            && !url.trim().is_empty()
            && !urls.iter().any(|value| value == url)
        {
            urls.push(url.to_owned());
        }
        if urls.len() == 32 {
            break;
        }
    }
    urls
}

fn open_install(s: &mut State, url: &str) {
    s.addons.install = rune::TextEdit::new(url);
    s.addons.error = None;
    s.addons.install_complete = false;
    s.addons.install_open = true;
}

impl State {
    pub(super) fn install(&mut self, input: String) {
        if self.addons.busy {
            return;
        }
        let Some(services) = &self.runtime.services else {
            self.addons.error =
                Some("Add-on services are unavailable. Restart the app to retry.".into());
            return;
        };
        self.addons.error = None;
        self.addons.busy = true;
        self.addons.pending = Some(AddonAction::Install(input.clone()));
        self.addons.install_complete = false;
        services.install(input);
    }
}

pub(super) fn addons(s: &State) -> El {
    let mut list = el()
        .key("addons-grid")
        .equal_columns(4)
        .w_full()
        .gap(tokens::layout::PAGE_GAP);
    for (slot, provider) in SUPPORTED.iter().enumerate() {
        let card = if let Some((index, addon)) = s
            .addons
            .installed
            .iter()
            .enumerate()
            .find(|(_, addon)| addon.manifest.id == provider.id)
        {
            installed_card(s, index, addon, provider.description)
        } else {
            let url = provider.url;
            provider_card(
                s,
                s.addons.installed.len() + slot,
                provider.name,
                provider.description,
                "Available",
                provider.logo,
            )
            .key(format!("available-addon-{slot}"))
            .child(
                Button::new(format!("supported-addon-{slot}"), "Install")
                    .enabled(!s.addons.busy)
                    .busy(matches!(s.addons.pending.as_ref(), Some(AddonAction::Install(input)) if input == url))
                    .on_press(move |s: &mut State| s.install(url.to_owned()))
                    .w_full(),
            )
        };
        list = list.child(
            col()
                .key(format!("addon-slot-{slot}"))
                .min_w(0.0)
                .child(card),
        );
    }
    for (index, addon) in s.addons.installed.iter().enumerate().filter(|(_, addon)| {
        !SUPPORTED
            .iter()
            .any(|provider| provider.id == addon.manifest.id)
    }) {
        list = list.child(installed_card(
            s,
            index,
            addon,
            &format!(
                "{} catalogs · Custom provider",
                addon.manifest.catalogs.len()
            ),
        ));
    }
    col()
        .key("addons-page")
        .gap(tokens::layout::PAGE_GAP)
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .child(detail_header(
            "addons-header",
            "Add-ons",
            &format!(
                "{} installed · {} enabled",
                s.addons.installed.len(),
                s.addons.installed.iter().filter(|a| !a.disabled).count()
            ),
            row().child(
                Button::new("add-addon", "Add by URL")
                    .variant(Variant::Primary)
                    .enabled(!s.addons.busy)
                    .on_press(|s| open_install(s, "")),
            ),
        ))
        .child(
            col().h(tokens::addon::NOTICE_HEIGHT).child(
                if let Some(error) = &s.addons.error {
                    feedback::status("addons-error", error, Tone::Danger)
                } else {
                    label("Your providers, all in one place. Install a provider or add your own manifest URL.")
                },
            ),
        )
        .child(
            scroll("addons-scroll", list).on_layout(|s: &mut State, rect| {
                s.addons.layout.viewport = Some(rect);
                s.refresh_artwork();
            }),
        )
}

fn installed_card(s: &State, index: usize, addon: &scry_addons::Addon, description: &str) -> El {
    let id = addon.manifest.id.clone();
    let remove_id = id.clone();
    let disabled = addon.disabled;
    let toggling =
        matches!(s.addons.pending.as_ref(), Some(AddonAction::Toggle(pending)) if pending == &id);
    let removing = matches!(s.addons.pending.as_ref(), Some(AddonAction::Remove(pending)) if pending == &remove_id);
    provider_card(
        s,
        index,
        &addon.manifest.name,
        description,
        if disabled { "Disabled" } else { "Enabled" },
        addon.manifest.logo.as_deref(),
    )
    .key(format!("installed-addon-{index}"))
    .child(
        row()
            .w_full()
            .gap(tokens::layout::ACTION_GAP)
            .child(el().grow(1.0))
            .child(
                Button::new(
                    format!("toggle-addon-{index}"),
                    if disabled { "Enable" } else { "Disable" },
                )
                .icon(if disabled {
                    &icons::PLAY
                } else {
                    &icons::PAUSE
                })
                .enabled(!s.addons.busy)
                .busy(toggling)
                .on_press(move |s: &mut State| {
                    if !s.addons.busy
                        && let Some(services) = &s.runtime.services
                    {
                        s.addons.error = None;
                        s.addons.pending = Some(AddonAction::Toggle(id.clone()));
                        services.set_addon_disabled(id.clone(), !disabled);
                        s.addons.busy = true;
                    }
                }),
            )
            .child(
                Button::new(format!("remove-{index}"), "Remove")
                    .icon(&icons::TRASH)
                    .variant(Variant::Danger)
                    .enabled(!s.addons.busy)
                    .busy(removing)
                    .on_press(move |s: &mut State| {
                        if !s.addons.busy
                            && let Some(services) = &s.runtime.services
                        {
                            s.addons.error = None;
                            s.addons.pending = Some(AddonAction::Remove(remove_id.clone()));
                            services.remove(remove_id.clone());
                            s.addons.busy = true;
                        }
                    }),
            ),
    )
}

fn provider_card(
    s: &State,
    index: usize,
    name: &str,
    description: &str,
    status: &str,
    logo: Option<&str>,
) -> El {
    let tile = super::primitives::status::provider_tile();
    let tile = if let Some(image) = logo.and_then(|url| s.artwork.posters.as_ref()?.get(url)) {
        tile.child(
            el().key(format!("addon-logo-{index}"))
                .role("img")
                .label(format!("{name} logo"))
                .w(40.0)
                .h(40.0)
                .pixels_fit(&image, rune::SurfaceFit::Fit),
        )
    } else {
        tile.child(
            icon(&icons::ADDONS, 28.0)
                .key(format!("addon-logo-fallback-{index}"))
                .color(tokens::accent()),
        )
    };
    super::primitives::surface::Variant::Provider
        .apply(col())
        .w_full()
        .min_w(0.0)
        .h(tokens::addon::TILE_HEIGHT)
        .min_h(tokens::addon::TILE_HEIGHT)
        .padding(tokens::layout::PANEL_INSET)
        .gap(tokens::layout::PAGE_GAP)
        .on_layout(move |s: &mut State, rect| {
            s.addons.layout.rects.insert(index, rect);
            s.refresh_artwork();
        })
        .child(
            row()
                .w_full()
                .center_y()
                .gap(tokens::layout::COPY_GAP)
                .child(tile)
                .child(el().grow(1.0))
                .child(chip(status)),
        )
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(7.0)
                .child(
                    text(name)
                        .font(tokens::type_::ITEM_TITLE)
                        .color(tokens::theme().text)
                        .truncate(),
                )
                .child(label(description.to_owned())),
        )
}

fn install_feedback(s: &State) -> El {
    if let Some(error) = &s.addons.error {
        feedback::status("addon-install-error", error, Tone::Danger)
    } else if s.addons.install_complete {
        row()
            .gap(tokens::layout::ACTION_GAP)
            .center_y()
            .child(
                icon(&icons::CHECK, tokens::control::ICON_SIZE)
                    .key("addon-install-success")
                    .color(tokens::success()),
            )
            .child(feedback::status(
                "addon-installed",
                "Add-on installed successfully.",
                Tone::Success,
            ))
    } else {
        label("Only install add-ons from providers you trust. Your manifest URL is kept private.")
    }
}

pub(super) fn install_dialog(s: &State) -> El {
    let submit = if s.addons.install_complete {
        Button::new("addon-install-done", "Done")
            .variant(Variant::Primary)
            .size(Size::DialogAction)
            .on_press(|s: &mut State| s.addons.install_open = false)
    } else {
        Button::new("install-addon", "Install")
            .variant(Variant::Primary)
            .size(Size::DialogAction)
            .busy(s.addons.busy)
            .on_press(|s: &mut State| s.install(s.addons.install.text.clone()))
    };
    dialog::layer(
        "addon-dialog-layer",
        dialog::panel("addon-install-dialog", |s: &mut State| {
            s.addons.install_open = false;
        })
        .w(tokens::addon::DIALOG_WIDTH)
        .padding(tokens::layout::PANEL_INSET)
        .gap(tokens::layout::SECTION_GAP)
        .child(
            row()
                .w_full()
                .center_y()
                .gap(tokens::layout::PAGE_GAP)
                .child(super::primitives::status::provider_tile().child(
                    icon(&icons::DOWNLOAD, tokens::badge::MEDALLION_ICON).color(tokens::accent()),
                ))
                .child(
                    dialog::header("Add by URL")
                        .w(0.0)
                        .min_w(0.0)
                        .grow(1.0)
                        .child(dialog::close(
                            "addon-dialog-close",
                            "Close install dialog",
                            |s: &mut State| s.addons.install_open = false,
                        )),
                ),
        )
        .child(label(
            "Connect a custom provider using its Stremio-compatible manifest URL.",
        ))
        .child(
            col()
                .w_full()
                .gap(tokens::layout::ACTION_GAP)
                .child(label("Manifest URL"))
                .child(
                    dialog_input(
                        "manifest",
                        &s.addons.install,
                        "https://provider.example/manifest.json",
                        |s| &mut s.addons.install,
                        on_input_key,
                        |s| {
                            if !s.addons.install_complete {
                                s.install(s.addons.install.text.clone());
                            }
                        },
                    )
                    .grow(0.0)
                    .w_full(),
                ),
        )
        .child(
            col()
                .key("addon-install-feedback")
                .w_full()
                .h(tokens::addon::FEEDBACK_HEIGHT)
                .child(install_feedback(s)),
        )
        .child(super::primitives::surface::divider())
        .child(
            row()
                .w_full()
                .gap(tokens::layout::ACTION_GAP)
                .center_y()
                .child(el().grow(1.0))
                .child(
                    Button::new("addon-install-cancel", "Close")
                        .size(Size::DialogAction)
                        .on_press(|s: &mut State| s.addons.install_open = false),
                )
                .child(submit),
        ),
    )
}
