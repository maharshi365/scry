use rune::Icon;

pub(in crate::ui) static TRAILER: Icon = Icon {
    name: "trailer",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/clapperboard.svg"),
};
pub(in crate::ui) static EYE: Icon = Icon {
    name: "watched",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/eye.svg"),
};

pub(in crate::ui) static LIBRARY: Icon = Icon {
    name: "library",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/library.svg"),
};
pub(in crate::ui) static CALENDAR: Icon = Icon {
    name: "calendar",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/calendar.svg"),
};
pub(in crate::ui) static ADDONS: Icon = Icon {
    name: "addons",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/addons.svg"),
};
pub(in crate::ui) static SETTINGS: Icon = Icon {
    name: "settings",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/settings.svg"),
};
pub(in crate::ui) static MOVIE: Icon = Icon {
    name: "movie",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/movie.svg"),
};
pub(in crate::ui) static TV: Icon = Icon {
    name: "tv",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/tv.svg"),
};

pub(in crate::ui) static SPINNER: Icon = Icon {
    name: "loader-circle",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/loader-circle.svg"),
};

pub(in crate::ui) static DOWNLOAD: Icon = Icon {
    name: "download",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/download.svg"),
};
pub(in crate::ui) static TRASH: Icon = Icon {
    name: "trash",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/trash-2.svg"),
};

pub(in crate::ui) static PLAY: Icon = Icon {
    name: "player-play",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/play.svg"),
};
pub(in crate::ui) static PAUSE: Icon = Icon {
    name: "player-pause",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/pause.svg"),
};
pub(in crate::ui) static BACK: Icon = Icon {
    name: "player-back",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/rewind.svg"),
};
pub(in crate::ui) static FORWARD: Icon = Icon {
    name: "player-forward",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/fast-forward.svg"),
};
pub(in crate::ui) static VOLUME: Icon = Icon {
    name: "player-volume",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/volume-2.svg"),
};
pub(in crate::ui) static MUTED: Icon = Icon {
    name: "player-muted",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/volume-x.svg"),
};
pub(in crate::ui) static CAPTIONS: Icon = Icon {
    name: "player-captions",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/captions.svg"),
};
pub(in crate::ui) static AUDIO: Icon = Icon {
    name: "player-audio",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/audio-lines.svg"),
};
pub(in crate::ui) static FULLSCREEN: Icon = Icon {
    name: "player-fullscreen",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/maximize.svg"),
};
pub(in crate::ui) static EXIT_FULLSCREEN: Icon = Icon {
    name: "player-exit-fullscreen",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/minimize.svg"),
};
pub(in crate::ui) static LEFT: Icon = Icon {
    name: "arrow-left",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/arrow-left.svg"),
};
pub(in crate::ui) static RIGHT: Icon = Icon {
    name: "arrow-right",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/arrow-right.svg"),
};
pub(in crate::ui) static DOWN: Icon = Icon {
    name: "chevron-down",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/chevron-down.svg"),
};
pub(in crate::ui) static REFRESH: Icon = Icon {
    name: "refresh",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/refresh-cw.svg"),
};
pub(in crate::ui) static CLOSE: Icon = Icon {
    name: "close",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/x.svg"),
};
pub(in crate::ui) static CHECK: Icon = Icon {
    name: "check",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/check.svg"),
};
pub(in crate::ui) static ADD: Icon = Icon {
    name: "add",
    view: 24.0,
    body: include_str!("../../../assets/icons/rune/plus.svg"),
};

