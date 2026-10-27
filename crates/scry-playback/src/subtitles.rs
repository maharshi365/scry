//! Subtitle preferences and bounded discovery through the playback adapter.
use crate::PlaybackEngine;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const SIZES: &[u16] = &[75, 100, 125, 150, 175, 200, 250];
pub const LANGUAGES: &[(&str, &str)] = &[
    ("ara", "Arabic"),
    ("ben", "Bengali"),
    ("bul", "Bulgarian"),
    ("zho", "Chinese"),
    ("hrv", "Croatian"),
    ("ces", "Czech"),
    ("dan", "Danish"),
    ("nld", "Dutch"),
    ("eng", "English"),
    ("est", "Estonian"),
    ("fin", "Finnish"),
    ("fra", "French"),
    ("deu", "German"),
    ("ell", "Greek"),
    ("heb", "Hebrew"),
    ("hin", "Hindi"),
    ("hun", "Hungarian"),
    ("ind", "Indonesian"),
    ("ita", "Italian"),
    ("jpn", "Japanese"),
    ("kor", "Korean"),
    ("lav", "Latvian"),
    ("lit", "Lithuanian"),
    ("msa", "Malay"),
    ("nor", "Norwegian"),
    ("fas", "Persian"),
    ("pol", "Polish"),
    ("por", "Portuguese"),
    ("ron", "Romanian"),
    ("rus", "Russian"),
    ("srp", "Serbian"),
    ("slk", "Slovak"),
    ("slv", "Slovenian"),
    ("spa", "Spanish"),
    ("swe", "Swedish"),
    ("tam", "Tamil"),
    ("tel", "Telugu"),
    ("tha", "Thai"),
    ("tur", "Turkish"),
    ("ukr", "Ukrainian"),
    ("urd", "Urdu"),
    ("vie", "Vietnamese"),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub size: u16,
    pub position: u8,
    pub font: String,
    pub bold: bool,
    pub text_color: String,
    pub background_color: String,
    pub outline_color: String,
    pub ass_styles: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            primary: Some("eng".into()),
            secondary: None,
            size: 100,
            position: 95,
            font: "sans-serif".into(),
            bold: false,
            text_color: "#FFFFFFFF".into(),
            background_color: "#00000000".into(),
            outline_color: "#FF000000".into(),
            ass_styles: true,
        }
    }
}

impl Preferences {
    /// # Errors
    /// Rejects malformed or out-of-range persisted preferences.
    pub fn validate(&self) -> Result<()> {
        for language in [&self.primary, &self.secondary].into_iter().flatten() {
            ensure!(
                LANGUAGES.iter().any(|(code, _)| code == language),
                "Invalid subtitle language"
            );
        }
        ensure!(
            SIZES.contains(&self.size) && self.position <= 100,
            "Invalid subtitle size or position"
        );
        ensure!(
            ["sans-serif", "serif", "monospace", "Arial", "Verdana"].contains(&self.font.as_str()),
            "Invalid subtitle font"
        );
        for color in [
            &self.text_color,
            &self.background_color,
            &self.outline_color,
        ] {
            ensure!(
                color.len() == 9
                    && color.starts_with('#')
                    && color.bytes().skip(1).all(|b| b.is_ascii_hexdigit()),
                "Invalid subtitle color"
            );
        }
        Ok(())
    }

    #[must_use]
    pub fn commands(&self) -> Vec<Vec<String>> {
        [
            ("sub-scale", (f64::from(self.size) / 100.0).to_string()),
            ("sub-pos", self.position.to_string()),
            ("secondary-sub-pos", "5".into()),
            ("sub-font", self.font.clone()),
            ("sub-bold", if self.bold { "yes" } else { "no" }.into()),
            ("sub-color", self.text_color.clone()),
            ("sub-back-color", self.background_color.clone()),
            ("sub-border-style", "background-box".into()),
            ("sub-border-color", self.outline_color.clone()),
            (
                "sub-ass-override",
                if self.ass_styles { "no" } else { "force" }.into(),
            ),
            (
                "secondary-sub-ass-override",
                if self.ass_styles { "no" } else { "force" }.into(),
            ),
        ]
        .into_iter()
        .map(|(key, value)| vec!["set".into(), key.into(), value])
        .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub id: i64,
    pub language: String,
    pub title: String,
    pub codec: String,
    pub external: bool,
}

impl Track {
    #[must_use]
    pub fn styled(&self) -> bool {
        matches!(self.codec.as_str(), "ass" | "ssa")
    }

