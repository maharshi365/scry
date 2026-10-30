//! Built-in providers install directly; custom URLs use the installation dialog.
use super::{
    State,
    app::on_input_key,
    design::{icons, tokens},
    primitives::{
        button::{Button, Size, Variant},
        dialog,
        input::dialog_input,
        pattern::detail_header,
        status::spinner,
        surface::scroll,
        text::{heading, label},
    },
};
use crate::services::{CINEMETA, KITSU};
use rune::{El, col, el, icon, row, text};

pub(super) struct AddonsState {
    pub error: Option<String>,
    pub status: String,
    pub installed: Vec<scry_addons::Addon>,
    pub busy: bool,
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
            install: rune::TextEdit::new(""),
            direct: rune::TextEdit::new(""),
            install_open: false,
            install_complete: false,
            layout: AddonLayout::default(),
        }
    }
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
        description: "Movie and TV series catalogs, metadata, and episode release dates.",
        logo: None,
    },
    Supported {
        id: "community.anime.kitsu",
        name: "Anime Kitsu",
        url: KITSU,
        description: "Anime catalogs, search, metadata, and episodes using Kitsu IDs.",
        logo: Some("https://i.imgur.com/7N6XGoO.png"),
    },
    Supported {
        id: "com.stremio.torrentio.addon",
        name: "Torrentio",
        url: "https://torrentio.strem.fun/manifest.json",
        description: "Streams from direct torrents or a configured debrid provider.",
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
        services.install(input);
    }
}

pub(super) fn addons(s: &State) -> El {
    let mut list = col().w_full().gap(16.0);
    for (index, addon) in s.addons.installed.iter().enumerate() {
        let id = addon.manifest.id.clone();
        let remove_id = id.clone();
        let disabled = addon.disabled;
        list = list.child(
            provider_card(
                s,
                index,
                &addon.manifest.name,
                &format!(
                    "{} catalogs · Manifest URL hidden",
                    addon.manifest.catalogs.len()
                ),
                if disabled { "Disabled" } else { "Enabled" },
                addon.manifest.logo.as_deref(),
            )
            .key(format!("installed-addon-{index}"))
            .child(
                Button::new(
                    format!("toggle-addon-{index}"),
                    if disabled { "Enable" } else { "Disable" },
                )
                .enabled(!s.addons.busy)
                .on_press(move |s: &mut State| {
                    if !s.addons.busy
                        && let Some(services) = &s.runtime.services
                    {
                        s.addons.error = None;
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
                    .on_press(move |s: &mut State| {
                        if !s.addons.busy
                            && let Some(services) = &s.runtime.services
                        {
                            s.addons.error = None;
                            services.remove(remove_id.clone());
                            s.addons.busy = true;
                        }
                    }),
            ),
        );
    }
    for (index, provider) in available(s) {
        let url = provider.url;
        list = list.child(
            provider_card(
                s,
                s.addons.installed.len() + index,
                provider.name,
                provider.description,
                "Available",
                provider.logo,
            )
            .child(
                Button::new(format!("supported-addon-{index}"), "Install")
                    .enabled(!s.addons.busy)
                    .on_press(move |s: &mut State| s.install(url.to_owned())),
            ),
        );
    }
    col()
        .key("addons-page")
        .gap(tokens::layout::PAGE_GAP)
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .child(
            detail_header("addons-header", "Add-ons",
                &format!("{} installed · {} enabled", s.addons.installed.len(), s.addons.installed.iter().filter(|a| !a.disabled).count()),
                row().child(Button::new("add-addon", "Add by URL")
                    .variant(Variant::Primary)
                    .enabled(!s.addons.busy)
                    .on_press(|s| open_install(s, "")))),
        )
        .child(label("Browse available providers and manage installed add-ons in one place. Disable an add-on to keep its configuration without using it."))
        .child(scroll("addons-scroll", list).on_layout(|s: &mut State, rect| {
            s.addons.layout.viewport = Some(rect);
            s.refresh_artwork();
        }))
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
        .apply(row())
        .w_full()
        .padding(tokens::layout::PANEL_INSET)
        .gap(tokens::layout::PAGE_GAP)
        .center_y()
        .on_layout(move |s: &mut State, rect| {
            s.addons.layout.rects.insert(index, rect);
            s.refresh_artwork();
        })
        .child(tile)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(7.0)
                .child(heading(name.to_owned()))
                .child(label(description.to_owned()))
                .child(label(status.to_owned())),
        )
}

pub(super) fn install_dialog(s: &State) -> El {
    let mut feedback = col().w_full().gap(10.0);
    if let Some(error) = &s.addons.error {
        feedback = feedback.child(label(error.clone()));
    }
    let mut submit = Button::new("install-addon", "Install")
        .variant(Variant::Primary)
        .size(Size::Dialog)
        .busy(s.addons.busy)
        .frame();
    if s.addons.install_complete {
        feedback = feedback.child(label("Add-on installed successfully."));
        submit = Button::new("addon-install-done", "Done")
            .on_press(|s: &mut State| s.addons.install_open = false);
    } else if s.addons.busy {
        submit = submit.child(spinner("addon-install-spinner", 24.0));
    } else {
        submit = submit
            .child(icon(&icons::DOWNLOAD, 24.0).color(tokens::accent()))
            .on_press(|s: &mut State| s.install(s.addons.install.text.clone()));
    }
    if s.addons.install_complete {
        feedback = feedback.child(
            icon(&icons::CHECK, 28.0)
                .key("addon-install-success")
                .color(tokens::accent()),
        );
    }
    dialog::layer(
        "addon-dialog-layer",
        dialog::panel("addon-install-dialog", |s: &mut State| {
            s.addons.install_open = false;
        })
        .w(640.0)
        .padding(tokens::layout::PANEL_INSET)
        .gap(tokens::layout::PAGE_GAP)
        .child(dialog::header("Install add-on").child(dialog::close(
            "addon-dialog-close",
            "Close install dialog",
            |s: &mut State| s.addons.install_open = false,
        )))
        .child(label("Manifest URL"))
        .child(
            dialog_input(
                "manifest",
                &s.addons.install,
                "http://…/manifest.json or https://…/manifest.json",
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
        )
        .child(feedback)
        .child(
            row().gap(12.0).center_y().child(submit).child(
                text(if s.addons.error.is_some() {
                    "Check the URL and try again."
                } else {
                    ""
                })
                .font(tokens::type_::LABEL)
                .color(tokens::theme().text),
            ),
        ),
    )
}
