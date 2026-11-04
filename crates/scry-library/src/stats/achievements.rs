use std::collections::HashMap;

use anyhow::Result;
use diesel::{
    prelude::*,
    sql_types::{BigInt, Bool, Nullable, Text},
};

use crate::{
    Library,
    stats::metrics::{count, number},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Evaluation {
    Unavailable,
    Progress { current: f64, target: f64 },
    Unlocked,
}

impl Evaluation {
    #[must_use]
    pub fn threshold(current: f64, target: f64) -> Self {
        if current >= target {
            Self::Unlocked
        } else {
            Self::Progress { current, target }
        }
    }
}

pub struct Achievement {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub requirement: &'static str,
    pub hidden: bool,
    pub evaluate: fn(&mut AchievementDb<'_>) -> Result<Evaluation>,
}

/// Snapshot queries with revision-keyed reuse across evaluation runs.
pub struct AchievementDb<'a> {
    connection: &'a mut SqliteConnection,
    cache: &'a mut MetricCache,
    revision: Revisions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Revision([i64; 4]);

pub(crate) type MetricCache = HashMap<&'static str, (Revision, f64)>;

#[derive(QueryableByName)]
struct Revisions {
    #[diesel(sql_type = BigInt)]
    library: i64,
    #[diesel(sql_type = BigInt)]
    playback: i64,
    #[diesel(sql_type = BigInt)]
    hours: i64,
    #[diesel(sql_type = BigInt)]
    release: i64,
}

impl AchievementDb<'_> {
    fn metric(&mut self, query: &'static str) -> Result<f64> {
        self.cached(
            query,
            Revision([
                self.revision.library,
                self.revision.playback,
                self.revision.hours,
                self.revision.release,
            ]),
        )
    }

    fn playback_metric(&mut self, query: &'static str) -> Result<f64> {
        self.cached(query, Revision([0, self.revision.playback, 0, 0]))
    }

    fn library_metric(&mut self, query: &'static str) -> Result<f64> {
        self.cached(
            query,
            Revision([self.revision.library, 0, 0, self.revision.release]),
        )
    }

    fn hours_metric(&mut self, query: &'static str) -> Result<f64> {
        self.cached(query, Revision([0, 0, self.revision.hours, 0]))
    }

    fn cached(&mut self, query: &'static str, revision: Revision) -> Result<f64> {
        if let Some(&(previous, value)) = self.cache.get(query)
            && previous == revision
        {
            return Ok(value);
        }
        let value = number(self.connection, query)?;
        self.cache.insert(query, (revision, value));
        Ok(value)
    }

    /// # Errors
    /// Returns an error if playback totals cannot be read.
    pub fn total_watch_seconds(&mut self) -> Result<f64> {
        self.metric("SELECT movie_seconds+tv_seconds AS value FROM watch_totals WHERE id=1")
    }

    /// # Errors
    /// Returns an error if episode state cannot be read.
    pub fn episodes_watched(&mut self) -> Result<f64> {
        self.library_metric(
            "SELECT CAST(episodes_watched AS REAL) AS value FROM watch_totals WHERE id=1",
        )
    }

    /// # Errors
    /// Returns an error if series state cannot be read.
    pub fn completed_series_count(&mut self) -> Result<f64> {
        self.library_metric("SELECT CAST(COUNT(*) AS REAL) AS value FROM series_facts WHERE total>0 AND watched=total AND (last_release IS NULL OR last_release<=unixepoch())")
    }

    /// # Errors
    /// Returns an error if season state cannot be read.
    pub fn completed_season_count(&mut self) -> Result<f64> {
        self.library_metric("SELECT CAST(COUNT(*) AS REAL) AS value FROM (SELECT m.parent_id,r.season FROM media m JOIN episode_releases r USING(media_id) LEFT JOIN watched w USING(media_id) WHERE m.kind='episode' AND r.season>0 GROUP BY m.parent_id,r.season HAVING COUNT(*)>0 AND MIN(COALESCE(w.watched,0))=1 AND MIN(COALESCE(r.released_at<=unixepoch(),0))=1)")
    }
}

#[derive(Clone, Debug)]
pub struct AchievementEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub requirement: &'static str,
    pub hidden: bool,
    pub unlocked: bool,
    pub historical: bool,
    pub unlocked_at: Option<i64>,
    pub progress: Evaluation,
}

