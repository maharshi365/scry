//! Native drag surface; interactive descendants retain their own input handling.
use super::super::design::tokens;
use rune::{El, row};

pub(in crate::ui) fn titlebar(native_inset: f32) -> El {
    super::surface::Variant::Chrome
        .apply(row())
        .window_drag()
        .w_full()
        .h(tokens::shell::TOPBAR_HEIGHT)
        .pl(native_inset + tokens::shell::TITLEBAR_INSET_GAP)
        .pr(tokens::shell::TITLEBAR_INSET_GAP)
        .gap(8.0)
        .center_y()
}
