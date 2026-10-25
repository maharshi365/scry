//! Available and installed providers, with one shared installation dialog.
use super::{
    State,
    controls::input,
    design::{icons, tokens},
    primitives::{
        button::{Button, Variant},
        dialog,
        status::spinner,
        surface::{scroll, surface},
        text::{heading, label},
    },
};
use crate::services::CINEMETA;
use rune::{El, col, el, icon, row, text};

struct Supported {
    id: &'static str,
    name: &'static str,
    url: &'static str,
    description: &'static str,
    logo: Option<&'static str>,
}

const SUPPORTED: [Supported; 2] = [
    Supported {
        id: "com.linvo.cinemeta",
        name: "Cinemeta",
        url: CINEMETA,
        description: "Movie and TV series catalogs, metadata, and episode release dates.",
        logo: None,
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
            .iter()
            .any(|addon| addon.manifest.id == provider.id)
    })
}

pub(super) fn artwork_urls(s: &State) -> Vec<String> {
    let installed = s
        .addons
        .iter()
        .enumerate()
        .map(|(index, addon)| (index, addon.manifest.logo.as_deref()));
    let available = available(s).map(|(index, provider)| (s.addons.len() + index, provider.logo));
    let mut urls = Vec::new();
    for (index, logo) in installed.chain(available) {
        let visible = s.addon_layout.rects.get(&index).is_none_or(|rect| {
            s.addon_layout
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
    s.install = rune::TextEdit::new(url);
    s.error = None;
    s.addon_install_complete = false;
    s.addon_install_open = true;
}

pub(super) fn addons(s: &State) -> El {
    let mut list = col().w_full().gap(16.0);
    for (index, addon) in s.addons.iter().enumerate() {
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
                .enabled(!s.config_busy)
                .on_press(move |s: &mut State| {
                    if !s.config_busy
                        && let Some(services) = &s.services
                    {
                        s.error = None;
                        services.set_addon_disabled(id.clone(), !disabled);
                        s.config_busy = true;
                    }
                }),
            )
            .child(
                Button::new(format!("remove-{index}"), "Remove")
                    .icon(&icons::TRASH)
                    .variant(Variant::Danger)
                    .enabled(!s.config_busy)
                    .on_press(move |s: &mut State| {
                        if !s.config_busy
                            && let Some(services) = &s.services
                        {
                            s.error = None;
                            services.remove(remove_id.clone());
                            s.config_busy = true;
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
                s.addons.len() + index,
                provider.name,
                provider.description,
                "Available",
                provider.logo,
            )
            .child(
                Button::new(format!("supported-addon-{index}"), "Install")
                    .variant(Variant::Primary)
                    .enabled(!s.config_busy)
                    .on_press(move |s| open_install(s, url)),
            ),
        );
    }
    col()
        .key("addons-page")
        .gap(20.0)
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .child(
            row().w_full().gap(16.0).center_y()
                .child(col().grow(1.0).gap(6.0)
                    .child(heading("Add-ons"))
                    .child(label(format!("{} installed · {} enabled", s.addons.len(), s.addons.iter().filter(|a| !a.disabled).count()))))
                .child(Button::new("add-addon", "Add by URL")
                    .enabled(!s.config_busy)
                    .on_press(|s| open_install(s, ""))),
        )
        .child(label("Browse available providers and manage installed add-ons in one place. Disable an add-on to keep its configuration without using it."))
        .child(scroll("addons-scroll", list).on_layout(|s: &mut State, rect| {
            s.addon_layout.viewport = Some(rect);
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
    let tile = el()
        .w(56.0)
        .h(56.0)
        .radius(14.0)
        .fill(tokens::selected())
        .center();
    let tile = if let Some(image) = logo.and_then(|url| s.artwork.as_ref()?.get(url)) {
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
    surface(row())
        .w_full()
        .padding(24.0)
        .radius(14.0)
        .gap(18.0)
        .center_y()
        .on_layout(move |s: &mut State, rect| {
            s.addon_layout.rects.insert(index, rect);
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
    if let Some(error) = &s.error {
        feedback = feedback.child(label(error.clone()));
    }
    let mut submit = Button::new("install-addon", "Install")
        .variant(Variant::Primary)
        .busy(s.config_busy)
        .frame()
        .w(52.0)
        .h(48.0);
    if s.addon_install_complete {
        feedback = feedback.child(label("Add-on installed successfully."));
        submit = Button::new("addon-install-done", "Done")
            .on_press(|s: &mut State| s.addon_install_open = false);
    } else if s.config_busy {
        submit = submit.child(spinner("addon-install-spinner", 24.0));
    } else {
        submit = submit
            .child(icon(&icons::DOWNLOAD, 24.0).color(tokens::accent()))
            .on_press(|s: &mut State| s.install(s.install.text.clone()));
    }
    if s.addon_install_complete {
        feedback = feedback.child(
            icon(&icons::CHECK, 28.0)
                .key("addon-install-success")
                .color(tokens::accent()),
        );
    }
    dialog::layer(
        "addon-dialog-layer",
        dialog::panel("addon-install-dialog", |s: &mut State| {
            s.addon_install_open = false;
        })
        .w(640.0)
        .padding(28.0)
        .gap(20.0)
        .child(dialog::header("Install add-on").child(dialog::close(
            "addon-dialog-close",
            "Close install dialog",
            |s: &mut State| s.addon_install_open = false,
        )))
        .child(label("Manifest URL"))
        .child(
            input(
                "manifest",
                &s.install,
                "http://…/manifest.json or https://…/manifest.json",
                |s| &mut s.install,
                |s| {
                    if !s.addon_install_complete {
                        s.install(s.install.text.clone());
                    }
                },
            )
            .grow(0.0)
            .w_full()
            .h(48.0)
            .min_h(48.0),
        )
        .child(feedback)
        .child(
            row().gap(12.0).center_y().child(submit).child(
                text(if s.error.is_some() {
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