#[derive(Clone, Debug, Default)]
pub struct AchievementEvaluation {
    pub entries: Vec<AchievementEntry>,
    pub newly_unlocked: Vec<AchievementEntry>,
}

macro_rules! achievement {
    ($id:literal, $name:literal, $description:literal, $requirement:literal, $hidden:literal, $rule:expr) => {
        Achievement {
            id: $id,
            name: $name,
            description: $description,
            requirement: $requirement,
            hidden: $hidden,
            evaluate: $rule,
        }
    };
}

const ACHIEVEMENTS: &[Achievement] = &[
    achievement!(
        "pilot-error",
        "Pilot Error",
        "This is how it starts.",
        "Watch an episode.",
        false,
        |db| Ok(Evaluation::threshold(db.episodes_watched()?, 1.0))
    ),
    achievement!(
        "seasoned-professional",
        "Seasoned Professional",
        "A productive day, by your standards.",
        "Finish a known, released season.",
        false,
        |db| Ok(Evaluation::threshold(db.completed_season_count()?, 1.0))
    ),
    achievement!(
        "closure",
        "Closure",
        "Now find a new personality.",
        "Finish a series using its known episode inventory.",
        false,
        |db| Ok(Evaluation::threshold(db.completed_series_count()?, 1.0))
    ),
    achievement!(
        "a-day-well-spent",
        "A Day Well Spent",
        "Well. A day spent.",
        "Accumulate 24 watch hours.",
        false,
        |db| Ok(Evaluation::threshold(
            db.total_watch_seconds()? / 3600.0,
            24.0
        ))
    ),
    achievement!(
        "centurion",
        "Centurion",
        "You’ve really made yourself comfortable.",
        "Accumulate 100 watch hours.",
        false,
        |db| Ok(Evaluation::threshold(
            db.total_watch_seconds()? / 3600.0,
            100.0
        ))
    ),
    achievement!(
        "thousand-yard-stare",
        "Thousand-Hour Stare",
        "The screen has started watching you back.",
        "Accumulate 1,000 watch hours.",
        false,
        |db| Ok(Evaluation::threshold(
            db.total_watch_seconds()? / 3600.0,
            1000.0
        ))
    ),
    achievement!(
        "expert",
        "The 10,000-Hour Expert",
        "One skill mastered. It’s watching television.",
        "Accumulate 10,000 watch hours.",
        false,
        |db| Ok(Evaluation::threshold(
            db.total_watch_seconds()? / 3600.0,
            10_000.0
        ))
    ),
    achievement!(
        "just-browsing",
        "Just Browsing",
        "A collection of good intentions.",
        "Have 50 unwatched movies or shows saved.",
        false,
        |db| {
            Ok(Evaluation::threshold(db.library_metric("SELECT CAST(COUNT(*) AS REAL) AS value FROM watchlist l JOIN media m USING(media_id) LEFT JOIN watched w USING(media_id) LEFT JOIN series_facts s USING(media_id) WHERE CASE WHEN m.kind='series' THEN NOT COALESCE(s.total>0 AND s.watched=s.total AND (s.last_release IS NULL OR s.last_release<=unixepoch()),0) ELSE COALESCE(w.watched,0)=0 END")?, 50.0))
        }
    ),
    achievement!(
        "comfort-zone",
        "Comfort Zone",
        "The plot has not changed.",
        "Play more than 70% of the same title in 3 separate playback runs.",
        false,
        |db| Ok(Evaluation::threshold(
            db.playback_metric(
                "SELECT CAST(COALESCE(MAX(completions),0) AS REAL) AS value FROM stats_titles"
            )?,
            3.0
        ))
    ),
    achievement!(
        "one-more-episode",
        "One More Episode",
        "Your last four promises meant nothing.",
        "Play more than 70% of 5 episodes in one session. A 30-minute idle gap ends a session.",
        false,
        |db| Ok(Evaluation::threshold(
            db.playback_metric(
                "SELECT CAST(COALESCE(MAX(episodes),0) AS REAL) AS value FROM stats_sessions"
            )?,
            5.0
        ))
    ),
    achievement!(
        "night-shift",
        "Night Shift",
        "Tomorrow is someone else’s problem.",
        "Record playback between 2 a.m. and 5 a.m., in your local time.",
        true,
        |db| {
            Ok(Evaluation::threshold(
                number(
                    db.connection,
                    "SELECT COALESCE(SUM(seconds),0.0) AS value FROM stats_hours WHERE CAST(strftime('%H',hour,'unixepoch','localtime') AS INTEGER) BETWEEN 2 AND 4",
                )?,
                1.0,
            ))
        }
    ),
    achievement!(
        "full-time-viewer",
        "Full-Time Viewer",
        "Unfortunately, this position is unpaid.",
        "Accumulate 40 watch hours across any 7 consecutive UTC calendar days. Undated history counts toward all-time milestones instead.",
        false,
        |db| {
            Ok(Evaluation::threshold(db.metric("SELECT COALESCE(MAX(total),0.0) AS value FROM (SELECT SUM(SUM(seconds)) OVER (ORDER BY day RANGE BETWEEN 6 PRECEDING AND CURRENT ROW) AS total FROM (SELECT day,seconds FROM stats_days UNION ALL SELECT watched_at/86400 AS day,credit AS seconds FROM stats_counted WHERE watched_at IS NOT NULL AND credit>0) GROUP BY day)")? / 3600.0, 40.0))
        }
    ),
    achievement!(
        "double-feature",
        "Double Feature",
        "The living room has a second screening.",
        "Record more than 70% of each of two different movies in one UTC calendar day. Known runtimes are required; manual watched marks do not count.",
        true,
        |db| {
            Ok(Evaluation::threshold(db.metric("SELECT CAST(COALESCE(MAX(movies),0) AS REAL) AS value FROM (SELECT COUNT(*) AS movies FROM (SELECT h.hour/86400 AS day,h.media_id FROM stats_hours h JOIN media m USING(media_id) JOIN media_runtime r USING(media_id) WHERE m.kind='movie' GROUP BY h.hour/86400,h.media_id HAVING SUM(h.seconds)>r.seconds*0.7) GROUP BY day)")?, 2.0))
        }
    ),
    achievement!(
        "channel-surfer",
        "Channel Surfer",
        "Three fictional universes. One sofa.",
        "Record at least 20 minutes of episodes from each of three different series in one UTC calendar day. Manual watched marks do not count.",
        true,
        |db| {
            Ok(Evaluation::threshold(db.metric("SELECT CAST(COALESCE(MAX(shows),0) AS REAL) AS value FROM (SELECT COUNT(*) AS shows FROM (SELECT h.hour/86400 AS day,m.parent_id FROM stats_hours h CROSS JOIN media m USING(media_id) JOIN media p ON p.media_id=m.parent_id WHERE m.kind='episode' AND p.kind='series' GROUP BY h.hour/86400,m.parent_id HAVING SUM(h.seconds)>=1200) GROUP BY day)")?, 3.0))
        }
    ),
    achievement!(
        "standing-appointment",
        "Standing Appointment",
        "The sofa has put you on the calendar.",
        "Record at least 20 minutes of playback on each of seven consecutive UTC calendar days. Manual watched marks do not count.",
        true,
        |db| {
            Ok(Evaluation::threshold(db.hours_metric("SELECT CAST(COALESCE(MAX(days),0) AS REAL) AS value FROM (SELECT COUNT(*) AS days FROM (SELECT day-ROW_NUMBER() OVER (ORDER BY day) AS streak FROM stats_days WHERE seconds>=1200) GROUP BY streak)")?, 7.0))
        }
    ),
    achievement!(
        "silver-screen",
        "Silver Screen",
        "Your own little film festival.",
        "Record a completed playback run for 25 different movies. A run needs more than 70% of the runtime played; manual watched marks do not count.",
        false,
        |db| {
            Ok(Evaluation::threshold(db.metric("SELECT CAST(COUNT(*) AS REAL) AS value FROM stats_titles t JOIN media m USING(media_id) WHERE m.kind='movie' AND t.completions>0")?, 25.0))
        }
    ),
    achievement!(
        "encore-club",
        "Encore Club",
        "Some stories deserve another round.",
        "Record at least two completed playback runs for each of 5 different movies or episodes. Each run needs more than 70% of the runtime played; manual watched marks do not count.",
        false,
        |db| {
            Ok(Evaluation::threshold(db.metric("SELECT CAST(COUNT(*) AS REAL) AS value FROM stats_titles t CROSS JOIN media m USING(media_id) WHERE m.kind IN ('movie','episode') AND t.completions>=2")?, 5.0))
        }
    ),
    achievement!(
        "roll-credits",
        "Roll Credits",
        "For once, the list is not winning.",
        "Have at least 10 movies saved, with every saved movie watched. Saved series do not affect this achievement.",
        true,
        |db| {
            Ok(Evaluation::threshold(db.metric("SELECT CAST(COUNT(*)>=10 AND COALESCE(MIN(COALESCE(w.watched,0)),0)=1 AS REAL) AS value FROM watchlist l JOIN media m USING(media_id) LEFT JOIN watched w USING(media_id) WHERE m.kind='movie'")?, 1.0))
        }
    ),
    achievement!(
        "intermission",
        "Intermission",
        "There was time for a whole other movie.",
        "Record more than 70% of a movie whose known runtime is at least three hours. Recorded time can span multiple runs; manual watched marks do not count.",
        true,
        |db| {
            Ok(Evaluation::threshold(db.metric("SELECT CAST(COUNT(*) AS REAL) AS value FROM stats_titles t JOIN media m USING(media_id) JOIN media_runtime r USING(media_id) WHERE m.kind='movie' AND r.seconds>=10800 AND t.seconds*10>r.seconds*7")?, 1.0))
        }
    ),
];

