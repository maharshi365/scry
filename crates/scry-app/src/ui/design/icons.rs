//! Static Rune vectors. Names stay stable for Rune's icon cache.
use rune::Icon;

pub(in crate::ui) static PLAY: Icon = Icon {
    name: "player-play",
    view: 24.0,
    body: r#"<path d="M8 5v14l11-7Z" fill="currentColor"/>"#,
};
pub(in crate::ui) static PAUSE: Icon = Icon {
    name: "player-pause",
    view: 24.0,
    body: r#"<path d="M6 5h4v14H6zM14 5h4v14h-4z" fill="currentColor"/>"#,
};
pub(in crate::ui) static BACK: Icon = Icon {
    name: "player-back",
    view: 24.0,
    body: r#"<path d="m11 6-8 6 8 6V6Zm10 0-8 6 8 6V6Z" fill="currentColor"/>"#,
};
pub(in crate::ui) static FORWARD: Icon = Icon {
    name: "player-forward",
    view: 24.0,
    body: r#"<path d="m3 6 8 6-8 6V6Zm10 0 8 6-8 6V6Z" fill="currentColor"/>"#,
};
pub(in crate::ui) static VOLUME: Icon = Icon {
    name: "player-volume",
    view: 24.0,
    body: r#"<path d="M3 9h4l5-4v14l-5-4H3Z" fill="currentColor"/><path d="M16 8q4 4 0 8m3-11q7 7 0 14" fill="none" stroke="currentColor" stroke-width="2"/>"#,
};
pub(in crate::ui) static MUTED: Icon = Icon {
    name: "player-muted",
    view: 24.0,
    body: r#"<path d="M3 9h4l5-4v14l-5-4H3Z" fill="currentColor"/><path d="m16 9 6 6m0-6-6 6" stroke="currentColor" stroke-width="2"/>"#,
};
pub(in crate::ui) static CAPTIONS: Icon = Icon {
    name: "player-captions",
    view: 24.0,
    body: r#"<rect x="2" y="5" width="20" height="14" rx="2" fill="none" stroke="currentColor" stroke-width="2"/><path d="M10 9H6v6h4m8-6h-4v6h4" fill="none" stroke="currentColor" stroke-width="2"/>"#,
};
pub(in crate::ui) static AUDIO: Icon = Icon {
    name: "player-audio",
    view: 24.0,
    body: r#"<path d="M5 10v4m4-8v12m4-15v18m4-15v12m4-8v4" stroke="currentColor" stroke-width="2"/>"#,
};
pub(in crate::ui) static STOP: Icon = Icon {
    name: "player-stop",
    view: 24.0,
    body: r#"<rect x="6" y="6" width="12" height="12" rx="1" fill="currentColor"/>"#,
};
pub(in crate::ui) static FULLSCREEN: Icon = Icon {
    name: "player-fullscreen",
    view: 24.0,
    body: r#"<path d="M9 3H3v6m12-6h6v6M3 15v6h6m12-6v6h-6" fill="none" stroke="currentColor" stroke-width="2"/>"#,
};
pub(in crate::ui) static EXIT_FULLSCREEN: Icon = Icon {
    name: "player-exit-fullscreen",
    view: 24.0,
    body: r#"<path d="M3 9h6V3m12 6h-6V3M9 21v-6H3m12 6v-6h6" fill="none" stroke="currentColor" stroke-width="2"/>"#,
};
pub(in crate::ui) static LEFT: Icon = Icon {
    name: "arrow-left",
    view: 24.0,
    body: r#"<path d="m14 6-6 6 6 6M8 12h13" fill="none" stroke="currentColor" stroke-width="2"/>"#,
};
pub(in crate::ui) static RIGHT: Icon = Icon {
    name: "arrow-right",
    view: 24.0,
    body: r#"<path d="m10 6 6 6-6 6M3 12h13" fill="none" stroke="currentColor" stroke-width="2"/>"#,
};
pub(in crate::ui) static DOWN: Icon = Icon {
    name: "chevron-down",
    view: 24.0,
    body: r#"<path d="m6 9 6 6 6-6" fill="none" stroke="currentColor" stroke-width="2"/>"#,
};
pub(in crate::ui) static REFRESH: Icon = Icon {
    name: "refresh",
    view: 24.0,
    body: r#"<path d="M20 4v6h-6M4 20v-6h6M5 9a7 7 0 0 1 12-4l3 5M4 14l3 5a7 7 0 0 0 12-4" fill="none" stroke="currentColor" stroke-width="2"/>"#,
};
pub(in crate::ui) static CLOSE: Icon = Icon {
    name: "close",
    view: 24.0,
    body: r#"<path d="m6 6 12 12M18 6 6 18" stroke="currentColor" stroke-width="2"/>"#,
};
pub(in crate::ui) static CHECK: Icon = Icon {
    name: "check",
    view: 24.0,
    body: r#"<path d="m5 12 4 4L19 6" fill="none" stroke="currentColor" stroke-width="2"/>"#,
};
pub(in crate::ui) static ADD: Icon = Icon {
    name: "add",
    view: 24.0,
    body: r#"<path d="M12 5v14M5 12h14" stroke="currentColor" stroke-width="2"/>"#,
};
