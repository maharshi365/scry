//! Bindings owned by the active player.
use super::{
    super::{
        Page, State,
        shortcuts::{Binding, Focus, KeyMatch, Modifiers, resolve},
    },
    actions::PlayerAction,
};
use rune::KeyEvent;

const fn plain(key: KeyMatch, action: PlayerAction) -> Binding<PlayerAction> {
    Binding {
        key,
        modifiers: Modifiers::Plain,
        in_text: false,
        action,
    }
}

const BINDINGS: &[Binding<PlayerAction>] = &[
    plain(KeyMatch::Text(" "), PlayerAction::TogglePause),
    plain(KeyMatch::Text("k"), PlayerAction::TogglePause),
    plain(KeyMatch::Text("m"), PlayerAction::ToggleMute),
    plain(KeyMatch::Text("j"), PlayerAction::SeekRelative(-10)),
    plain(KeyMatch::Text("l"), PlayerAction::SeekRelative(10)),
    plain(KeyMatch::Text("c"), PlayerAction::CycleSubtitles),
    plain(KeyMatch::Text("f"), PlayerAction::ToggleFullscreen),
    plain(KeyMatch::Left, PlayerAction::SeekRelative(-5)),
    plain(KeyMatch::Right, PlayerAction::SeekRelative(5)),
    plain(KeyMatch::Up, PlayerAction::ChangeVolume(5)),
    plain(KeyMatch::Down, PlayerAction::ChangeVolume(-5)),
];

pub(in crate::ui) fn handle(s: &mut State, event: &KeyEvent, focus: Focus) -> bool {
    if s.navigation.page != Page::Player || !s.player.playing {
        return false;
    }
    if s.player.fullscreen && event.key == rune::Key::Escape && focus == Focus::Screen {
        s.set_fullscreen(false);
        return true;
    }
    let Some(action) = resolve(BINDINGS, event, focus) else {
        return false;
    };
    s.player_action(action);
    true
}
