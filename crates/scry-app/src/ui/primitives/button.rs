use super::super::design::tokens::{self, control, type_};
use rune::{El, Icon, Tx, icon, row, text};

pub(in crate::ui) fn menu_item<S: 'static>(
    key: impl Into<String>,
    title: impl Into<String>,
    f: impl Fn(&mut S) + 'static,
) -> El {
    Button::new(key, title)
        .variant(Variant::Ghost)
        .on_press(f)
        .w_full()
        .justify_start()
        .border(0.0, tokens::edge())
        .focus_follows()
}

pub(in crate::ui) fn segmented_group() -> El {
    row()
        .gap(4.0)
        .padding(4.0)
        .radius(10.0)
        .fill(tokens::panel())
        .border(1.0, tokens::edge())
}

pub(in crate::ui) fn segment<S: 'static>(
    key: impl Into<String>,
    title: impl Into<String>,
    selected: bool,
    f: impl Fn(&mut S) + 'static,
) -> El {
    Button::new(key, title)
        .variant(Variant::Ghost)
        .selected(selected)
        .on_press(f)
        .min_w(96.0)
        .h(control::HEIGHT)
        .px(16.0)
        .radius(6.0)
        .border(0.0, tokens::edge())
}

#[derive(Clone, Copy, Default)]
pub(in crate::ui) enum Variant {
    Primary,
    #[default]
    Secondary,
    Ghost,
    Danger,
}

pub(in crate::ui) struct Button {
    key: String,
    title: String,
    variant: Variant,
    selected: bool,
    enabled: bool,
    busy: bool,
    glyph: Option<&'static Icon>,
}

impl Button {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            variant: Variant::Secondary,
            selected: false,
            enabled: true,
            busy: false,
            glyph: None,
        }
    }
    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }
    pub fn icon(mut self, glyph: &'static Icon) -> Self {
        self.glyph = Some(glyph);
        self
    }

    /// Shared interactive frame for controls with caller-owned contents.
    pub fn frame(&self) -> El {
        let active = self.enabled && !self.busy;
        let label = if self.busy {
            format!("{}, busy", self.title)
        } else if !self.enabled {
            format!("{}, unavailable", self.title)
        } else if self.selected {
            format!("{}, selected", self.title)
        } else {
            self.title.clone()
        };
        let (fill, border) = if self.selected {
            (tokens::selected(), tokens::accent())
        } else {
            match self.variant {
                Variant::Primary => (tokens::primary_fill(), tokens::primary_edge()),
                Variant::Secondary => (tokens::panel(), tokens::edge()),
                Variant::Ghost => (tokens::transparent(), tokens::transparent_edge()),
                Variant::Danger => (tokens::danger_fill(), tokens::danger_edge()),
            }
        };
        let frame = row()
            .key(self.key.clone())
            .role("button")
            .label(label)
            .h(control::HEIGHT)
            .px(10.0)
            .center()
            .radius(control::RADIUS)
            .fill(fill)
            .border(1.0, border)
            .focusable(active)
            .focus_selected(self.selected);
        if active {
            super::interaction::focus_ring(frame.hover(tokens::theme().hover))
        } else {
            frame.opacity(tokens::opacity::DISABLED)
        }
    }

    fn content(&self) -> El {
        let color = if !self.enabled || self.busy {
            tokens::disabled()
        } else if matches!(self.variant, Variant::Danger) {
            tokens::danger()
        } else {
            tokens::theme().text
        };
        if let Some(glyph) = self.glyph {
            self.frame()
                .w(control::ICON_TARGET)
                .h(control::ICON_TARGET)
                .px(0.0)
                .child(icon(glyph, control::ICON_SIZE).color(color))
        } else {
            self.frame().child(
                text(self.title.clone())
                    .truncate()
                    .font(type_::LABEL)
                    .color(color),
            )
        }
    }

    pub fn on_press<S: 'static>(self, f: impl Fn(&mut S) + 'static) -> El {
        self.on_press_tx(move |s, _| f(s))
    }

    pub fn on_press_tx<S: 'static>(self, f: impl Fn(&mut S, &mut Tx) + 'static) -> El {
        let control = self.content();
        if self.enabled && !self.busy {
            control.on_press_tx(f)
        } else {
            control
        }
    }
}
