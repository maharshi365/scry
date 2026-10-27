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
mod navigation;
mod player;
mod poster_grid;
mod primitives;
#[cfg(feature = "profiling")]
mod profiling;
mod scenes;
mod settings;
mod shell;
mod shortcuts;
mod source_picker;
mod source_ranking;
mod stats;

use crate::services::{Event, Services};
use library_view::LibraryUi;
use navigation::{NavigationState, Page};
use player::state::PlayerState;
use rune::TextEdit;
use scry_addons::{Addon, Catalog, Meta, Stream};
use scry_app::platform::Host;
use std::sync::{Arc, Mutex};

const EPISODES_PER_PAGE: usize = 100;

#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent UI, configuration, and playback flags are not mutually exclusive states"
)]
pub struct State {
    #[cfg(feature = "profiling")]
    profiling: Option<profiling::Driver>,
    native_navigation: scry_app::platform::NavigationInput,
    logo: rune::Pixels,
    artwork: Option<crate::artwork::Artwork>,
    artwork_urls: Vec<String>,
    player_artwork: Option<crate::artwork::Artwork>,
    player_artwork_urls: Vec<String>,
    poster_rects: std::collections::BTreeMap<usize, [f32; 4]>,
    poster_columns: usize,
    poster_viewport: Option<[f32; 4]>,
    poster_route: u64,
    library: LibraryUi,
    calendar: calendar::CalendarUi,
    stats: stats::StatsUi,
    guide: guide::GuideUi,
    context_menu: Option<(rune::PointerPress, Option<context_menu::Target>)>,
    addon_install_open: bool,
    addon_install_complete: bool,
    addon_layout: addons::AddonLayout,
    detail_namespace: String,
    pending_episode: Option<String>,
    host: Host,
    host_error: Arc<Mutex<Option<String>>>,
    services: Option<Services>,
    events: Option<tokio::sync::mpsc::Receiver<Event>>,
    navigation: NavigationState,
    player: PlayerState,
    addons: Vec<Addon>,
    config_busy: bool,
    install: TextEdit,
    direct: TextEdit,
    search: TextEdit,
    displayed_query: String,
    kind: String,
    catalog: Option<(Addon, Catalog)>,
    catalog_paging: catalog::CatalogPaging,
    metas: Arc<Vec<Meta>>,
    providers: Arc<Vec<Addon>>,
    detail: Option<Meta>,
    detail_preview: bool,
    app_fullscreen: bool,
    // Stable protocol ID (including season/episode suffix) for library integration.
    selected_video_id: Option<String>,
    episode_offset: usize,
    episode_search: TextEdit,
    #[expect(
        clippy::option_option,
        reason = "None means no selection; Some(None) explicitly selects unnumbered episodes"
    )]
    episode_season: Option<Option<u32>>,
    season_menu_open: bool,
    streams: Vec<(String, Stream)>,
    stream_provider: Option<String>,
    stream_best_first: bool,
    source_menu_open: bool,
    source_sort_open: bool,
    warnings: Vec<String>,
    error: Option<String>,
    status: String,
    loading: bool,
    catalog_id: u64,
    detail_id: u64,
    stream_id: u64,
    startup_play: Option<String>,
    quit: Option<rune::AppHandle<State>>,
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
