//! Native application setup, state initialization, and shutdown.
use super::{
    State,
    events::poll,
    library_view::LibraryUi,
    scenes::{
        scene_calendar, scene_calendar_day, scene_calendar_month, scene_calendar_refresh,
        scene_detail, scene_detail_watched, scene_discover, scene_episodes, scene_library,
        scene_library_empty, scene_movie, scene_player_fullscreen, scene_preview,
        scene_source_wrap, scene_watching,
    },
    shell::view,
    shortcuts::{Focus, Registration, Scope, dispatch},
};
use crate::services::Services;
use rune::{App, AppSpec};
use scry_app::platform::{Host, VideoHost};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(super) struct AppRuntime {
    pub error: Option<String>,
    pub streams: super::request::StreamRequest,
    pub services: Option<Services>,
    pub events: Option<tokio::sync::mpsc::Receiver<crate::services::Event>>,
    pub startup_play: Option<String>,
    pub quit: Option<rune::AppHandle<State>>,
    #[cfg(feature = "profiling")]
    pub profiling: Option<super::profiling::Driver>,
}

const SHORTCUTS: &[Registration] = &[
    Registration {
        scope: Scope::Screen,
        handle: super::guide::shortcut,
    },
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
    if event.key == rune::Key::Escape && s.player.audio.overlay.is_open() {
        s.player.audio.overlay.dismiss();
        tx.instant();
        return true;
    }
    if event.key == rune::Key::Escape && s.player.subtitles.overlay.is_open() {
        s.player.subtitles.overlay.dismiss();
        tx.instant();
        return true;
    }
    if s.player.fullscreen {
        s.player.interaction.reveal(std::time::Instant::now());
        if event.key == rune::Key::Tab {
            s.player.interaction.keyboard = true;
        }
    }
    if event.key == rune::Key::Escape && (s.shell.context_menu.is_some() || s.addons.install_open) {
        s.shell.context_menu = None;
        s.addons.install_open = false;
        return true;
    }
    let fullscreen = s.player.fullscreen;
    let handled = dispatch(s, event, Focus::Screen, SHORTCUTS);
    if s.player.fullscreen != fullscreen {
        tx.instant();
    }
    handled
}

pub(super) fn on_input_key(s: &mut State, event: &rune::KeyEvent, tx: &mut rune::Tx) -> bool {
    // Escape belongs to the editor, even when no feature handles it.
    let fullscreen = s.player.fullscreen;
    let handled = dispatch(s, event, Focus::TextInput, SHORTCUTS) || event.key == rune::Key::Escape;
    if s.player.fullscreen != fullscreen {
        tx.instant();
    }
    handled
}

impl State {
    pub(super) fn new(host: Host, host_error: Arc<Mutex<Option<String>>>) -> Self {
        Self {
            runtime: AppRuntime::default(),
            shell: super::shell::ShellState {
                host,
                window: scry_app::platform::Window::default(),
                host_error,
                native_navigation: scry_app::platform::NavigationInput::default(),
                context_menu: None,
            },
            artwork: super::artwork::ArtworkState::default(),
            posters: super::poster_grid::PosterGridState::default(),
            library: LibraryUi::default(),
            calendar: super::calendar::CalendarUi::default(),
            stats: super::stats::StatsUi::default(),
            guide: super::guide::GuideUi::default(),
            navigation: super::navigation::NavigationState::default(),
            player: super::player::state::PlayerState::default(),
            settings: super::settings::SettingsState::default(),
            addons: super::addons::AddonsState::default(),
            discover: super::catalog::DiscoverState::default(),
            detail: super::detail::DetailState::default(),
        }
    }
}

pub(super) fn init() -> State {
    State::new(Host::default(), Arc::default())
}

