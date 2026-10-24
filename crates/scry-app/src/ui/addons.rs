//! Add-on management and direct playback controls.
use super::{
    State,
    components::{action, heading, input, label, scroll},
};
use crate::services::CINEMETA;
use rune::{
    El, col, row, text,
    tok::{self, TextStyle},
};

pub(super) fn addons(s: &State) -> El {
    let t = tok::theme(true);
    let mut list = col().key("addon-list").gap(12.0);
    for (index, addon) in s.addons.iter().enumerate() {
        let id = addon.manifest.id.clone();
        list = list.child(
            row()
                .key(format!("addon-{index}"))
                .padding(18.0)
                .radius(12.0)
                .fill(t.card)
                .gap(16.0)
                .center_y()
                .child(
                    col()
                        .grow(1.0)
                        .gap(5.0)
                        .child(
                            text(addon.manifest.name.clone())
                                .truncate()
                                .font(TextStyle::new(17.0, 550.0))
                                .color(t.text),
                        )
                        .child(label(format!(
                            "{} catalogs · manifest URL kept private",
                            addon.manifest.catalogs.len()
                        ))),
                )
                .child(action(format!("remove-{index}"), "Remove", move |s| {
                    if s.config_busy {
                        return;
                    }
                    s.config_busy = true;
                    if let Some(services) = &s.services {
                        services.remove(id.clone());
                    }
                })),
        );
    }
    col().key("addons-page").gap(10.0).grow(1.0).min_h(0.0).min_w(0.0)
        .child(heading("Add-ons & direct play"))
        .child(label("Install a Stremio-compatible HTTPS manifest URL, including a configured Torrentio URL."))
        .child(row().gap(8.0)
            .child(input("manifest", &s.install, "https://…/manifest.json", |s| &mut s.install))
            .child(action("install-addon", if s.config_busy { "Working…" } else { "Install add-on" }, |s| s.install(s.install.text.clone()))))
        .child(label("Privacy: configured manifest URLs can contain credentials. They are saved as plaintext in your private app-data addons.json. Do not share that file."))
        .child(row().gap(8.0)
            .child(action("install-cinemeta", "Add Cinemeta", |s| s.install(CINEMETA.into())))
            .child(action("install-torrentio", "Add Torrentio", |s| s.install("https://torrentio.strem.fun/manifest.json".into()))))
        .child(label("Torrentio's default sources use direct torrents. For debrid HTTP streams, paste your configured manifest instead."))
        .child(scroll("addons-scroll", list))
        .child(label("DIRECT PLAYBACK · HTTP / HTTPS"))
        .child(row().gap(8.0)
            .child(input("direct-url", &s.direct, "Paste a direct video URL…", |s| &mut s.direct))
            .child(action("play-direct", "Play URL", State::play_direct)))
        .child(label("Only play media you have permission to access. Torrent playback connects to peers and may upload data."))
}
