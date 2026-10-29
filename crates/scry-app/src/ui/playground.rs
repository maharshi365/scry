//! Isolated, in-memory examples of shared UI controls and patterns.
use super::{
    design::tokens,
    primitives::{
        button::{Button, Variant, menu_choice, segment, segmented_group},
        dialog,
        feedback::{self, Tone},
        form::{choices, option_row},
        input::input,
        motion,
        pattern::{detail_header, page, section, settings_group},
        stepper::stepper,
        text::label,
    },
};
use rune::playground::Playground;
use rune::{App, AppSpec, Cx, El, Key, KeyEvent, TextEdit, Tx, col, el, row};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Category {
    #[default]
    Controls,
    Settings,
    ListDetail,
    Feedback,
    Motion,
}

impl Category {
    #[cfg(test)]
    const fn story(self) -> &'static str {
        match self {
            Self::Controls => "Examples/Controls",
            Self::Settings => "Examples/Settings",
            Self::ListDetail => "Examples/List detail",
            Self::Feedback => "Feedback/Notice",
            Self::Motion => "Examples/Motion",
        }
    }
}

// This state deliberately has no app state, host, worker, storage, or service handles.
struct State {
    category: Category,
    name: TextEdit,
    presses: u32,
    quality: Quality,
    volume: i32,
    selected: usize,
    saved: Option<Quality>,
    dialog_open: bool,
    notice: bool,
    expanded: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Quality {
    #[default]
    Automatic,
    High,
}

impl Quality {
    fn title(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic",
            Self::High => "High quality",
        }
    }
}

fn init() -> State {
    State {
        category: Category::default(),
        name: TextEdit::new("Local example"),
        presses: 0,
        quality: Quality::default(),
        volume: 50,
        selected: 0,
        saved: None,
        dialog_open: false,
        notice: true,
        expanded: false,
    }
}

impl State {
    fn show(&mut self, category: Category) {
        self.category = category;
        self.dialog_open = false;
    }

    fn save(&mut self) {
        self.saved = Some(self.quality);
    }

    fn confirm(&mut self) {
        self.dialog_open = false;
        self.notice = false;
    }
}

fn on_key(s: &mut State, event: &KeyEvent, _: &mut Tx) -> bool {
    if event.key == Key::Escape && s.dialog_open {
        s.dialog_open = false;
        return true;
    }
    false
}

fn controls(s: &State) -> El {
    section(
        "playground-control-examples",
        "Buttons and input",
        "Real local interactions. Disabled and busy controls do not run actions.",
        col()
            .gap(20.0)
            .child(
                choices()
                    .child(
                        Button::new("playground-primary", "Primary action")
                            .variant(Variant::Primary)
                            .on_press(|s: &mut State| s.presses = s.presses.saturating_add(1)),
                    )
                    .child(
                        Button::new("playground-secondary", "Secondary action")
                            .on_press(|s: &mut State| s.presses = s.presses.saturating_add(1)),
                    )
                    .child(
                        Button::new("playground-disabled", "Disabled")
                            .enabled(false)
                            .on_press(|s: &mut State| s.presses = s.presses.saturating_add(1)),
                    )
                    .child(
                        Button::new("playground-busy", "Working")
                            .busy(true)
                            .on_press(|s: &mut State| s.presses = s.presses.saturating_add(1)),
                    ),
            )
            .child(label(format!("Actions taken: {}", s.presses)).key("playground-count"))
            .child(option_row(
                "playground-name",
                "Example name",
                "Edit and press Enter. Nothing is sent or stored.",
                input(
                    "playground-name-input",
                    &s.name,
                    "Example name",
                    |s: &mut State| &mut s.name,
                    on_key,
                    |s: &mut State| s.presses = s.presses.saturating_add(1),
                ),
            )),
    )
}

fn settings(s: &State) -> El {
    let mut quality = segmented_group();
    for value in [Quality::Automatic, Quality::High] {
        quality = quality.child(segment(
            format!("playground-quality-{}", value.title()),
            value.title(),
            s.quality == value,
            move |s: &mut State| s.quality = value,
        ));
    }
    section(
        "playground-settings-examples",
        "Playback preferences",
        "A settings pattern using fixture values, not your playback preferences.",
        col()
            .gap(20.0)
            .child(settings_group(
                "playground-settings-group",
                [
                    option_row(
                        "playground-quality",
                        "Quality",
                        "Choose the preferred quality for this example.",
                        quality,
                    ),
                    option_row(
                        "playground-volume",
                        "Volume",
                        "Adjust the fixture from 0 to 100. No audio is played.",
                        stepper(
                            "playground-volume",
                            "Example volume",
                            format!("{}%", s.volume),
                            true,
                            |s: &mut State, direction| {
                                s.volume = (s.volume + direction * 10).clamp(0, 100);
                            },
                        ),
                    ),
                ],
            ))
            .child(
                Button::new("playground-save", "Apply to example")
                    .variant(Variant::Primary)
                    .on_press(State::save),
            )
            .child(label(s.saved.map_or_else(
                || "Changes stay in this window.".into(),
                |quality| format!("Applied in memory: {}", quality.title()),
            ))),
    )
}

const ITEMS: [(&str, &str); 3] = [
    (
        "Quiet landscapes",
        "A calm fixture for a list and detail layout.",
    ),
    (
        "Night journeys",
        "A second fixture. Select rows to change the detail header.",
    ),
    (
        "A very long example title that wraps across narrow layouts",
        "Long text must wrap without hiding the actions or forcing horizontal scrolling.",
    ),
];

