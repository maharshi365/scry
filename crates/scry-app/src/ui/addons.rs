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

const SUPPORTED: [(&str, &str, &str, &str); 2] = [
    (
        "com.linvo.cinemeta",
        "Cinemeta",
        CINEMETA,
        "Movie and TV series catalogs, metadata, and episode release dates.",
    ),
    (
        "com.stremio.torrentio.addon",
        "Torrentio",
        "https://torrentio.strem.fun/manifest.json",
        "Streams from direct torrents or a configured debrid provider.",
    ),
];

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
                &addon.manifest.name,
                &format!(
                    "{} catalogs · Manifest URL hidden",
                    addon.manifest.catalogs.len()
                ),
                if disabled { "Disabled" } else { "Enabled" },
                if disabled {
                    &icons::PAUSE
                } else {
                    &icons::CHECK
                },
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
    for (index, (id, name, url, description)) in SUPPORTED.into_iter().enumerate() {
        if s.addons.iter().any(|a| a.manifest.id == id) {
            continue;
        }
        list = list.child(
            provider_card(name, description, "Available", &icons::DOWNLOAD).child(
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
        .child(scroll("addons-scroll", list))
}

fn provider_card(name: &str, description: &str, status: &str, glyph: &'static rune::Icon) -> El {
    surface(row())
        .w_full()
        .padding(24.0)
        .radius(14.0)
        .gap(18.0)
        .center_y()
        .child(
            el().w(56.0)
                .h(56.0)
                .radius(14.0)
                .fill(tokens::selected())
                .center()
                .child(icon(glyph, 28.0).color(tokens::accent())),
        )
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
