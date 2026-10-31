//! Native UI state and feature module wiring.
mod addons;
mod app;
mod artwork;
mod calendar;
mod catalog;
mod context_menu;
mod controls;
mod design;
mod detail;
mod discover;
mod episodes;
mod events;
mod formatting;
mod guide;
mod library_view;
mod my_media;
mod navigation;
mod player;
mod playground;
mod poster_grid;
mod primitives;
#[cfg(feature = "profiling")]
mod profiling;
mod request;
mod scenes;
mod settings;
mod shell;
mod shortcuts;
mod source_picker;
mod source_ranking;
mod stats;

use library_view::LibraryUi;
use navigation::{NavigationState, Page};
use player::state::PlayerState;

const EPISODES_PER_PAGE: usize = 100;

pub struct State {
    runtime: app::AppRuntime,
    shell: shell::ShellState,
    artwork: artwork::ArtworkState,
    posters: poster_grid::PosterGridState,
    library: LibraryUi,
    my_media: my_media::MediaUi,
    calendar: calendar::CalendarUi,
    stats: stats::StatsUi,
    guide: guide::GuideUi,
    navigation: NavigationState,
    player: PlayerState,
    settings: settings::SettingsState,
    addons: addons::AddonsState,
    discover: catalog::DiscoverState,
    detail: detail::DetailState,
}

pub fn run() -> anyhow::Result<()> {
    app::run()
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    reason = "UI test fixtures and missing-element assertions must fail the test immediately"
)]
mod tests;
