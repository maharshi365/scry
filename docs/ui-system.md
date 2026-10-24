# UI system

The native Rune UI shares its visual vocabulary inside `crates/scry-app/src/ui`.

```text
design/
  tokens.rs       semantic colors, text roles, shared control sizes
  icons.rs        static Rune SVG icons
primitives/
  button.rs       variants, selection, disabled/busy states, focus
  input.rs        text field with caller-supplied edit and submit handlers
  text.rs         heading and secondary text
  surface.rs      cards, badges, empty states, scroll containers
controls.rs       thin bindings to the app's State and input shortcuts
formatting.rs     clocks, metrics, excerpts; no styling
```

Screens own navigation, data loading, playback rules, and layout geometry.
`poster_grid.rs` owns the common card and virtualized grid for Movies, TV shows,
search, Library, and Currently watching. Screens put their own headings and filters
above that grid. Movie posters show saved progress as an overlay one-sixth of the
poster height above the bottom, or a top-right check mark when watched. Visible
movie states are read in bounded batches on the database worker for all grid routes.
TV posters use the latest unfinished episode's progress. A check mark requires
every known released episode across all seasons to be watched. Future and undated
episodes do not count; an empty released set never earns a check mark. The database
summary is independent of episode and Currently watching pagination. Visible tracked
shows fetch metadata in batches of 12 with four requests in flight; the bounded
32-show in-memory metadata cache expires after ten minutes and Refresh retries failures.
The show summary is also persisted for Library filters. A separate background
scan checks tracked shows even when the current filter hides them; explicit
show-level watched/unwatched overrides take priority. Season context actions
capture all of a season's episodes and apply one atomic database command. Context
menu options use borderless ghost buttons; the outer menu retains its panel edge.
Episode posters come from their parent series without changing resume identity.
Poster metadata is loaded only for visible cards, in batches of at most
12 with four provider requests in flight. It uses the same bounded image cache
as catalogs. Missing or offline artwork keeps a title placeholder; Refresh retries
failed lookups. No database migration or saved-state rewrite is needed.
Primitives receive callbacks and must not depend on those features. The player
keeps its playback gate and fullscreen transaction in its own module.

## Use the shared controls

For a normal action, use `controls::action`. For a selected filter or tab, use
`controls::select_action`; do not append a checkmark to its text. For icon-only
actions, use `controls::icon_action` with an explicit action name and an icon
from `design::icons`.

Use the `Button` builder when a control needs a variant or an inactive state:

```rust,ignore
Button::new("install-addon", if busy { "Working…" } else { "Install" })
    .variant(Variant::Primary)
    .busy(busy)
    .on_press(|s: &mut State| s.install(s.install.text.clone()))
```

Variants are Primary, Secondary, Ghost, and Danger. Disabled and busy controls
have no press handler and do not take focus. Active buttons use a shared
keyboard focus ring. Selected buttons use the same fill and border and set
Rune's focus-selected flag. Rune does not expose a general selected/disabled
accessibility attribute here, so the action label also includes state.

Input submit behavior is supplied by the screen. An element key identifies the
element; it must not choose an app command.

Use `surface::card` for a vertical panel or `surface::surface(row())` for a
horizontal panel. Keep page-specific dimensions and virtualization geometry in
the feature module. Shared tokens are not a reason to replace every layout number.

## Prevent drift

From the repository root:

```powershell
cargo test -p scry-app --bin scry
cargo clippy -p scry-app --all-targets -- -D warnings
```

`clippy.toml` disallows Rune color constructors, direct theme lookups, and
`TextStyle::new`. `design/tokens.rs` is the only app UI exception. The standalone
embedded-player probe has its own narrowly scoped exception because it does not
include the app UI. The workspace denies `clippy::disallowed_methods`, so these
calls fail even a normal Clippy run without `-D warnings`.

`ui::tests::design_contract` also checks source boundaries: raw visual values
belong in the token module, SVG bodies belong in the icon module, and primitives
must not refer to app state or services. This is a lightweight source-policy
check, not a full Rust dependency analyzer. Clippy resolves the banned APIs,
including imported aliases. Do not add lint exceptions in screen modules.

The runtime tests cover action labels, selected styling, inactive action gates,
keyboard activation, input submission, and existing scroll/fullscreen behavior.
Use the existing tooling scenes for visual checks at 880×600 and 1280×820.
