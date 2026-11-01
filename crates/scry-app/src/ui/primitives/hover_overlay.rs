use super::surface::card;
use rune::{Align, El, Live, Side, Tx};
use std::time::{Duration, Instant};

const LEAVE_DELAY: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Closed,
    Hover,
    Pinned,
}

/// Caller-owned state shared by a trigger and its out-of-flow panel.
pub(in crate::ui) struct HoverOverlay {
    mode: Mode,
    trigger_inside: bool,
    panel_inside: bool,
    close_at: Option<Instant>,
    trigger_rect: Live<[f32; 4]>,
    pointer: Option<[f64; 2]>,
    pub rect: Option<[f32; 4]>,
}

impl Default for HoverOverlay {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            trigger_inside: false,
            panel_inside: false,
            close_at: None,
            trigger_rect: Live::new([0.0; 4]),
            pointer: None,
            rect: None,
        }
    }
}

impl HoverOverlay {
    pub fn is_open(&self) -> bool {
        self.mode != Mode::Closed
    }

    pub fn show(&mut self) {
        if self.mode == Mode::Closed {
            self.mode = Mode::Hover;
        }
        self.close_at = None;
    }

    /// Interactive panels stay open after a click or keyboard activation.
    pub fn pin(&mut self) {
        self.show();
        self.mode = Mode::Pinned;
    }

    pub fn dismiss(&mut self) {
        self.mode = Mode::Closed;
        self.trigger_inside = false;
        self.panel_inside = false;
        self.close_at = None;
        self.rect = None;
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
        if self.pointer == point && !self.is_open() {
            return;
        }
        let contains = |rect: Option<[f32; 4]>| {
            rect.zip(point).is_some_and(|([x, y, w, h], [px, py])| {
                px >= f64::from(x)
                    && py >= f64::from(y)
                    && px < f64::from(x + w)
                    && py < f64::from(y + h)
            })
        };
        let trigger_inside = contains(Some(self.trigger_rect.get()));
        let panel_inside = self.is_open() && contains(self.rect);
        if self.pointer == point
            && self.trigger_inside == trigger_inside
            && self.panel_inside == panel_inside
        {
            return;
        }
        self.pointer = point;
        self.trigger_inside = trigger_inside;
        self.panel_inside = panel_inside;
        self.update_hover(now, tx);
    }

