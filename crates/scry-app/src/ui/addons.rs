//! Supported providers and one shared installation dialog.
use super::{
    State,
    controls::{action, icon_action, input, select_action},
    design::{icons, tokens},
    primitives::{
        button::{Button, Variant},
        surface::{card, scroll, surface},
        text::{heading, label},
    },
};
use crate::services::CINEMETA;
use rune::{El, col, el, row};

pub(super) fn addons(s: &State) -> El {
    let mut list = col().w_full().gap(12.0);
    if s.addon_installed_tab {
        for (index, addon) in s.addons.iter().enumerate() {
            let id = addon.manifest.id.clone();
            list = list.child(
                surface(row())
                    .w_full()
                    .padding(18.0)
                    .radius(12.0)
                    .gap(16.0)
                    .center_y()
                    .child(
                        col()
                            .grow(1.0)
                            .gap(5.0)
                            .child(heading(addon.manifest.name.clone()))
                            .child(label(format!(
                                "{} catalogs · Manifest URL hidden",
                                addon.manifest.catalogs.len()
                            ))),
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
                                    services.remove(id.clone());
                                    s.config_busy = true;
                                }
                            }),
                    ),
            );
        }
        if s.addons.is_empty() {
            list = list.child(label("No add-ons installed."));
        }
    } else {
        for (index, (name, url, description)) in [
            (
                "Cinemeta",
                CINEMETA,
                "Movie and TV series catalogs, metadata, and episode release dates.",
            ),
            (
                "Torrentio",
                "https://torrentio.strem.fun/manifest.json",
                "Streams from direct torrents or a configured debrid provider.",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            list = list.child(
                surface(row())
                    .w_full()
                    .padding(18.0)
                    .radius(12.0)
                    .gap(16.0)
                    .center_y()
                    .child(
                        col()
                            .grow(1.0)
                            .gap(5.0)
                            .child(heading(name))
                            .child(label(description)),
                    )
                    .child(icon_action(
                        format!("supported-addon-{index}"),
                        "Install",
                        &icons::DOWNLOAD,
                        move |s| {
                            s.install = rune::TextEdit::new(url);
                            s.addon_install_open = true;
                        },
                    )),
            );
        }
    }
    col()
        .key("addons-page")
        .gap(12.0)
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .child(
            row()
                .w_full()
                .gap(12.0)
                .child(heading("Add-ons").grow(1.0))
                .child(action("add-addon", "Add by URL", |s| {
                    s.install = rune::TextEdit::new("");
                    s.addon_install_open = true;
                })),
        )
        .child(addon_tabs(s))
        .child(scroll("addons-scroll", list))
}

fn addon_tabs(s: &State) -> El {
    row()
        .gap(8.0)
        .child(select_action(
            "addons-supported-tab",
            "Supported",
            !s.addon_installed_tab,
            |s| s.addon_installed_tab = false,
        ))
        .child(select_action(
            "addons-installed-tab",
            format!("Installed · {}", s.addons.len()),
            s.addon_installed_tab,
            |s| s.addon_installed_tab = true,
        ))
}

pub(super) fn install_dialog(s: &State) -> El {
    let mut feedback = col().w_full();
    if let Some(error) = &s.error {
        feedback = feedback.child(label(error.clone()));
    }
    el().key("addon-dialog-layer")
        .w_full()
        .h_full()
        .absolute(0.0, 0.0)
        .center()
        .fill(tokens::scrim())
        .child(
            card()
                .key("addon-install-dialog")
                .w(560.0)
                .padding(24.0)
                .gap(16.0)
                .modal(true, |s: &mut State| s.addon_install_open = false)
                .child(
                    row()
                        .w_full()
                        .child(heading("Install add-on").grow(1.0))
                        .child(icon_action(
                            "addon-dialog-close",
                            "Close install dialog",
                            &icons::CLOSE,
                            |s| s.addon_install_open = false,
                        )),
                )
                .child(label("Manifest URL"))
                .child(
                    input(
                        "manifest",
                        &s.install,
                        "http://…/manifest.json or https://…/manifest.json",
                        |s| &mut s.install,
                        |s| s.install(s.install.text.clone()),
                    )
                    .grow(0.0)
                    .w_full()
                    .h(46.0)
                    .min_h(46.0),
                )
                .child(feedback)
                .child(
                    Button::new(
                        "install-addon",
                        if s.config_busy {
                            "Working…"
                        } else {
                            "Install"
                        },
                    )
                    .icon(&icons::DOWNLOAD)
                    .variant(Variant::Primary)
                    .busy(s.config_busy)
                    .on_press(|s: &mut State| s.install(s.install.text.clone())),
                ),
        )
}
