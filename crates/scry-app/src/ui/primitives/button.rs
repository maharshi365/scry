use super::super::design::tokens::{self, control, type_};
use rune::{El, Icon, Tx, col, el, icon, row, text};

pub(in crate::ui) fn tab<S: 'static>(
    key: &str,
    title: &str,
    selected: bool,
    select: impl Fn(&mut S) + 'static,
) -> El {
    col()
        .child(
            Button::new(key, title)
                .variant(Variant::Ghost)
                .selected(selected)
                .on_press(select)
                .role("tab")
                .fill(tokens::transparent())
                .border(tokens::border::NONE, tokens::transparent_edge())
                .radius(tokens::radius::NONE)
                .px(16.0),
        )
        .child(el().w_full().h(2.0).fill(if selected {
            tokens::accent()
        } else {
            tokens::transparent()
        }))
}

pub(in crate::ui) fn menu_item<S: 'static>(
    key: impl Into<String>,
    title: impl Into<String>,
    f: impl Fn(&mut S) + 'static,
) -> El {
    Button::new(key, title)
        .list_item()
        .on_press(f)
        .w_full()
        .justify_start()
        .focus_follows()
}

pub(in crate::ui) fn segmented_group() -> El {
    row()
        .gap(4.0)
        .padding(4.0)
        .radius(tokens::radius::SEGMENTED)
        .fill(tokens::panel())
        .border(tokens::border::WIDTH, tokens::edge())
}

pub(in crate::ui) fn menu_choice<S: 'static>(
    key: impl Into<String>,
    title: impl Into<String>,
    selected: bool,
    f: impl Fn(&mut S) + 'static,
) -> El {
    let key = key.into();
    let title = title.into();
    let mut choice = menu_item(key.clone(), title.clone(), f)
        .label(if selected {
            format!("{title}, selected")
        } else {
            title
        })
        .child(el().grow(1.0));
    if selected {
        choice = choice.child(
            icon(&super::super::design::icons::CHECK, 14.0)
                .key(format!("{key}-selected"))
                .color(tokens::accent()),
        );
    }
    choice
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
        .radius(tokens::radius::SM)
        .border(tokens::border::NONE, tokens::edge())
}

#[derive(Clone, Copy, Default)]
pub(in crate::ui) enum Variant {
    Primary,
    #[default]
    Secondary,
    Ghost,
    Danger,
}

#[derive(Clone, Copy, Default)]
pub(in crate::ui) enum Size {
    #[default]
    Default,
    Compact,
    Pill,
    Setting,
}

#[derive(Clone, Copy, Default)]
enum Layout {
    #[default]
    Standard,
    ListItem,
    Dropdown,
}

#[derive(Clone, Copy)]
pub(in crate::ui) enum DropdownVariant {
    Transparent,
    Framed,
}

pub(in crate::ui) fn dropdown_trigger(
    key: impl Into<String>,
    title: impl Into<String>,
    variant: DropdownVariant,
) -> Button {
    let mut button = Button::new(key, title).variant(match variant {
        DropdownVariant::Transparent => Variant::Ghost,
        DropdownVariant::Framed => Variant::Secondary,
    });
    button.layout = Layout::Dropdown;
    button
}

pub(in crate::ui) struct Button {
    key: String,
    title: String,
    variant: Variant,
    selected: bool,
    enabled: bool,
    busy: bool,
    glyph: Option<&'static Icon>,
    size: Size,
    layout: Layout,
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
            size: Size::Default,
            layout: Layout::Standard,
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

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    pub fn list_item(mut self) -> Self {
        self.layout = Layout::ListItem;
        self.variant = Variant::Ghost;
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
        let mut frame = row()
            .key(self.key.clone())
            .role("button")
            .label(label)
            .h(control::HEIGHT)
            .px(10.0)
            .center()
            .radius(control::RADIUS)
            .fill(fill)
            .border(tokens::border::WIDTH, border)
            .focusable(active)
            .focus_selected(self.selected);
        frame = match self.size {
            Size::Default => frame,
            Size::Compact => frame.h(control::COMPACT_HEIGHT).px(6.0),
            Size::Pill => frame.h(control::PILL_HEIGHT).radius(control::PILL_RADIUS),
            Size::Setting => frame
                .h(control::SETTING_HEIGHT)
                .min_w(control::SETTING_MIN_WIDTH)
                .radius(tokens::radius::MD),
        };
        frame = match self.layout {
            Layout::Standard => frame,
            Layout::ListItem => frame
                .w_full()
                .justify_start()
                .border(tokens::border::NONE, tokens::transparent_edge()),
            Layout::Dropdown => frame.gap(6.0),
        };
        let transparent_dropdown =
            matches!(self.layout, Layout::Dropdown) && matches!(self.variant, Variant::Ghost);
        if transparent_dropdown {
            frame = frame.px(0.0).cursor(rune::Cursor::Pointer);
        }
        if active {
            super::interaction::focus_ring(frame.hover(if transparent_dropdown {
                tokens::transparent()
            } else {
                tokens::theme().hover
            }))
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
                .w(match self.size {
                    Size::Compact => control::COMPACT_HEIGHT,
                    Size::Pill => control::PILL_HEIGHT,
                    _ => control::ICON_TARGET,
                })
                .px(0.0)
                .child(
                    icon(
                        glyph,
                        if matches!(self.size, Size::Compact) {
                            control::COMPACT_ICON_SIZE
                        } else {
                            control::ICON_SIZE
                        },
                    )
                    .color(color),
                )
        } else {
            let content = self.frame().child(
                text(self.title.clone())
                    .truncate()
                    .font(if matches!(self.size, Size::Compact) {
                        type_::SMALL
                    } else {
                        type_::LABEL
                    })
                    .color(color),
            );
            if matches!(self.layout, Layout::Dropdown) {
                content.child(
                    icon(&super::super::design::icons::DOWN, control::CHEVRON_SIZE)
                        .key(format!("{}-chevron", self.key))
                        .color(tokens::theme().text_secondary),
                )
            } else {
                content
            }
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