    fn update_hover(&mut self, now: Instant, tx: &mut Tx) {
        if self.trigger_inside || self.panel_inside {
            self.show();
        } else if self.mode == Mode::Hover {
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
    geometry: &HoverOverlay,
    state: fn(&mut S) -> &mut HoverOverlay,
) -> El {
    content
        .drawn_rect_into(&geometry.trigger_rect)
        .on_hover_change(move |s: &mut S, inside, tx| {
            if enabled {
                state(s).hover(true, inside, Instant::now(), tx);
            }
        })
        .on_physical_input(move |s: &mut S, event, tx| {
            let dismiss = matches!(
                event,
                rune::PhysicalInput::Key {
                    key: rune::PhysicalKeyCode::Escape | rune::PhysicalKeyCode::Tab,
                    pressed: true,
                    ..
                }
            );
            let tab = matches!(
                event,
                rune::PhysicalInput::Key {
                    key: rune::PhysicalKeyCode::Tab,
                    ..
                }
            );
            if dismiss && state(s).is_open() && !(tab && state(s).mode == Mode::Pinned) {
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
        .radius(super::super::design::tokens::overlay::RADIUS)
        .role("group")
        .w(width)
        .padding(super::super::design::tokens::layout::PANEL_INSET)
        .anchored(anchor, Side::Top, Align::End, 12.0)
        .hit()
        .on_press(|_: &mut S| {})
        .on_hover_change(move |s: &mut S, inside, tx| {
            state(s).hover(false, inside, Instant::now(), tx);
        })
        .on_layout_tx(move |s: &mut S, rect, tx| {
            state(s).rect = Some(rect);
            tx.live_only();
        })
        .child(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_a_scrolling_trigger_does_not_rebuild_the_view() {
        #[derive(Default)]
        struct Example {
            overlay: HoverOverlay,
            builds: std::cell::Cell<usize>,
        }

        fn view(s: &Example, _: &mut rune::Cx<Example>) -> El {
            s.builds.set(s.builds.get() + 1);
            rune::col()
                .w(200.0)
                .h(150.0)
                .scroll(rune::ScrollSpec {
                    stick_bottom: false,
                })
                .child(
                    rune::col()
                        .child(rune::el().h(50.0))
                        .child(trigger(
                            rune::el().key("trigger").w(80.0).h(40.0),
                            true,
                            &s.overlay,
                            |s: &mut Example| &mut s.overlay,
                        ))
                        .child(rune::el().h(300.0)),
                )
        }

        let mut rt = rune::Runtime::new(
            Example::default(),
            view,
            (200.0, 150.0),
            rune::Env::default(),
        );
        for _ in 0..6 {
            rt.frame(1.0 / 60.0);
        }
        let before = rt.state.overlay.trigger_rect.get();
        let builds = rt.state.builds.get();
        rt.wheel(180.0, 120.0, -20.0);
        for _ in 0..6 {
            rt.frame(1.0 / 60.0);
        }
        let after = rt.state.overlay.trigger_rect.get();
        assert!(
            after[1] < before[1],
            "The tracked trigger must follow the scroll"
        );
        assert_eq!(
            rt.state.builds.get(),
            builds,
            "Tracking geometry alone must not rebuild the view"
        );
    }

    #[test]
    fn native_pointer_rechecks_a_trigger_that_moves_under_a_still_pointer() {
        let mut overlay = HoverOverlay::default();
        let trigger_rect = overlay.trigger_rect.clone();
        trigger_rect.set([100.0, 100.0, 40.0, 40.0]);
        let now = Instant::now();
        let mut tx = Tx::default();
        let point = Some([120.0, 120.0]);
        overlay.pointer(point, now, &mut tx);
        assert!(overlay.is_open());
        trigger_rect.set([200.0, 100.0, 40.0, 40.0]);
        overlay.pointer(point, now, &mut tx);
        assert!(
            overlay.poll(now + LEAVE_DELAY),
            "Moving the trigger away must start dismissal even without pointer motion"
        );
        assert!(!overlay.is_open());
        trigger_rect.set([100.0, 100.0, 40.0, 40.0]);
        overlay.pointer(Some([121.0, 120.0]), now + LEAVE_DELAY, &mut tx);
        assert!(
            overlay.is_open(),
            "Dismissal must preserve the trigger's live binding"
        );
    }

    #[test]
    fn native_pointer_does_not_reopen_a_dismissed_panel_until_it_moves() {
        let mut overlay = HoverOverlay::default();
        overlay.trigger_rect.set([100.0, 100.0, 40.0, 40.0]);
        let now = Instant::now();
        let mut tx = Tx::default();
        let point = Some([120.0, 120.0]);
        overlay.pointer(point, now, &mut tx);
        assert!(overlay.is_open());
        overlay.dismiss();
        overlay.pointer(point, now + LEAVE_DELAY, &mut tx);
        assert!(
            !overlay.is_open(),
            "Explicit dismissal must survive the next native pointer poll"
        );
        overlay.pointer(Some([121.0, 120.0]), now + LEAVE_DELAY, &mut tx);
        assert!(
            overlay.is_open(),
            "Moving over the trigger may open it again"
        );
    }

    #[test]
    fn pinned_interactive_panel_survives_pointer_leaving_until_dismissed() {
        let mut overlay = HoverOverlay::default();
        let mut tx = Tx::default();
        let now = Instant::now();
        overlay.pin();
        overlay.hover(false, false, now, &mut tx);
        assert!(!overlay.poll(now + LEAVE_DELAY * 2));
        assert!(overlay.is_open());
        overlay.dismiss();
        assert!(!overlay.is_open());
    }

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
            trigger_rect: Live::new([100.0, 100.0, 40.0, 40.0]),
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
