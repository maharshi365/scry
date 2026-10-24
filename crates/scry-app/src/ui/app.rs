//! Native application setup, state initialization, and shutdown.
use super::{
    State,
    events::poll,
    library_view::LibraryUi,
    scenes::{
        scene_calendar, scene_calendar_day, scene_calendar_month, scene_calendar_refresh,
        scene_detail, scene_detail_watched, scene_discover, scene_library, scene_library_empty,
        scene_watching,
    },
    shell::view,
    shortcuts::{Focus, Registration, Scope, dispatch},
};
use crate::services::Services;
use rune::{App, AppSpec, TextEdit};
use scry_app::platform::{Host, VideoHost};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

const SHORTCUTS: &[Registration] = &[
    Registration {
        scope: Scope::Screen,
        handle: super::player::shortcuts::handle,
    },
    Registration {
        scope: Scope::Screen,
        handle: super::navigation::shortcuts::screen,
    },
    Registration {
        scope: Scope::App,
        handle: super::navigation::shortcuts::history,
    },
];

pub(super) fn on_key(s: &mut State, event: &rune::KeyEvent, tx: &mut rune::Tx) -> bool {
    if event.key == rune::Key::Escape && (s.context_menu.is_some() || s.addon_install_open) {
        s.context_menu = None;
        s.addon_install_open = false;
        return true;
    }
    let fullscreen = s.player.fullscreen;
    let handled = dispatch(s, event, Focus::Screen, SHORTCUTS);
    if s.player.fullscreen != fullscreen {
        tx.instant();
    }
    handled
}

pub(super) fn on_input_key(s: &mut State, event: &rune::KeyEvent, _: &mut rune::Tx) -> bool {
    // Escape belongs to the editor, even when no feature handles it.
    dispatch(s, event, Focus::TextInput, SHORTCUTS) || event.key == rune::Key::Escape
}

impl State {
    pub(super) fn new(host: Host, host_error: Arc<Mutex<Option<String>>>) -> Self {
        let logo = rune::Pixels::new();
        logo.set(
            128,
            128,
            include_bytes!(concat!(env!("OUT_DIR"), "/skry.rgba")).to_vec(),
        );
        Self {
            native_navigation: scry_app::platform::NavigationInput::default(),
            logo,
            artwork: None,
            artwork_urls: Vec::new(),
            player_artwork: None,
            player_artwork_urls: Vec::new(),
            poster_rects: BTreeMap::default(),
            poster_columns: 3,
            poster_viewport: None,
            poster_route: 0,
            library: LibraryUi::default(),
            calendar: super::calendar::CalendarUi::default(),
            context_menu: None,
            addon_install_open: false,
            addon_installed_tab: false,
            detail_namespace: String::new(),
            pending_episode: None,
            host,
            host_error,
            services: None,
            events: None,
            navigation: super::navigation::NavigationState::default(),
            player: super::player::state::PlayerState::default(),
            addons: Vec::new(),
            config_busy: true,
            install: TextEdit::new(""),
            direct: TextEdit::new(""),
            search: TextEdit::new(""),
            displayed_query: String::new(),
            kind: "movie".into(),
            catalog: None,
            metas: Arc::default(),
            providers: Arc::default(),
            detail: None,
            selected_video_id: None,
            episode_offset: 0,
            episode_season: None,
            season_menu_open: false,
            streams: Vec::new(),
            stream_provider: None,
            source_menu_open: false,
            warnings: Vec::new(),
            error: None,
            status: String::new(),
            loading: false,
            catalog_id: 0,
            detail_id: 0,
            stream_id: 0,
            startup_play: None,
            quit: None,
        }
    }
}

pub(super) fn init() -> State {
    State::new(Host::default(), Arc::default())
}

impl Drop for State {
    fn drop(&mut self) {
        self.events.take();
        if let Some(mut services) = self.services.take() {
            services.shutdown();
        }
    }
}

pub(super) fn headless() -> bool {
    std::env::args().any(|a| {
        matches!(
            a.as_str(),
            "--snap" | "--frames" | "--first-pixels" | "--tree" | "--trace" | "--strip"
        )
    })
}

fn setup(s: &mut State, wake: rune::Waker) {
    s.native_navigation.connect(wake.clone());
    if headless() {
        s.config_busy = false;
        return;
    }
    s.artwork = crate::artwork::Artwork::start(&wake).ok();
    s.player_artwork = crate::artwork::Artwork::start_player(&wake).ok();
    match Services::start(wake.clone()) {
        Ok((services, events)) => {
            let path = services.data_dir().join(scry_library::DATABASE_NAME);
            let result = std::fs::create_dir_all(services.data_dir())
                .map_err(anyhow::Error::from)
                .and_then(|()| crate::library_worker::Worker::start(path, wake));
            match result {
                Ok(worker) => s.library.worker = Some(worker),
                Err(_) => s.library.error = Some("Cannot start the local library".into()),
            }
            s.services = Some(services);
            s.events = Some(events);
        }
        Err(error) => {
            s.config_busy = false;
            s.error = Some(error.to_string());
        }
    }
}

pub(super) fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut rune_args = Vec::new();
    let mut startup_play = None;
    while let Some(arg) = args.next() {
        if arg == "--play" {
            startup_play = Some(
                args.next()
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| anyhow::anyhow!("--play requires a URL or local file path"))?,
            );
        } else {
            rune_args.push(arg);
        }
    }
    let host = Host::default();
    let window_host = host.clone();
    let error = Arc::new(Mutex::new(None));
    let window_error = error.clone();
    let native_navigation = scry_app::platform::NavigationInput::default();
    let window_navigation = native_navigation.clone();
    let app = App::new(AppSpec {
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        scenes: &[
            ("discover", scene_discover),
            ("detail", scene_detail),
            ("detail-watched", scene_detail_watched),
            ("library", scene_library),
            ("library-empty", scene_library_empty),
            ("watching", scene_watching),
            ("calendar", scene_calendar),
            ("calendar-month", scene_calendar_month),
            ("calendar-day", scene_calendar_day),
            ("calendar-refresh", scene_calendar_refresh),
        ],
        min_size: Some((880.0, 600.0)),
        ..AppSpec::new("Scry", (1280.0, 820.0), init, view)
    })
    .keep_running();
    let quit = app.handle();
    app.args(rune_args)
        .init(move || {
            let mut state = State::new(host, error);
            state.native_navigation = native_navigation;
            state.startup_play = startup_play;
            state.quit = Some(quit);
            state
        })
        .on_window(move |window| {
            if let Err(error) = window_navigation.attach(window)
                && let Ok(mut current) = window_error.lock()
            {
                *current = Some(error.to_string());
            }
            if let Err(error) = window_host.attach(window)
                && let Ok(mut current) = window_error.lock()
            {
                *current = Some(error.to_string());
            }
        })
        .on_close(|s, tx| {
            s.set_fullscreen(false);
            s.save_library_progress();
            let finish_library = s.take_library_shutdown();
            s.host.hide();
            s.events.take();
            if let Some(mut services) = s.services.take() {
                let quit = s.quit.clone();
                services.shutdown_then(move || {
                    finish_library();
                    if let Some(quit) = quit {
                        quit.quit();
                    }
                });
            } else if let Some(quit) = s.quit.clone() {
                std::thread::spawn(move || {
                    finish_library();
                    quit.quit();
                });
            } else {
                std::thread::spawn(finish_library);
                tx.quit();
            }
        })
        .run();
    Ok(())
}