macro_rules! stats_icon {
    ($name:ident, $file:literal) => {
        pub(in crate::ui) static $name: Icon = Icon {
            name: $file,
            view: 24.0,
            body: include_str!(concat!("../../../assets/icons/rune/", $file, ".svg")),
        };
    };
}
stats_icon!(LAYERS, "layers");
stats_icon!(FLAG, "flag");
stats_icon!(SUN, "sun");
stats_icon!(SHIELD, "shield");
stats_icon!(SCAN_EYE, "scan-eye");
stats_icon!(GRADUATION, "graduation-cap");
stats_icon!(REPEAT, "repeat-2");
stats_icon!(MOON, "moon");
stats_icon!(BRIEFCASE, "briefcase-business");
stats_icon!(TROPHY, "trophy");
stats_icon!(CLOCK, "clock-3");
stats_icon!(CHART, "chart-no-axes-combined");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lucide_bodies_preserve_upstream_geometry_and_inherited_styles()
    -> Result<(), Box<dyn std::error::Error>> {
        macro_rules! source {
            ($icon:ident, $file:literal) => {
                (
                    &$icon,
                    include_str!(concat!("../../../assets/icons/lucide/", $file, ".svg")),
                )
            };
        }
        let sources = [
            source!(DOWNLOAD, "download"),
            source!(TRASH, "trash-2"),
            source!(PLAY, "play"),
            source!(PAUSE, "pause"),
            source!(BACK, "rewind"),
            source!(FORWARD, "fast-forward"),
            source!(VOLUME, "volume-2"),
            source!(MUTED, "volume-x"),
            source!(CAPTIONS, "captions"),
            source!(AUDIO, "audio-lines"),
            source!(FULLSCREEN, "maximize"),
            source!(EXIT_FULLSCREEN, "minimize"),
            source!(LEFT, "arrow-left"),
            source!(RIGHT, "arrow-right"),
            source!(DOWN, "chevron-down"),
            source!(REFRESH, "refresh-cw"),
            source!(CLOSE, "x"),
            source!(CHECK, "check"),
            source!(ADD, "plus"),
            source!(SETTINGS, "settings"),
            source!(LAYERS, "layers"),
            source!(FLAG, "flag"),
            source!(SUN, "sun"),
            source!(SHIELD, "shield"),
            source!(SCAN_EYE, "scan-eye"),
            source!(GRADUATION, "graduation-cap"),
            source!(REPEAT, "repeat-2"),
            source!(MOON, "moon"),
            source!(BRIEFCASE, "briefcase-business"),
            source!(TROPHY, "trophy"),
            source!(CLOCK, "clock-3"),
            source!(CHART, "chart-no-axes-combined"),
        ];
        for (icon, svg) in sources {
            assert!(
                icon.body.trim_start().starts_with("<svg"),
                "{}: adapted SVG must have a root element",
                icon.name
            );
            assert!(
                icon.body.trim_end().ends_with("</svg>"),
                "{}: adapted SVG must close its root element",
                icon.name
            );
            let (root, rest) = svg.split_once('>').ok_or("Missing SVG root")?;
            let view = format!("viewBox=\"0 0 {0} {0}\"", icon.view);
            assert!(root.contains(&view), "{}: view box changed", icon.name);
            let styles = root
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with("fill=") || line.starts_with("stroke"))
                .collect::<Vec<_>>()
                .join(" ");
            let inner = rest.strip_suffix('\n').unwrap_or(rest);
            let inner = inner
                .strip_suffix("</svg>")
                .ok_or("Missing SVG end")?
                .trim();
            let mut expected = String::from(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\">\n",
            );
            for line in inner.lines() {
                let element = line.trim().strip_suffix(" />").ok_or("Invalid element")?;
                let tag = element.split_whitespace().next().ok_or("Missing SVG tag")?;
                assert!(
                    matches!(tag, "<path" | "<rect" | "<circle" | "<ellipse" | "<line"),
                    "{}: element unsupported by the pinned Rune parser: {tag}",
                    icon.name
                );
                expected.push_str(element);
                expected.push(' ');
                expected.push_str(&styles);
                expected.push_str(" />\n");
            }
            expected.push_str("</svg>\n");
            assert_eq!(
                icon.body, expected,
                "{}: Lucide adaptation drifted",
                icon.name
            );
        }
        Ok(())
    }
}
