use super::surface::card;
use rune::{Align, El, Side, Tx};
use std::time::{Duration, Instant};

const LEAVE_DELAY: Duration = Duration::from_millis(150);

/// Caller-owned state shared by a trigger and its out-of-flow panel.
#[derive(Default)]
pub(in crate::ui) struct HoverOverlay {
    open: bool,
    trigger_inside: bool,
    panel_inside: bool,
    close_at: Option<Instant>,
    trigger_rect: Option<[f32; 4]>,
    pointer: Option<[f64; 2]>,
    pub rect: Option<[f32; 4]>,
}

impl HoverOverlay {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn show(&mut self) {
        self.open = true;
        self.close_at = None;
    }

    pub fn dismiss(&mut self) {
        *self = Self {
            trigger_rect: self.trigger_rect,
            pointer: self.pointer,
            ..Self::default()
        };
    }

    fn hover(&mut self, trigger: bool, inside: bool, now: Instant, tx: &mut Tx) {
        if trigger {
            self.trigger_inside = inside;
        } else {
            self.panel_inside = inside;
        }
        self.update_hover(now, tx);
    }

    /// Native video children do not forward pointer movement to the UI runtime.
    pub fn pointer(&mut self, point: Option<[f64; 2]>, now: Instant, tx: &mut Tx) {
        if self.pointer == point {
            return;
        }
        self.pointer = point;
        let contains = |rect: Option<[f32; 4]>| {
            rect.zip(point).is_some_and(|([x, y, w, h], [px, py])| {
                px >= f64::from(x)
                    && py >= f64::from(y)
                    && px < f64::from(x + w)
                    && py < f64::from(y + h)
            })
        };
        self.trigger_inside = contains(self.trigger_rect);
        self.panel_inside = self.open && contains(self.rect);
        self.update_hover(now, tx);
    }

    fn update_hover(&mut self, now: Instant, tx: &mut Tx) {
        if self.trigger_inside || self.panel_inside {
            self.show();
        } else if self.open {
            self.close_at.get_or_insert(now + LEAVE_DELAY);
            tx.wake_after(LEAVE_DELAY);
        }
        tx.instant();
    }

    /// Poll from the owning feature's update hook, including scheduled wakes.
    pub fn poll(&mut self, now: Instant) -> bool {
        if self.close_at.is_some_and(|at| now >= at) {
            self.dismiss();
            return true;
        }
        false
    }
}

/// Attach to any trigger. Its existing press handler can call `show` for keyboard use.
pub(in crate::ui) fn trigger<S: 'static>(
    content: El,
    enabled: bool,
    state: fn(&mut S) -> &mut HoverOverlay,
) -> El {
    content
        .on_layout(move |s: &mut S, rect| state(s).trigger_rect = Some(rect))
        .on_hover_change(move |s: &mut S, inside, tx| {
            if enabled {
                state(s).hover(true, inside, Instant::now(), tx);
            }
        })
        .on_physical_input(move |s: &mut S, event, tx| {
            let dismiss = matches!(event, rune::PhysicalInput::FocusLost)
                || matches!(
                    event,
                    rune::PhysicalInput::Key {
                        key: rune::PhysicalKeyCode::Escape | rune::PhysicalKeyCode::Tab,
                        pressed: true,
                        ..
                    }
                );
            if dismiss && state(s).is_open() {
                state(s).dismiss();
                tx.instant();
                return matches!(
                    event,
                    rune::PhysicalInput::Key {
                        key: rune::PhysicalKeyCode::Escape,
                        ..
                    }
                );
            }
            false
        })
}

/// Attach after the trigger at the window root. Content and feature policy are caller-owned.
pub(in crate::ui) fn panel<S: 'static>(
    key: &str,
    anchor: &str,
    width: f32,
    content: El,
    state: fn(&mut S) -> &mut HoverOverlay,
) -> El {
    card()
        .key(key)
        .role("group")
        .w(width)
        .padding(16.0)
        .anchored(anchor, Side::Top, Align::End, 12.0)
        .hit()
        .on_press(|_: &mut S| {})
        .on_hover_change(move |s: &mut S, inside, tx| {
            state(s).hover(false, inside, Instant::now(), tx);
        })
        .on_layout(move |s: &mut S, rect| state(s).rect = Some(rect))
        .child(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaving_both_surfaces_closes_but_crossing_the_gap_does_not() {
        let mut overlay = HoverOverlay::default();
        let mut tx = Tx::default();
        let now = Instant::now();
        overlay.hover(true, true, now, &mut tx);
        assert!(overlay.is_open());
        overlay.hover(true, false, now, &mut tx);
        assert!(!overlay.poll(now + LEAVE_DELAY / 2));
        overlay.hover(false, true, now + LEAVE_DELAY / 2, &mut tx);
        assert!(!overlay.poll(now + LEAVE_DELAY * 2));
        assert!(overlay.is_open());
        overlay.hover(false, false, now + LEAVE_DELAY * 2, &mut tx);
        assert!(overlay.poll(now + LEAVE_DELAY * 3));
        assert!(!overlay.is_open());
        assert!(overlay.rect.is_none());
    }

    #[test]
    fn native_pointer_leaving_closes_even_without_a_ui_hover_event() {
        let mut overlay = HoverOverlay {
            trigger_rect: Some([100.0, 100.0, 40.0, 40.0]),
            ..HoverOverlay::default()
        };
        let mut tx = Tx::default();
        let now = Instant::now();
        overlay.pointer(Some([120.0, 120.0]), now, &mut tx);
        assert!(overlay.is_open());
        overlay.pointer(Some([20.0, 20.0]), now, &mut tx);
        overlay.pointer(Some([21.0, 20.0]), now + LEAVE_DELAY / 2, &mut tx);
        assert!(overlay.poll(now + LEAVE_DELAY));
        assert!(!overlay.is_open());
        overlay.pointer(Some([120.0, 120.0]), now + LEAVE_DELAY, &mut tx);
        assert!(overlay.is_open());
    }
}