    #[must_use]
    pub fn bitmap(&self) -> bool {
        matches!(
            self.codec.as_str(),
            "hdmv_pgs_subtitle" | "dvd_subtitle" | "dvb_subtitle" | "xsub"
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub tracks: Vec<Track>,
    pub selected: [Option<i64>; 2],
}

impl Snapshot {
    #[must_use]
    pub fn read(engine: &dyn PlaybackEngine) -> Self {
        let count = engine
            .property("track-list/count")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
            .min(256);
        let mut tracks = Vec::new();
        for index in 0..count {
            let get = |name: &str| engine.property(&format!("track-list/{index}/{name}"));
            if get("type").as_deref() != Some("sub") {
                continue;
            }
            let Some(id) = get("id").and_then(|v| v.parse().ok()) else {
                continue;
            };
            tracks.push(Track {
                id,
                language: get("lang").unwrap_or_default(),
                title: get("title").unwrap_or_default(),
                codec: get("codec").unwrap_or_default(),
                external: get("external").as_deref() == Some("yes"),
            });
        }
        Self {
            tracks,
            selected: ["sid", "secondary-sid"]
                .map(|key| engine.property(key).and_then(|v| v.parse().ok())),
        }
    }
}

#[must_use]
pub fn language_code(value: &str) -> String {
    let lower = value.trim().to_ascii_lowercase();
    let base = lower.split(['-', '_']).next().unwrap_or("");
    let code = match base {
        "en" => "eng",
        "es" => "spa",
        "fr" | "fre" => "fra",
        "de" | "ger" => "deu",
        "zh" | "chi" => "zho",
        "ja" => "jpn",
        "ko" => "kor",
        "ar" => "ara",
        "pt" => "por",
        "ru" => "rus",
        "it" => "ita",
        "nl" | "dut" => "nld",
        "cs" | "cze" => "ces",
        "el" | "gre" => "ell",
        "fa" | "per" => "fas",
        "ro" | "rum" => "ron",
        "sk" | "slo" => "slk",
        "ms" | "may" => "msa",
        "bn" => "ben",
        "bg" => "bul",
        "hr" => "hrv",
        "da" => "dan",
        "et" => "est",
        "fi" => "fin",
        "he" => "heb",
        "hi" => "hin",
        "hu" => "hun",
        "id" => "ind",
        "lv" => "lav",
        "lt" => "lit",
        "no" | "nb" | "nn" | "nob" | "nno" => "nor",
        "pl" => "pol",
        "sr" => "srp",
        "sl" => "slv",
        "sv" => "swe",
        "ta" => "tam",
        "te" => "tel",
        "th" => "tha",
        "tr" => "tur",
        "uk" => "ukr",
        "ur" => "urd",
        "vi" => "vie",
        _ => base,
    };
    code.to_owned()
}

#[must_use]
pub fn language_label(value: &str) -> String {
    let code = language_code(value);
    LANGUAGES.iter().find(|(key, _)| *key == code).map_or_else(
        || {
            if value.is_empty() {
                "Unknown language".into()
            } else {
                value.to_owned()
            }
        },
        |(_, label)| (*label).into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_preferences_are_rejected() {
        let mut prefs = Preferences::default();
        assert!(prefs.validate().is_ok());
        prefs.size = 0;
        assert!(prefs.validate().is_err());
        prefs.size = 100;
        prefs.text_color = "#private-url".into();
        assert!(prefs.validate().is_err());
        prefs.text_color = "#FFFFFFFF".into();
        prefs.primary = Some("not-a-language".into());
        assert!(prefs.validate().is_err());
    }

    #[test]
    fn language_aliases_and_regions_share_selection_language() {
        assert_eq!(language_code(" EN-us "), "eng");
        assert_eq!(language_code("fre"), "fra");
        assert_eq!(language_label("es"), "Spanish");
        assert_eq!(language_label(""), "Unknown language");
    }
}