#[derive(QueryableByName)]
struct Saved {
    #[diesel(sql_type = Text)]
    id: String,
    #[diesel(sql_type = Bool)]
    unlocked: bool,
    #[diesel(sql_type = Bool)]
    historical: bool,
    #[diesel(sql_type = Nullable<BigInt>)]
    unlocked_at: Option<i64>,
}

struct Snapshot {
    result: AchievementEvaluation,
    needs_persist: bool,
}

fn evaluate_snapshot(
    connection: &mut SqliteConnection,
    definitions: &[Achievement],
    cache: &mut MetricCache,
) -> Result<Snapshot> {
    let saved_rows =
        diesel::sql_query("SELECT id,unlocked,historical,unlocked_at FROM achievement_checks")
            .load::<Saved>(connection)?;
    let saved: HashMap<_, _> = saved_rows
        .iter()
        .map(|entry| (entry.id.as_str(), entry))
        .collect();
    let revision = diesel::sql_query("SELECT library,playback,hours,COALESCE((SELECT MAX(released_at) FROM episode_releases WHERE released_at<=unixepoch()),-9223372036854775808) AS release FROM source_revisions WHERE id=1")
        .get_result::<Revisions>(connection)?;
    let mut db = AchievementDb {
        connection,
        cache,
        revision,
    };
    let mut result = AchievementEvaluation::default();
    let mut needs_persist = false;
    for definition in definitions {
        let previous = saved.get(definition.id);
        let earned = previous.is_some_and(|entry| entry.unlocked);
        // Earned rules are never called, even to calculate collection progress.
        let progress = if earned {
            Evaluation::Unlocked
        } else {
            (definition.evaluate)(&mut db)?
        };
        let unlocked = progress == Evaluation::Unlocked;
        needs_persist |= previous.is_none() || (unlocked && !earned);
        let historical = previous.map_or(unlocked, |entry| entry.historical);
        let unlocked_at = previous.and_then(|entry| entry.unlocked_at);
        let entry = AchievementEntry {
            id: definition.id,
            name: definition.name,
            description: definition.description,
            requirement: definition.requirement,
            hidden: definition.hidden,
            unlocked,
            historical,
            unlocked_at,
            progress,
        };
        if unlocked && !earned {
            result.newly_unlocked.push(entry.clone());
        }
        result.entries.push(entry);
    }
    Ok(Snapshot {
        result,
        needs_persist,
    })
}

