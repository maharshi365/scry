# UI state and shortcuts

Player and navigation state belong to their feature modules. Rune still requires
one root `State`. That root holds the feature state and connects services; it is
not a process-global singleton.

```text
crates/scry-app/src/ui/
├── app.rs                   # feature setup and shortcut registration
├── shortcuts.rs             # shared matching, focus policy, and scope priority
├── navigation.rs            # routes, navigation state, and history actions
├── navigation/shortcuts.rs  # history and Escape bindings
└── player/
    ├── mod.rs               # video surface and controls
    ├── state.rs             # playback state and defaults
    ├── actions.rs           # shared player action API
    ├── session.rs           # start, stop, and service commands
    └── shortcuts.rs         # active-player bindings
```

## Use a player action

Buttons, key handlers, and other UI features call the same API:

```rust
use crate::ui::player::actions::PlayerAction;

state.player_action(PlayerAction::TogglePause);
state.player_action(PlayerAction::ChangeVolume(5));
```

Actions require active playback, but do not require the player page. Keyboard
bindings additionally require the player page. Use actions instead of changing
player fields to request a player operation. Service events still update the
reported state directly, with playback-generation checks.

## Add feature shortcuts

1. Keep the action type and binding table inside the feature.
2. Add a handler that checks whether the feature is active, calls `resolve`, and
   runs the matched action. Return `false` if the key does not apply.
3. Register the handler in `app.rs` with its scope. Do not add feature-specific
   key checks to the shared dispatcher or input component.

Dispatch checks **local → screen → app**. The first handled key stops dispatch.
Handlers in the same scope use registration order. Each binding explicitly
states whether it can run inside a text input. History bindings allow it;
player bindings and navigation Escape do not. The input adapter consumes Escape
to keep it local to the editor. Ordinary editing keys remain with Rune.

Plain bindings allow Shift, but reject Alt, Ctrl, Cmd, and Logo. History retains
its existing Alt-based matching, including additional modifiers.

History is owned by the app, not Rune. On Windows, `platform/windows/navigation.rs`
captures mouse back/forward buttons and browser commands on the app window. It
queues directions and wakes Rune. The UI poll drains that queue and calls the
same `State::travel` action used by keyboard shortcuts and toolbar buttons.
Native callbacks never mutate UI state directly. The window subclass is removed
and its input handle released when the window is destroyed.

## Remaining state work

This is the first extraction, not a complete root-state migration. Catalog,
search, detail, add-on, and artwork fields remain on `State`. Library state
already has `LibraryUi`. Those features can adopt the same ownership pattern
without changing the shared shortcut system. Cross-feature navigation snapshots
and the service-event match also remain in place.
