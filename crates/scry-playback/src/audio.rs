use crate::PlaybackEngine;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub id: i64,
    pub language: String,
    pub title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub tracks: Vec<Track>,
    pub selected: Option<i64>,
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
            if get("type").as_deref() != Some("audio") {
                continue;
            }
            let Some(id) = get("id")
                .and_then(|v| v.parse::<i64>().ok())
                .filter(|id| *id > 0)
            else {
                continue;
            };
            tracks.push(Track {
                id,
                language: get("lang").unwrap_or_default(),
                title: get("title").unwrap_or_default(),
            });
        }
        let selected = engine
            .property("aid")
            .and_then(|v| v.parse().ok())
            .filter(|id| tracks.iter().any(|track| track.id == *id));
        Self { tracks, selected }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlaybackEvent, telemetry::Sample};
    use std::{cell::Cell, collections::HashMap};

    #[derive(Default)]
    struct Engine {
        properties: HashMap<String, String>,
        reads: Cell<usize>,
    }

    impl PlaybackEngine for Engine {
        fn property(&self, name: &str) -> Option<String> {
            self.reads.set(self.reads.get() + 1);
            self.properties.get(name).cloned()
        }
        fn load(&self, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn command(&self, _: &[&str]) -> anyhow::Result<()> {
            Ok(())
        }
        fn event(&self, _: f64) -> Option<PlaybackEvent> {
            None
        }
        fn sample(&self, _: f64) -> Sample {
            Sample::default()
        }
    }

    fn engine(properties: &[(&str, &str)]) -> Engine {
        Engine {
            properties: properties
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
            ..Engine::default()
        }
    }

    #[test]
    fn discovers_only_valid_audio_tracks_and_reads_the_active_id() {
        let mut engine = engine(&[
            ("track-list/count", "5"),
            ("track-list/0/type", "video"),
            ("track-list/1/type", "audio"),
            ("track-list/1/id", "3"),
            ("track-list/1/lang", "eng"),
            ("track-list/1/title", "Main"),
            ("track-list/2/type", "sub"),
            ("track-list/3/type", "audio"),
            ("track-list/3/id", "9"),
            ("track-list/4/type", "audio"),
            ("track-list/4/id", "invalid"),
            ("aid", "9"),
        ]);
        let snapshot = Snapshot::read(&engine);
        assert_eq!(
            snapshot.tracks,
            vec![
                Track {
                    id: 3,
                    language: "eng".into(),
                    title: "Main".into()
                },
                Track {
                    id: 9,
                    language: String::new(),
                    title: String::new()
                },
            ]
        );
        assert_eq!(snapshot.selected, Some(9));
        for selected in ["no", "auto", "99"] {
            engine.properties.insert("aid".into(), selected.into());
            assert_eq!(Snapshot::read(&engine).selected, None);
        }
    }

    #[test]
    fn unavailable_tracks_are_empty_and_large_track_counts_are_bounded() {
        for count in ["", "invalid", "-1"] {
            assert_eq!(
                Snapshot::read(&engine(&[("track-list/count", count)])),
                Snapshot::default()
            );
        }
        let engine = engine(&[("track-list/count", "1000000")]);
        assert_eq!(Snapshot::read(&engine), Snapshot::default());
        assert_eq!(
            engine.reads.get(),
            258,
            "Count, at most 256 track types, and aid"
        );
    }
}