impl Drop for State {
    fn drop(&mut self) {
        self.runtime.events.take();
        if let Some(mut services) = self.runtime.services.take() {
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
    #[cfg(feature = "profiling")]
    {
        s.runtime.profiling = super::profiling::Driver::start(wake.clone());
    }
    s.shell.native_navigation.connect(wake.clone());
    if headless() {
        s.addons.busy = s.addons.pending.is_some();
        return;
    }
    s.artwork.posters = crate::artwork::Artwork::start(&wake).ok();
    s.artwork.player = crate::artwork::Artwork::start_player(&wake).ok();
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
            s.runtime.services = Some(services);
            s.settings.subtitles.busy = true;
            s.settings.streaming.busy = true;
            s.runtime.events = Some(events);
        }
        Err(error) => {
            s.addons.busy = false;
            s.runtime.error = Some(error.to_string());
        }
    }
}

pub(super) fn run() -> anyhow::Result<()> {
    if std::env::args().any(|arg| arg == "--playground") {
        super::playground::run(std::env::args().skip(1).collect());
        return Ok(());
    }
    run_app()
}

fn run_app() -> anyhow::Result<()> {
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
    let native_window = scry_app::platform::Window::default();
    let attached_window = native_window.clone();
    let error = Arc::new(Mutex::new(None));
    let window_error = error.clone();
    let native_navigation = scry_app::platform::NavigationInput::default();
    let window_navigation = native_navigation.clone();
    let app = App::new(AppSpec {
        fonts: super::design::tokens::fonts::FILES,
        families: super::design::tokens::fonts::FAMILIES,
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        scenes: SCENES,
        min_size: Some((880.0, 600.0)),
        ..AppSpec::new("Scry", (1280.0, 820.0), init, view)
    })
    .chrome(super::design::tokens::shell::CHROME)
    .keep_running();
    let quit = app.handle();
    app.args(rune_args)
        .init(move || {
            let mut state = State::new(host, error);
            state.shell.window = native_window;
            state.shell.native_navigation = native_navigation;
            state.runtime.startup_play = startup_play;
            state.runtime.quit = Some(quit);
            state
        })
        .on_window(move |window| {
            if let Err(error) =
                attached_window.attach(window, super::design::tokens::topbar(), &window_navigation)
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
            s.shell.host.hide();
            s.runtime.events.take();
            if let Some(mut services) = s.runtime.services.take() {
                let quit = s.runtime.quit.clone();
                services.shutdown_then(move || {
                    finish_library();
                    if let Some(quit) = quit {
                        quit.quit();
                    }
                });
            } else if let Some(quit) = s.runtime.quit.clone() {
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

type Scene = (&'static str, fn(&mut State));
const SCENES: &[Scene] = &[
    ("guide-empty", super::guide::scene_empty),
    ("guide-empty-movies", super::guide::scene_empty_movies),
    ("movies-empty", super::scenes::scene_movies_empty),
    ("series-empty", super::scenes::scene_series_empty),
    ("anime-empty", super::scenes::scene_anime_empty),
    ("watching-empty", super::library_view::scene_watching_empty),
    ("player-fullscreen", scene_player_fullscreen),
    ("player-intro", super::scenes::scene_player_intro),
    (
        "player-intro-fullscreen",
        super::scenes::scene_player_intro_fullscreen,
    ),
    (
        "player-intro-upcoming",
        super::scenes::scene_player_intro_upcoming,
    ),
    ("settings", super::scenes::scene_settings),
    ("addon-dialog", super::scenes::scene_addon_dialog),
    ("addon-installing", super::scenes::scene_addon_installing),
    ("addons", super::scenes::scene_addons),
    ("subtitles", super::player::subtitles::scene),
    ("stats", super::stats::scene),
    ("discover", scene_discover),
    ("detail", scene_detail),
    ("detail-watched", scene_detail_watched),
    ("episodes", scene_episodes),
    ("movie", scene_movie),
    ("preview", scene_preview),
    ("source-wrap", scene_source_wrap),
    ("library", scene_library),
    ("library-empty", scene_library_empty),
    ("watching", scene_watching),
    ("calendar", scene_calendar),
    ("calendar-month", scene_calendar_month),
    ("calendar-day", scene_calendar_day),
    ("calendar-refresh", scene_calendar_refresh),
];
