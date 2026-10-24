# Smaller Rust modules

## Goal

Split files by responsibility, not by an arbitrary line limit. Keep public APIs,
database formats, worker messages, and runtime behavior unchanged. Use normal
Rust directory modules rather than `include!` or more `#[path]` overrides.

The first extraction pass is implemented. The remaining sections describe the
target boundaries for later passes. Keep state-ownership changes separate from
mechanical moves. Error handling changed where needed to meet the workspace
lints; existing behavior tests and a poisoned-artwork-cache regression test cover
those paths.

## Implemented

- `ui/mod.rs` now contains shared state and module wiring, not page rendering,
  startup, navigation, polling, or tests.
- Flat UI feature modules own app startup, catalog requests, playback commands,
  navigation, service events, artwork, shared components, and each page.
- `ui/tests/` separates catalog, episode, layout, library, navigation, and playback
  tests. Shared fixtures live in its `mod.rs`.
- `ui/scenes.rs` owns tooling fixtures. The source picker now lives under `ui/`.
- `ui/library_view/progress.rs` owns progress saves, EOF handling, and the shutdown
  barrier. The remaining identity, synchronization, and rendering split is still
  pending.
- `library_worker/` separates its handle, protocol, thread runtime, and database
  operations. Worker tests remain with the handle for now.
- Workspace Clippy checks pass, including pedantic warnings. Intentional test
  assertions, diagnostic CLI output, saturating pixel/time conversions, and
  selected declarative layouts have scoped `#[expect(..., reason = ...)]`
  attributes. No workspace-wide lint exemptions were added.

## Main findings

- `crates/scry-app/src/ui.rs` combines application startup, all shared state,
  navigation, service events, page rendering, tooling scenes, and UI tests.
- `library_view.rs` combines media identity, worker integration, playback progress,
  shutdown, playlist actions, rendering, and tests. It is a child of `ui` through
  a `#[path]` override and imports the entire parent with `use super::*`.
- `library_worker.rs` has a useful bounded interface already. Its protocol,
  thread lifecycle, database operations, and tests can be separated without
  changing that interface.
- `services.rs` mixes add-on configuration, provider requests, search aggregation,
  playback, and shutdown.
- Tests make up a large part of several files. Move tests with their feature;
  do not merely move all tests into another large file.

## Proposed application layout

Paths below are relative to `crates/scry-app/src/`.

```text
ui/
  mod.rs                 # State, Page, shared constants, module wiring
  app.rs                 # init, setup, run, close handling, State::new/Drop
  navigation.rs          # Route, navigate, remember, travel, keyboard history
  catalog.rs             # catalog selection, browse/search requests, results
  episodes.rs            # seasons, episode paging and selection
  playback.rs            # play, play_source, play_direct, stop, controls
  events.rs              # poll and service event handling
  components.rs          # buttons, inputs, labels, shared colors/formatting
  artwork.rs             # visible artwork and poster viewport tracking
  shell.rs               # sidebar, topbar, page dispatch
  scenes.rs              # explicit tooling fixtures only
  pages/
    mod.rs
    discover.rs          # poster grid and Discover rendering
    addons.rs            # add-on management rendering
    detail.rs            # details, episodes and season selector rendering
    player.rs            # player surface and controls rendering
  source_picker.rs       # existing source picker, moved under its owner
  library/
    mod.rs               # LibraryUi and narrow feature entry points
    identity.rs          # canonical keys, parent/episode Media conversion
    sync.rs              # requests, generations, acknowledgments, snapshots
    progress.rs          # captured playback identity, saves, EOF, flush
    actions.rs           # open media, select playlist, reorder items
    detail.rs            # detail-page library controls and watched labels
    view.rs              # watchlist, watched list and playlist rendering
  tests/
    mod.rs               # shared test helpers and test module declarations
    navigation.rs
    catalog.rs
    episodes.rs
    playback.rs
    layout.rs
    library.rs           # rendered library actions and persistence
library_worker/
  mod.rs                 # Worker handle, start/send/drain, nonblocking Drop
  protocol.rs            # Command, Event, Snapshot, SelectedState
  runtime.rs             # run loop, emit, cancellation and backpressure
  operations.rs          # apply commands and build snapshots
  tests.rs               # bounded worker integration and persistence tests
services/
  mod.rs                 # Services, Inbox, Event, Control, Source, lifecycle
  addons.rs              # load/install/remove and configuration replacement
  catalog.rs             # browse requests
  search.rs              # async search fan-out; keep ranking in crate::search
  detail.rs              # metadata and stream provider requests
  playback.rs            # player_loop, torrent preparation, report lifecycle
```