fn list_detail(s: &State) -> El {
    let mut list = col().key("playground-list").gap(8.0).min_w(0.0);
    for (index, (title, _)) in ITEMS.iter().enumerate() {
        list = list.child(menu_choice(
            format!("playground-item-{index}"),
            *title,
            index == s.selected,
            move |s: &mut State| s.selected = index,
        ));
    }
    let (title, description) = ITEMS.get(s.selected).copied().unwrap_or(ITEMS[0]);
    section(
        "playground-list-examples",
        "List and detail",
        "Select a fixture. No catalog or artwork is loaded.",
        col()
            .gap(24.0)
            .child(list)
            .child(detail_header(
                "playground-detail",
                title,
                description,
                Button::new("playground-detail-action", "Mark example")
                    .on_press(|s: &mut State| s.presses = s.presses.saturating_add(1)),
            ))
            .child(label(format!("Examples marked: {}", s.presses))),
    )
}

fn feedback(s: &State) -> El {
    let mut content = col().gap(16.0);
    for (key, title, description, tone) in [
        (
            "neutral",
            "Ready to explore",
            "This is a neutral message with supporting text.",
            Tone::Neutral,
        ),
        (
            "success",
            "Example complete",
            "The local action completed successfully.",
            Tone::Success,
        ),
        (
            "warning",
            "Check this example",
            "A warning explains what needs attention.",
            Tone::Warning,
        ),
        (
            "danger",
            "Example unavailable",
            "An error explains the problem and offers a next step.",
            Tone::Danger,
        ),
    ] {
        content = content.child(feedback::message(
            &format!("playground-message-{key}"),
            title,
            description,
            tone,
            el(),
        ));
    }
    content = content
        .child(feedback::status(
            "playground-notice-status",
            if s.notice {
                "Example notice present"
            } else {
                "Example notice cleared"
            },
            if s.notice {
                Tone::Neutral
            } else {
                Tone::Success
            },
        ))
        .child(
            Button::new("playground-open-dialog", "Clear example notice…")
                .on_press(|s: &mut State| s.dialog_open = true),
        );
    section(
        "playground-feedback-examples",
        "Messages and confirmation",
        "Feedback is deterministic and entirely local.",
        content,
    )
}

fn motion(s: &State) -> El {
    let mut content = col().gap(20.0).child(
        Button::new(
            "playground-motion-toggle",
            if s.expanded {
                "Hide example"
            } else {
                "Reveal example"
            },
        )
        .on_press(|s: &mut State| s.expanded = !s.expanded),
    );
    if s.expanded {
        content = content.child(motion::surface(feedback::message(
            "playground-motion-surface",
            "A shared transition",
            "Reveal and hide this local surface to inspect its motion.",
            Tone::Neutral,
            el(),
        )));
    }
    section(
        "playground-motion-examples",
        "Surface motion",
        "Use the same shared transition as the application.",
        content,
    )
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let examples = match s.category {
        Category::Controls => controls(s),
        Category::Settings => settings(s),
        Category::ListDetail => list_detail(s),
        Category::Feedback => feedback(s),
        Category::Motion => motion(s),
    };
    let mut root = el()
        .key("playground-root")
        .w_full()
        .h_full()
        .padding(tokens::layout::PANEL_INSET)
        .fill(tokens::canvas())
        .child(page(
            "playground-page",
            "UI playground",
            "Isolated examples · in-memory fixtures · no network or saved data",
            el(),
            examples,
        ));
    if s.dialog_open {
        root = root.child(dialog::layer(
            "playground-dialog-layer",
            dialog::panel("playground-dialog", |s: &mut State| s.dialog_open = false)
                .w(480.0)
                .padding(24.0)
                .gap(20.0)
                .child(dialog::header("Clear the example notice?"))
                .child(label("This changes only this playground window. Your library and settings are not affected."))
                .child(
                    row()
                        .gap(12.0)
                        .flex_wrap()
                        .child(dialog::close("playground-dialog-cancel", "Cancel", |s: &mut State| s.dialog_open = false))
                        .child(Button::new("playground-dialog-confirm", "Clear notice").variant(Variant::Danger).on_press(State::confirm)),
                ),
        ));
    }
    root
}

fn scene_controls(s: &mut State) {
    *s = init();
}
fn scene_settings(s: &mut State) {
    *s = init();
    s.show(Category::Settings);
}
fn scene_list_detail(s: &mut State) {
    *s = init();
    s.show(Category::ListDetail);
}
fn scene_feedback(s: &mut State) {
    *s = init();
    s.show(Category::Feedback);
}
fn scene_dialog(s: &mut State) {
    scene_feedback(s);
    s.dialog_open = true;
}
fn scene_motion(s: &mut State) {
    *s = init();
    s.show(Category::Motion);
    s.expanded = true;
}

fn app() -> App<State> {
    App::new(AppSpec {
        fonts: tokens::fonts::FILES,
        families: tokens::fonts::FAMILIES,
        on_key: Some(on_key),
        scenes: &[
            ("Examples/Controls", scene_controls),
            ("Examples/Settings", scene_settings),
            ("Examples/List detail", scene_list_detail),
            ("Feedback/Notice", scene_feedback),
            ("Feedback/Dialog", scene_dialog),
            ("Examples/Motion", scene_motion),
        ],
        min_size: Some((1106.0, 480.0)),
        ..AppSpec::new("Scry · UI playground", (1280.0, 820.0), init, view)
    })
    .playground(Playground::new().enabled(true).sizes(&[
        ("Compact", (640.0, 480.0)),
        ("Medium", (880.0, 600.0)),
        ("Wide", (1280.0, 820.0)),
    ]))
}

pub(super) fn run(args: Vec<String>) {
    app().args(args).run();
}

#[cfg(test)]
mod tests;
