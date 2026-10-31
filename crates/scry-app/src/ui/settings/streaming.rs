use crate::services::streaming_settings::{CACHE_SIZES_GIB, Preferences};
use crate::ui::{
    State,
    primitives::{
        button::{Button, Size},
        feedback::{self, Tone},
        form::{choices, option_row},
        pattern::settings_group,
    },
};
use rune::{El, col, row};

#[derive(Default)]
pub(in crate::ui) struct SettingsUi {
    pub preferences: Preferences,
    pub busy: bool,
    pub error: Option<String>,
    pub pending: Option<Preferences>,
    pub clearing: bool,
    pub cache_message: Option<String>,
    pub cache_error: bool,
}

pub(super) fn view(s: &State) -> El {
    let settings = &s.settings.streaming;
    let mut sizes = choices();
    for &cache_gib in CACHE_SIZES_GIB {
        sizes = sizes.child(
            Button::new(
                format!("streaming-cache-{cache_gib}"),
                format!("{cache_gib} GiB"),
            )
            .size(Size::Setting)
            .selected(settings.preferences.cache_gib == cache_gib)
            .enabled(!settings.busy)
            .on_press(move |s: &mut State| s.save_streaming_preferences(Preferences { cache_gib }))
            .grow(1.0),
        );
    }
    let mut panel = settings_group(
        "streaming-settings",
        vec![
            option_row(
                "streaming-cache",
                "Torrent cache size",
                "Reuses downloaded video data. The current torrent can exceed this limit. Stored in Scry’s data directory.",
                sizes,
            ),
            option_row(
                "streaming-cache-clear-row",
                "Clear torrent cache",
                "Deletes downloaded torrent data, not your watch progress or saved streams. Stop playback first.",
                row().child(
                    Button::new(
                        "streaming-cache-clear",
                        if settings.clearing {
                            "Clearing…"
                        } else {
                            "Clear cache"
                        },
                    )
                    .enabled(!settings.clearing && !s.player.playing)
                    .on_press(|s: &mut State| s.clear_torrent_cache()),
                ),
            ),
        ],
    );
    if let Some(message) = &settings.cache_message {
        panel = panel.child(feedback::message(
            "streaming-cache-message",
            if settings.cache_error {
                "Cache not cleared"
            } else {
                "Cache cleared"
            },
            message,
            if settings.cache_error {
                Tone::Danger
            } else {
                Tone::Neutral
            },
            row(),
        ));
    }
    if let Some(error) = &settings.error {
        panel = panel.child(feedback::message(
            "streaming-settings-error",
            "Cannot update streaming settings",
            error,
            Tone::Danger,
            row().child(
                Button::new("streaming-settings-retry", "Retry")
                    .enabled(!settings.busy)
                    .on_press(|s: &mut State| {
                        let preferences = s
                            .settings
                            .streaming
                            .pending
                            .clone()
                            .unwrap_or_else(|| s.settings.streaming.preferences.clone());
                        s.save_streaming_preferences(preferences);
                    }),
            ),
        ));
    }
    col().key("streaming-preferences").w_full().child(panel)
}

impl State {
    pub(in crate::ui) fn clear_torrent_cache(&mut self) {
        if self.settings.streaming.clearing || self.player.playing {
            return;
        }
        if let Some(services) = &self.runtime.services {
            self.settings.streaming.clearing = true;
            self.settings.streaming.cache_message = None;
            services.clear_torrent_cache();
        }
    }

    pub(in crate::ui) fn save_streaming_preferences(&mut self, preferences: Preferences) {
        if self.settings.streaming.busy || preferences.validate().is_err() {
            return;
        }
        self.settings.streaming.error = None;
        if let Some(services) = &self.runtime.services {
            self.settings.streaming.pending = Some(preferences.clone());
            match services.save_streaming_preferences(preferences) {
                Ok(()) => self.settings.streaming.busy = true,
                Err(error) => self.settings.streaming.error = Some(error.to_string()),
            }
        } else {
            self.settings.streaming.preferences = preferences;
            self.settings.streaming.pending = None;
        }
    }
}
