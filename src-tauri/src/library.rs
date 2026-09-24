//! Saved queries + searchable run history, persisted as JSON under the app config dir.
//! History is recorded automatically when a query runs (capped, consecutive-dupes skipped).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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
    /// Set when the file exists but could not be read, so `save` must refuse rather than
    /// overwrite data we never saw. Never serialized — it describes this load, not the store.
    #[serde(skip)]
    unreadable: bool,
}

impl QueryLibrary {
    fn path() -> PathBuf {
        crate::appdir::app_dir().join("query_library.json")
    }

    pub fn load() -> Self {
        Self::load_at(&Self::path())
    }

    /// Load from an explicit path; a corrupt file is quarantined, not overwritten.
    ///
    /// The only read failure that may yield an empty library is "there is no file yet".
    /// Anything else (EACCES, a transient I/O fault, invalid UTF-8 — which is exactly what a
    /// half-written file looks like) means the user's saved queries are still on disk and we
    /// simply cannot see them; loading empty would let the next save overwrite the lot. Mark
    /// the library unreadable so `save` refuses instead.
    fn load_at(p: &Path) -> Self {
        let text = match fs::read_to_string(p) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(e) => {
                crate::applog::log(
                    "error",
                    &format!(
                        "{} could not be read ({e}); leaving it untouched — saved queries and \
                         history are unavailable until it can be read again",
                        p.display()
                    ),
                );
                return Self { unreadable: true, ..Self::default() };
            }
        };
        match serde_json::from_str(&text) {
            Ok(lib) => lib,
            Err(e) => {
                // Quarantined, so the path is now free and the next save is safe.
                crate::appdir::quarantine_corrupt(p, &e.to_string());
                Self::default()
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_at(&Self::path())
    }

    fn save_at(&self, p: &Path) -> Result<(), String> {
        if self.unreadable {
            return Err(format!(
                "{} could not be read, so it will not be overwritten. Fix its permissions or \
                 move it aside, then restart TableIQ.",
                p.display()
            ));
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        crate::appdir::write_atomic(p, json.as_bytes()).map_err(|e| e.to_string())
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

    /// Same contract as `connections.json`: a corrupt library is preserved, not overwritten.
    #[test]
    fn corrupt_library_is_quarantined_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("query_library.json");
        std::fs::write(&p, b"[not an object]").unwrap();

        let lib = QueryLibrary::load_at(&p);
        assert!(lib.saved.is_empty() && lib.history.is_empty());
        assert!(!p.exists());
        assert_eq!(
            std::fs::read_dir(dir.path())
                .unwrap()
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("query_library.json.corrupt-"))
                .count(),
            1
        );
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

    /// The read-failure path used to yield an empty library, and the next save overwrote
    /// every saved query and the whole run history with `{}`.
    #[test]
    fn an_unreadable_library_refuses_to_save_over_itself() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("query_library.json");
        // Not valid UTF-8, so `read_to_string` fails before serde_json ever sees it.
        let bytes: &[u8] = b"{\"saved\": [\xff\xfe";
        std::fs::write(&p, bytes).unwrap();

        let lib = QueryLibrary::load_at(&p);
        assert!(lib.saved.is_empty() && lib.history.is_empty());
        let err = lib.save_at(&p).expect_err("saving would have destroyed the file");
        assert!(err.contains("could not be read"), "unhelpful error: {err}");
        assert_eq!(std::fs::read(&p).unwrap(), bytes, "the user's file was modified");
    }

    #[test]
    fn a_missing_library_still_saves() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("query_library.json");
        assert_eq!(QueryLibrary::load_at(&p).save_at(&p), Ok(()));
        assert!(p.exists());
    }
}
