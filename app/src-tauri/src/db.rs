//! The user's private journal: streak start, relapses, urges and settings, in SQLite.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub reminders: bool,
    pub interval_min: u32,
    /// Local hours; no reminders from `quiet_start` until `quiet_end`.
    pub quiet_start: u32,
    pub quiet_end: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { reminders: true, interval_min: 120, quiet_start: 23, quiet_end: 7 }
    }
}

#[derive(Serialize)]
pub struct Event {
    pub ts: i64,
    pub note: String,
}

pub struct Db(Connection);

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS relapses (id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, note TEXT NOT NULL DEFAULT '');
             CREATE TABLE IF NOT EXISTS urges (id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, outcome TEXT NOT NULL DEFAULT '');",
        )?;
        Ok(Self(conn))
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self.0.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.0.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    /// When the user started their journey (their last relapse before installing).
    pub fn journey_start(&self) -> Result<Option<i64>> {
        Ok(self.meta("journey_start")?.and_then(|v| v.parse().ok()))
    }

    pub fn set_journey_start(&self, ts: i64) -> Result<()> {
        self.set_meta("journey_start", &ts.to_string())
    }

    /// Start of the current streak: the last relapse, or the journey start.
    pub fn streak_start(&self) -> Result<Option<i64>> {
        let last: Option<i64> = self.0.query_row("SELECT MAX(ts) FROM relapses", [], |r| r.get(0))?;
        Ok(match (self.journey_start()?, last) {
            (Some(j), Some(l)) => Some(j.max(l)),
            (j, l) => j.or(l),
        })
    }

    pub fn add_relapse(&self, ts: i64, note: &str) -> Result<()> {
        self.0.execute("INSERT INTO relapses (ts, note) VALUES (?1, ?2)", params![ts, note])?;
        Ok(())
    }

    pub fn add_urge(&self, ts: i64, outcome: &str) -> Result<()> {
        self.0.execute("INSERT INTO urges (ts, outcome) VALUES (?1, ?2)", params![ts, outcome])?;
        Ok(())
    }

    pub fn relapses(&self) -> Result<Vec<Event>> {
        self.events("SELECT ts, note FROM relapses ORDER BY ts")
    }

    pub fn urges(&self) -> Result<Vec<Event>> {
        self.events("SELECT ts, outcome FROM urges ORDER BY ts")
    }

    fn events(&self, sql: &str) -> Result<Vec<Event>> {
        let mut stmt = self.0.prepare(sql)?;
        let rows = stmt.query_map([], |r| Ok(Event { ts: r.get(0)?, note: r.get(1)? }))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn settings(&self) -> Result<Settings> {
        Ok(self
            .meta("settings")?
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default())
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.set_meta("settings", &serde_json::to_string(settings)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streak_start_follows_last_relapse() {
        let db = Db::open(Path::new(":memory:")).unwrap();
        assert_eq!(db.streak_start().unwrap(), None);
        db.set_journey_start(100).unwrap();
        assert_eq!(db.streak_start().unwrap(), Some(100));
        db.add_relapse(500, "").unwrap();
        db.add_relapse(300, "").unwrap();
        assert_eq!(db.streak_start().unwrap(), Some(500));
        assert_eq!(db.relapses().unwrap().len(), 2);
    }
}
