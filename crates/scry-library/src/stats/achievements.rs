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

/// Read-only, on-demand query helpers. Cached values live for one evaluation run.
pub struct AchievementDb<'a> {
    connection: &'a mut SqliteConnection,
    cache: HashMap<&'static str, f64>,
}

impl AchievementDb<'_> {
    fn metric(&mut self, query: &'static str) -> Result<f64> {
        if let Some(value) = self.cache.get(query) {
            return Ok(*value);
        }
        let value = number(self.connection, query)?;
        self.cache.insert(query, value);
        Ok(value)
    }

    /// # Errors
    /// Returns an error if playback totals cannot be read.
    pub fn total_watch_seconds(&mut self) -> Result<f64> {
        self.metric("SELECT COALESCE(SUM(recorded_seconds+credit),0.0) AS value FROM stats_counted")
    }

    /// # Errors
    /// Returns an error if episode state cannot be read.
    pub fn episodes_watched(&mut self) -> Result<f64> {
        self.metric("SELECT CAST(COUNT(*) AS REAL) AS value FROM media m JOIN effective_watched w USING(media_id) WHERE m.kind='episode' AND w.watched=1")
    }

    /// # Errors
    /// Returns an error if series state cannot be read.
    pub fn completed_series_count(&mut self) -> Result<f64> {
        self.metric("SELECT CAST(COUNT(*) AS REAL) AS value FROM media m JOIN effective_watched w USING(media_id) WHERE m.kind='series' AND w.watched=1")
    }

    /// # Errors
    /// Returns an error if season state cannot be read.
    pub fn completed_season_count(&mut self) -> Result<f64> {
        self.metric("SELECT CAST(COUNT(*) AS REAL) AS value FROM (SELECT m.parent_id,r.season FROM media m JOIN episode_releases r USING(media_id) LEFT JOIN effective_watched w USING(media_id) WHERE m.kind='episode' AND r.season>0 GROUP BY m.parent_id,r.season HAVING COUNT(*)>0 AND MIN(COALESCE(w.watched,0))=1 AND MIN(COALESCE(r.released_at<=unixepoch(),0))=1)")
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
            Ok(Evaluation::threshold(db.metric("SELECT CAST(COUNT(*) AS REAL) AS value FROM watchlist l JOIN effective_watched w USING(media_id) WHERE w.watched=0")?, 50.0))
        }
    ),
    achievement!(
        "comfort-zone",
        "Comfort Zone",
        "The plot has not changed.",
        "Play more than 70% of the same title in 3 separate playback runs.",
        false,
        |db| Ok(Evaluation::threshold(
            db.metric(
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
            db.metric(
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
            Ok(Evaluation::threshold(db.metric("SELECT COALESCE(SUM(seconds),0.0) AS value FROM stats_hours WHERE CAST(strftime('%H',hour,'unixepoch','localtime') AS INTEGER) BETWEEN 2 AND 4")?, 1.0))
        }
    ),
    achievement!(
        "full-time-viewer",
        "Full-Time Viewer",
        "Unfortunately, this position is unpaid.",
        "Accumulate 40 watch hours across any 7 consecutive UTC calendar days. Undated history counts toward all-time milestones instead.",
        false,
        |db| {
            Ok(Evaluation::threshold(db.metric("SELECT COALESCE(MAX(total),0.0) AS value FROM (SELECT SUM(SUM(seconds)) OVER (ORDER BY hour/86400 RANGE BETWEEN 6 PRECEDING AND CURRENT ROW) AS total FROM (SELECT hour,seconds FROM stats_hours UNION ALL SELECT watched_at AS hour,credit AS seconds FROM stats_counted WHERE watched_at IS NOT NULL AND credit>0) GROUP BY hour/86400)")? / 3600.0, 40.0))
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
            Ok(Evaluation::threshold(db.metric("SELECT CAST(COALESCE(MAX(shows),0) AS REAL) AS value FROM (SELECT COUNT(*) AS shows FROM (SELECT h.hour/86400 AS day,m.parent_id FROM stats_hours h JOIN media m USING(media_id) JOIN media p ON p.media_id=m.parent_id WHERE m.kind='episode' AND p.kind='series' GROUP BY h.hour/86400,m.parent_id HAVING SUM(h.seconds)>=1200) GROUP BY day)")?, 3.0))
        }
    ),
    achievement!(
        "standing-appointment",
        "Standing Appointment",
        "The sofa has put you on the calendar.",
        "Record at least 20 minutes of playback on each of seven consecutive UTC calendar days. Manual watched marks do not count.",
        true,
        |db| {
            Ok(Evaluation::threshold(db.metric("SELECT CAST(COALESCE(MAX(days),0) AS REAL) AS value FROM (SELECT COUNT(*) AS days FROM (SELECT day-ROW_NUMBER() OVER (ORDER BY day) AS streak FROM (SELECT hour/86400 AS day FROM stats_hours GROUP BY hour/86400 HAVING SUM(seconds)>=1200)) GROUP BY streak)")?, 7.0))
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

fn evaluate(
    connection: &mut SqliteConnection,
    definitions: &[Achievement],
) -> Result<AchievementEvaluation> {
    let saved: HashMap<_, _> =
        diesel::sql_query("SELECT id,unlocked,historical,unlocked_at FROM achievement_checks")
            .load::<Saved>(connection)?
            .into_iter()
            .map(|entry| (entry.id.clone(), entry))
            .collect();
    let mut db = AchievementDb {
        connection,
        cache: HashMap::new(),
    };
    let mut result = AchievementEvaluation::default();
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
        let historical = previous.map_or(unlocked, |entry| entry.historical);
        let mut unlocked_at = previous.and_then(|entry| entry.unlocked_at);
        if !earned {
            if unlocked && !historical {
                unlocked_at = Some(count(db.connection, "SELECT unixepoch() AS value")?);
            }
            diesel::sql_query("INSERT INTO achievement_checks(id,unlocked,historical,unlocked_at) VALUES (?,?,?,?) ON CONFLICT(id) DO UPDATE SET unlocked=excluded.unlocked,historical=excluded.historical,unlocked_at=excluded.unlocked_at")
                .bind::<Text,_>(definition.id).bind::<Bool,_>(unlocked).bind::<Bool,_>(historical)
                .bind::<Nullable<BigInt>,_>(unlocked_at).execute(db.connection)?;
        }
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
    Ok(result)
}

impl Library {
    /// Evaluate only locked rules, atomically saving first checks and new unlocks.
    /// A definition's first check is catch-up; later unlocks are live.
    /// # Errors
    /// Returns an error if a rule query or unlock write fails. No partial unlocks are saved.
    pub fn evaluate_achievements(&mut self) -> Result<AchievementEvaluation> {
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| evaluate(connection, ACHIEVEMENTS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::connection::SimpleConnection;

    fn seed(connection: &mut SqliteConnection, sql: &str) -> Result<()> {
        connection.batch_execute(sql)?;
        Ok(())
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
            &mut library.connection.borrow_mut(),
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
            &mut library.connection.borrow_mut(),
            "
            INSERT INTO stats_hours VALUES (86400,1,5000),(90000,1,5000),
                (86400,3,5000),(86400,2,4200),(172800,2,5000);
        ",
        )?;
        assert_hidden(&mut library, "double-feature", false)?;
        seed(
            &mut library.connection.borrow_mut(),
            "INSERT INTO stats_hours VALUES (90000,2,1);",
        )?;
        assert_hidden(&mut library, "double-feature", true)
    }

    #[test]
    fn channel_surfer_groups_episodes_by_series_and_day() -> Result<()> {
        let mut library = Library::memory()?;
        seed(
            &mut library.connection.borrow_mut(),
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
        seed(
            &mut library.connection.borrow_mut(),
            "INSERT INTO stats_hours VALUES (90000,8,1);",
        )?;
        assert_hidden(&mut library, "channel-surfer", true)
    }

    #[test]
    fn standing_appointment_needs_seven_consecutive_qualifying_days() -> Result<()> {
        let mut library = Library::memory()?;
        seed(
            &mut library.connection.borrow_mut(),
            "
            INSERT INTO media(media_id,namespace,kind,external_id,title) VALUES
                (1,'test','movie','one','One');
            INSERT INTO stats_hours VALUES (86400,1,1200),(172800,1,1200),
                (259200,1,1200),(345600,1,1199),(432000,1,1200),
                (518400,1,1200),(604800,1,1200),(691200,1,1200);
        ",
        )?;
        assert_hidden(&mut library, "standing-appointment", false)?;
        seed(
            &mut library.connection.borrow_mut(),
            "INSERT INTO stats_hours VALUES (349200,1,1);",
        )?;
        assert_hidden(&mut library, "standing-appointment", true)
    }

    fn forbidden(_: &mut AchievementDb<'_>) -> Result<Evaluation> {
        anyhow::bail!("an earned achievement must not be evaluated")
    }

    #[test]
    fn earned_rules_are_skipped_and_new_definitions_get_one_catch_up() -> Result<()> {
        let library = Library::memory()?;
        let connection = &mut *library.connection.borrow_mut();
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
    }
}