fn persist(
    connection: &mut SqliteConnection,
    mut result: AchievementEvaluation,
) -> Result<AchievementEvaluation> {
    let saved_rows =
        diesel::sql_query("SELECT id,unlocked,historical,unlocked_at FROM achievement_checks")
            .load::<Saved>(connection)?;
    let saved: HashMap<_, _> = saved_rows
        .iter()
        .map(|entry| (entry.id.as_str(), entry))
        .collect();
    result.newly_unlocked.clear();
    for entry in &mut result.entries {
        let previous = saved.get(entry.id);
        // Another evaluator may have committed since our read snapshot. Unlocks
        // are monotonic, and only the winning writer announces a transition.
        if let Some(previous) = previous.filter(|previous| previous.unlocked) {
            entry.unlocked = true;
            entry.progress = Evaluation::Unlocked;
            entry.historical = previous.historical;
            entry.unlocked_at = previous.unlocked_at;
            continue;
        }
        entry.historical = previous.map_or(entry.unlocked, |previous| previous.historical);
        if entry.unlocked && !entry.historical {
            entry.unlocked_at = Some(count(connection, "SELECT unixepoch() AS value")?);
        }
        if previous.is_none() || entry.unlocked {
            diesel::sql_query("INSERT INTO achievement_checks(id,unlocked,historical,unlocked_at) VALUES (?,?,?,?) ON CONFLICT(id) DO UPDATE SET unlocked=excluded.unlocked,historical=excluded.historical,unlocked_at=excluded.unlocked_at")
                .bind::<Text,_>(entry.id).bind::<Bool,_>(entry.unlocked).bind::<Bool,_>(entry.historical)
                .bind::<Nullable<BigInt>,_>(entry.unlocked_at).execute(connection)?;
        }
        if entry.unlocked {
            result.newly_unlocked.push(entry.clone());
        }
    }
    Ok(result)
}