Do not create every small file up front. For example, keep catalog and search
requests together until they benefit from separate files. Existing focused
modules such as `search.rs`, `source_picker.rs`, and `artwork_cache.rs` should
not be divided just to meet a size target.

### Visibility and dependencies

- Keep `ui::run`, `library_worker::Worker`, and the existing service interface
  stable. Re-export protocol types from `library_worker::mod.rs`.
- Keep `State` in the `ui` parent initially. Feature files can provide separate
  `impl State` blocks. Use `pub(super)` for direct-parent access, or
  `pub(in crate::ui)` where nested UI modules need access. Do not make fields
  public across the crate just to make the split compile.
- Replace wildcard parent imports with explicit imports. Rendering should depend
  on shared components and feature methods, not sibling page internals.
- Library identity and worker operations must not import rendering helpers.
- Keep the synchronous SQLite library behind the worker. Do not move database
  calls onto the UI thread.
- Let `scenes.rs` own tooling fixtures. Production pages must not depend on them.
- Keep pure tests beside their owning modules. Keep cross-feature Rune runtime
  tests under `ui/tests`, using a small set of shared fixtures.

The first split still shares `State`; it does not solve state coupling by itself.
A later, separate change can group route, catalog, episode, and playback state
after their boundaries are proven. Do not combine that redesign with code moves.

## Other candidates

| Current file | Suggested boundary |
| --- | --- |
| `diagnostics.rs` | `options.rs`, `probe.rs` (including fixture safety), `analysis.rs`, terminal output, and feature tests under `diagnostics/` |
| `artwork.rs` | UI-facing handle, queue/cache state, download/decode pipeline, and tests under `artwork/` |
| `crates/scry-library/src/lib.rs` | Keep `Library` and re-exports at the root; extract models, schema/opening, media/watchlist operations, playlists, and watched/progress operations |
| `crates/scry-addons/src/lib.rs` | Separate protocol models/validation from HTTP client operations when needed |
| `crates/scry-torrents/src/lib.rs` | Separate torrent session management from HTTP range serving when needed |

Prioritize the application UI, library integration, worker, and services. Leave
smaller, cohesive files alone until there is a concrete maintenance benefit.

## Safe implementation order

1. Record a passing test baseline. Run Clippy separately to identify existing
   lint failures before any source moves.
2. Split the library worker first. Its existing message boundary and tests make
   it the lowest-risk starting point.
3. Move library UI integration into `ui/library/`. Preserve canonical keys,
   generation rejection, acknowledgement tracking, progress capture, and flush.
4. Extract shared components and page rendering from `ui.rs`. Move the existing
   source picker under `ui/`; remove the two `#[path]` overrides.
5. Extract navigation, episode selection, playback, event polling, and startup.
   Split UI tests by feature without changing their assertions.
6. Split services, then diagnostics and the remaining justified candidates.

Use one reviewable change per feature boundary. Aim for roughly 150–400 lines
per production module where useful, but do not split cohesive logic just to
satisfy that guideline. Avoid generic `utils.rs` and `helpers.rs` dumping grounds.

## Validation

```text
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets
cargo fmt --all -- --check
```

Existing tests already cover important refactor risks: stale service and library
events, history restoration, season and episode identity, virtualized poster
layout, persisted library actions, progress at EOF, worker backpressure, and
shutdown flushing. Preserve those tests and run them after each extraction.
Check the Discover, Detail, and Library tooling scenes after rendering moves.

Workspace Clippy rules are inherited by every member. Pedantic findings are
warnings; the requested safety and output lints are errors. Future violations
must be fixed or have an explicit, narrowly scoped exception with a reason.
Diagnostic CLI output and Cargo build-script directives are intentional output.
Use `cargo clippy --workspace --all-targets -- -D warnings` to verify that the
current source has no outstanding warnings.
