use crate::services::streaming_settings::{CACHE_SIZES_GIB, Preferences};
use crate::ui::{
    State,
    primitives::{
        button::{Button, Size},
        feedback::{self, Tone},
        form::{choices, option_row},
        pattern::{section, settings_group},
    },
};
use rune::{El, row};

#[derive(Default)]
pub(in crate::ui) struct SettingsUi {
    pub preferences: Preferences,
    pub busy: bool,
    pub error: Option<String>,
    pub pending: Option<Preferences>,
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
        vec![option_row(
            "streaming-cache",
            "Torrent cache size",
            "Reuses downloaded video data. The current torrent can exceed this limit. Stored in Scry’s data directory.",
            sizes,
        )],
    );
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
    section(
        "streaming-preferences",
        "Streaming",
        "Cache size changes apply to the next playback. Older inactive data is removed first.",
        panel,
    )
}

impl State {
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