impl Library {
    /// Evaluate locked rules on a read snapshot, then commit unlocks in a short write.
    /// A definition's first check is catch-up; later unlocks are live.
    /// # Errors
    /// Returns an error if a rule query or unlock write fails. No partial unlocks are saved.
    pub fn evaluate_achievements(&mut self) -> Result<AchievementEvaluation> {
        let mut cache = std::mem::take(&mut self.achievement_cache);
        let result = self
            .transaction(|connection| evaluate_snapshot(connection, ACHIEVEMENTS, &mut cache))
            .and_then(|snapshot| {
                if snapshot.needs_persist {
                    self.immediate_transaction(|connection| persist(connection, snapshot.result))
                } else {
                    Ok(snapshot.result)
                }
            });
        if result.is_ok() {
            self.achievement_cache = cache;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::connection::SimpleConnection;

    fn seed(library: &Library, sql: &str) -> Result<()> {
        library.immediate_transaction(|connection| {
            connection.batch_execute(sql)?;
            Ok(())
        })
    }

    fn assert_hidden(library: &mut Library, id: &str, unlocked: bool) -> Result<()> {
        let result = library.evaluate_achievements()?;
        let entry = result.entries.iter().find(|entry| entry.id == id);
        assert!(
            entry.is_some_and(|entry| entry.hidden && entry.unlocked == unlocked),
            "hidden achievement {id} should have unlocked={unlocked}"
        );
        if unlocked {
            assert!(result.newly_unlocked.iter().any(|entry| {
                entry.id == id && !entry.historical && entry.unlocked_at.is_some()
            }));
            assert!(library.evaluate_achievements()?.newly_unlocked.is_empty());
        }
        Ok(())
    }

    #[test]
    fn double_feature_needs_two_distinct_movies_over_threshold_on_the_same_day() -> Result<()> {
        let mut library = Library::memory()?;
        seed(
            &library,
            "
            INSERT INTO media(media_id,namespace,kind,external_id,title) VALUES
                (1,'test','movie','one','One'),(2,'test','movie','two','Two'),
                (3,'test','episode','episode','Episode');
            INSERT INTO media_runtime(media_id,seconds) VALUES (1,6000),(2,6000),(3,6000);
            INSERT INTO watched VALUES (1,1),(2,1);
        ",
        )?;
        assert_hidden(&mut library, "double-feature", false)?;
        seed(
            &library,
            "
            INSERT INTO stats_hours VALUES (86400,1,5000),(90000,1,5000),
                (86400,3,5000),(86400,2,4200),(172800,2,5000);
        ",
        )?;
        assert_hidden(&mut library, "double-feature", false)?;
        seed(&library, "INSERT INTO stats_hours VALUES (90000,2,1);")?;
        assert_hidden(&mut library, "double-feature", true)
    }

    #[test]
    fn channel_surfer_groups_episodes_by_series_and_day() -> Result<()> {
        let mut library = Library::memory()?;
        seed(
            &library,
            "
            INSERT INTO media(media_id,namespace,kind,external_id,title) VALUES
                (1,'test','series','one','One'),(2,'test','series','two','Two'),
                (3,'test','series','three','Three'),(4,'test','movie','movie','Movie');
            INSERT INTO media(media_id,namespace,kind,external_id,title,parent_id) VALUES
                (5,'test','episode','a','A',1),(6,'test','episode','b','B',1),
                (7,'test','episode','c','C',2),(8,'test','episode','d','D',3);
            INSERT INTO stats_hours VALUES (86400,5,600),(90000,6,600),
                (86400,7,1200),(86400,8,1199),(172800,8,1200),(86400,4,1200);
        ",
        )?;
        assert_hidden(&mut library, "channel-surfer", false)?;
        seed(&library, "INSERT INTO stats_hours VALUES (90000,8,1);")?;
        assert_hidden(&mut library, "channel-surfer", true)
    }

    #[test]
    fn standing_appointment_needs_seven_consecutive_qualifying_days() -> Result<()> {
        let mut library = Library::memory()?;
        seed(
            &library,
            "
            INSERT INTO media(media_id,namespace,kind,external_id,title) VALUES
                (1,'test','movie','one','One');
            INSERT INTO stats_hours VALUES (86400,1,1200),(172800,1,1200),
                (259200,1,1200),(345600,1,1199),(432000,1,1200),
                (518400,1,1200),(604800,1,1200),(691200,1,1200);
        ",
        )?;
        assert_hidden(&mut library, "standing-appointment", false)?;
        seed(&library, "INSERT INTO stats_hours VALUES (349200,1,1);")?;
        assert_hidden(&mut library, "standing-appointment", true)
    }

    fn assert_visible_progress(
        library: &mut Library,
        id: &str,
        current: f64,
        target: f64,
    ) -> Result<()> {
        let result = library.evaluate_achievements()?;
        let entry = result
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| anyhow::anyhow!("missing achievement {id}"))?;
        assert!(
            !entry.hidden,
            "{id} should show its requirement and progress"
        );
        assert_eq!(entry.progress, Evaluation::threshold(current, target));
        assert_eq!(entry.unlocked, current >= target);
        if entry.unlocked {
            assert!(result.newly_unlocked.iter().any(|entry| {
                entry.id == id && !entry.historical && entry.unlocked_at.is_some()
            }));
            assert!(library.evaluate_achievements()?.newly_unlocked.is_empty());
        }
        Ok(())
    }

    #[test]
    fn silver_screen_counts_distinct_recorded_movie_completions() -> Result<()> {
        let mut library = Library::memory()?;
        assert_visible_progress(&mut library, "silver-screen", 0.0, 25.0)?;
        seed(
            &library,
            "
            WITH RECURSIVE n(id) AS (VALUES(1) UNION ALL SELECT id+1 FROM n WHERE id<25)
            INSERT INTO media(media_id,namespace,kind,external_id,title)
                SELECT id,'test','movie',CAST(id AS TEXT),'Movie' FROM n;
            INSERT INTO watched SELECT media_id,1 FROM media;
            INSERT INTO media(media_id,namespace,kind,external_id,title) VALUES
                (26,'test','episode','episode','Episode'),(27,'test','series','series','Series');
            INSERT INTO stats_titles(media_id,seconds,completions)
                SELECT media_id,100,3 FROM media WHERE media_id<>25;
        ",
        )?;
        assert_visible_progress(&mut library, "silver-screen", 24.0, 25.0)?;
        seed(
            &library,
            "INSERT INTO stats_titles(media_id,seconds,completions) VALUES (25,100,1);",
        )?;
        assert_visible_progress(&mut library, "silver-screen", 25.0, 25.0)
    }

    #[test]
    fn encore_club_needs_repeat_completions_not_just_rewatch_seconds() -> Result<()> {
        let mut library = Library::memory()?;
        assert_visible_progress(&mut library, "encore-club", 0.0, 5.0)?;
        seed(
            &library,
            "
            WITH RECURSIVE n(id) AS (VALUES(1) UNION ALL SELECT id+1 FROM n WHERE id<5)
            INSERT INTO media(media_id,namespace,kind,external_id,title)
                SELECT id,'test',CASE WHEN id%2=0 THEN 'episode' ELSE 'movie' END,
                    CAST(id AS TEXT),'Title' FROM n;
            INSERT INTO media(media_id,namespace,kind,external_id,title)
                VALUES (6,'test','series','series','Series');
            INSERT INTO stats_titles(media_id,seconds,completions,rewatch_seconds)
                SELECT media_id,1000,CASE WHEN media_id=5 THEN 1 ELSE 3 END,500 FROM media;
        ",
        )?;
        assert_visible_progress(&mut library, "encore-club", 4.0, 5.0)?;
        seed(
            &library,
            "UPDATE stats_titles SET completions=2 WHERE media_id=5;",
        )?;
        assert_visible_progress(&mut library, "encore-club", 5.0, 5.0)
    }

    #[test]
    fn roll_credits_needs_a_nontrivial_fully_watched_saved_movie_list() -> Result<()> {
        let mut library = Library::memory()?;
        assert_hidden(&mut library, "roll-credits", false)?;
        seed(
            &library,
            "
            WITH RECURSIVE n(id) AS (VALUES(1) UNION ALL SELECT id+1 FROM n WHERE id<11)
            INSERT INTO media(media_id,namespace,kind,external_id,title)
                SELECT id,'test','movie',CAST(id AS TEXT),'Movie' FROM n;
            INSERT INTO watched SELECT media_id,1 FROM media WHERE media_id<>10;
            INSERT INTO watchlist(media_id) SELECT media_id FROM media WHERE media_id<10;
            INSERT INTO media(media_id,namespace,kind,external_id,title)
                VALUES (12,'test','series','series','Series');
            INSERT INTO watchlist(media_id) VALUES (12);
        ",
        )?;
        assert_hidden(&mut library, "roll-credits", false)?;
        seed(&library, "INSERT INTO watchlist(media_id) VALUES (10);")?;
        assert_hidden(&mut library, "roll-credits", false)?;
        seed(&library, "INSERT INTO watched VALUES (10,1);")?;
        assert_hidden(&mut library, "roll-credits", true)
    }

    #[test]
    fn intermission_needs_known_long_movie_runtime_and_recorded_time() -> Result<()> {
        let mut library = Library::memory()?;
        assert_hidden(&mut library, "intermission", false)?;
        seed(
            &library,
            "
            INSERT INTO media(media_id,namespace,kind,external_id,title) VALUES
                (1,'test','movie','long','Long'),(2,'test','movie','short','Short'),
                (3,'test','movie','unknown','Unknown'),(4,'test','episode','episode','Episode');
            INSERT INTO media_runtime(media_id,seconds) VALUES (1,10800),(2,10799),(4,10800);
            INSERT INTO watched VALUES (1,1),(2,1),(3,1);
        ",
        )?;
        assert_hidden(&mut library, "intermission", false)?;
        seed(
            &library,
            "
            INSERT INTO stats_titles(media_id,seconds) VALUES
                (1,7560),(2,10800),(3,10800),(4,10800);
        ",
        )?;
        assert_hidden(&mut library, "intermission", false)?;
        seed(
            &library,
            "UPDATE stats_titles SET seconds=7561 WHERE media_id=1;",
        )?;
        assert_hidden(&mut library, "intermission", true)
    }

    fn forbidden(_: &mut AchievementDb<'_>) -> Result<Evaluation> {
        anyhow::bail!("an earned achievement must not be evaluated")
    }

    fn evaluate(
        connection: &mut SqliteConnection,
        definitions: &[Achievement],
    ) -> Result<AchievementEvaluation> {
        let result = evaluate_snapshot(connection, definitions, &mut MetricCache::new())?;
        persist(connection, result.result)
    }

    #[test]
    fn earned_rules_are_skipped_and_new_definitions_get_one_catch_up() -> Result<()> {
        let library = Library::memory()?;
        library.with_connection(|connection| {
            let mut definition = achievement!("test", "Test", "Test", "Test", false, |_| Ok(
                Evaluation::Unlocked
            ));
            assert_eq!(
                evaluate(connection, std::slice::from_ref(&definition))?
                    .newly_unlocked
                    .len(),
                1
            );
            definition.evaluate = forbidden;
            let result = evaluate(connection, &[definition])?;
            assert!(result.newly_unlocked.is_empty());
            assert!(
                result
                    .entries
                    .first()
                    .is_some_and(|entry| entry.unlocked && entry.historical)
            );
            Ok(())
        })
    }
}
