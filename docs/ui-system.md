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
