//! Saved queries + searchable run history, persisted as JSON under the app config dir.
//! History is recorded automatically when a query runs (capped, consecutive-dupes skipped).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const APP_DIR: &str = "ai.rhobots.tableiq";
const HISTORY_CAP: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedQuery {
    pub id: String,
    pub name: String,
    pub sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryEntry {
    pub sql: String,
    pub conn_id: String,
    /// Unix epoch seconds (the UI formats it).
    pub ts: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct QueryLibrary {
    #[serde(default)]
    pub saved: Vec<SavedQuery>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
}

impl QueryLibrary {
    fn path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join(APP_DIR).join("query_library.json")
    }

    pub fn load() -> Self {
        match fs::read_to_string(Self::path()) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        fs::write(&p, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }

    pub fn upsert_saved(&mut self, q: SavedQuery) {
        if let Some(slot) = self.saved.iter_mut().find(|s| s.id == q.id) {
            *slot = q;
        } else {
            self.saved.push(q);
        }
    }

    pub fn remove_saved(&mut self, id: &str) {
        self.saved.retain(|s| s.id != id);
    }

    /// Append a run to history; skips a consecutive duplicate (same sql + conn) and caps length.
    pub fn record(&mut self, entry: HistoryEntry) {
        if self
            .history
            .last()
            .map(|h| h.sql == entry.sql && h.conn_id == entry.conn_id)
            .unwrap_or(false)
        {
            return;
        }
        self.history.push(entry);
        let len = self.history.len();
        if len > HISTORY_CAP {
            self.history.drain(0..len - HISTORY_CAP);
        }
    }

    /// History matching `query` (case-insensitive substring), newest first. Empty query = all.
    pub fn search_history(&self, query: &str) -> Vec<HistoryEntry> {
        let q = query.to_lowercase();
        self.history
            .iter()
            .rev()
            .filter(|h| q.is_empty() || h.sql.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(sql: &str, conn: &str, ts: i64) -> HistoryEntry {
        HistoryEntry { sql: sql.into(), conn_id: conn.into(), ts }
    }

    #[test]
    fn upsert_saved_replaces_by_id() {
        let mut lib = QueryLibrary::default();
        lib.upsert_saved(SavedQuery { id: "1".into(), name: "A".into(), sql: "select 1".into() });
        lib.upsert_saved(SavedQuery { id: "1".into(), name: "B".into(), sql: "select 2".into() });
        assert_eq!(lib.saved.len(), 1);
        assert_eq!(lib.saved[0].name, "B");
    }

    #[test]
    fn remove_saved_drops_by_id() {
        let mut lib = QueryLibrary::default();
        lib.upsert_saved(SavedQuery { id: "1".into(), name: "A".into(), sql: "x".into() });
        lib.remove_saved("1");
        assert!(lib.saved.is_empty());
    }

    #[test]
    fn record_skips_consecutive_duplicate() {
        let mut lib = QueryLibrary::default();
        lib.record(entry("select 1", "c", 1));
        lib.record(entry("select 1", "c", 2)); // dup of the last → skipped
        lib.record(entry("select 2", "c", 3));
        assert_eq!(lib.history.len(), 2);
    }

    #[test]
    fn record_caps_history_length() {
        let mut lib = QueryLibrary::default();
        for i in 0..(HISTORY_CAP + 50) {
            lib.record(entry(&format!("select {i}"), "c", i as i64));
        }
        assert_eq!(lib.history.len(), HISTORY_CAP);
        // Oldest entries were dropped; newest survives.
        assert_eq!(lib.history.last().unwrap().sql, format!("select {}", HISTORY_CAP + 49));
    }

    #[test]
    fn search_history_is_case_insensitive_and_newest_first() {
        let mut lib = QueryLibrary::default();
        lib.record(entry("SELECT * FROM users", "c", 1));
        lib.record(entry("UPDATE orders SET x=1", "c", 2));
        lib.record(entry("select id from users", "c", 3));
        let hits = lib.search_history("users");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].ts, 3); // newest first
        assert_eq!(hits[1].ts, 1);
        assert_eq!(lib.search_history("").len(), 3);
    }
}
