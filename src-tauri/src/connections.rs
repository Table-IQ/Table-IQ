//! Saved connections: metadata in a JSON file under the app config dir; passwords in
//! the OS keychain (never on disk). Postgres only for Slice 1.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "ai.rhobots.tableiq";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    #[serde(default = "default_sslmode")]
    pub sslmode: String,
}

fn default_sslmode() -> String {
    "prefer".into()
}

impl ConnConfig {
    /// libpq-style URL (no password — that comes from the keychain at connect time).
    pub fn url_with_password(&self, password: &str) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}",
            urlencode(&self.username),
            urlencode(password),
            self.host,
            self.port,
            urlencode(&self.database),
            self.sslmode
        )
    }
}

fn urlencode(s: &str) -> String {
    // Minimal percent-encoding for URL userinfo/path segments.
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ConnStore {
    pub connections: Vec<ConnConfig>,
}

impl ConnStore {
    fn path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join(KEYRING_SERVICE).join("connections.json")
    }

    pub fn load() -> Self {
        let p = Self::path();
        match fs::read_to_string(&p) {
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

    pub fn upsert(&mut self, cfg: ConnConfig) {
        if let Some(slot) = self.connections.iter_mut().find(|c| c.id == cfg.id) {
            *slot = cfg;
        } else {
            self.connections.push(cfg);
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.connections.retain(|c| c.id != id);
    }
}

// ── keychain ────────────────────────────────────────────────────────────
pub fn set_password(conn_id: &str, password: &str) -> Result<(), String> {
    keyring::Entry::new(KEYRING_SERVICE, conn_id)
        .and_then(|e| e.set_password(password))
        .map_err(|e| e.to_string())
}

pub fn get_password(conn_id: &str) -> Result<String, String> {
    keyring::Entry::new(KEYRING_SERVICE, conn_id)
        .and_then(|e| e.get_password())
        .map_err(|e| e.to_string())
}

pub fn delete_password(conn_id: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, conn_id).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrips_through_json() {
        let cfg = ConnConfig {
            id: "abc".into(),
            name: "Local PG".into(),
            color: Some("#4f46e5".into()),
            host: "127.0.0.1".into(),
            port: 5432,
            database: "app".into(),
            username: "postgres".into(),
            sslmode: "prefer".into(),
        };
        let store = ConnStore {
            connections: vec![cfg.clone()],
        };
        let json = serde_json::to_string(&store).unwrap();
        let back: ConnStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.connections[0], cfg);
    }

    #[test]
    fn url_encodes_special_chars_in_password() {
        let cfg = ConnConfig {
            id: "x".into(),
            name: "x".into(),
            color: None,
            host: "h".into(),
            port: 5432,
            database: "d".into(),
            username: "u".into(),
            sslmode: "require".into(),
        };
        let url = cfg.url_with_password("p@ss/word");
        assert!(url.contains("p%40ss%2Fword"));
        assert!(url.contains("sslmode=require"));
    }

    #[test]
    fn upsert_replaces_existing_by_id() {
        let mut store = ConnStore::default();
        let mut cfg = ConnConfig {
            id: "1".into(),
            name: "A".into(),
            color: None,
            host: "h".into(),
            port: 5432,
            database: "d".into(),
            username: "u".into(),
            sslmode: "prefer".into(),
        };
        store.upsert(cfg.clone());
        cfg.name = "B".into();
        store.upsert(cfg.clone());
        assert_eq!(store.connections.len(), 1);
        assert_eq!(store.connections[0].name, "B");
    }
}
