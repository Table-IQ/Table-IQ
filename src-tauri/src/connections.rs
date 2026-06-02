//! Saved connections: metadata in a JSON file under the app config dir; passwords in
//! the OS keychain (never on disk). Postgres only for Slice 1.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ssh::SshConfig;
pub use crate::ssh::ssh_secret_account;

const KEYRING_SERVICE: &str = "ai.rhobots.tableiq";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default = "default_engine")]
    pub engine: String, // "postgres" | "mysql" | "sqlite"
    pub host: String,
    pub port: u16,
    pub database: String, // for sqlite: the file path
    pub username: String,
    #[serde(default = "default_sslmode")]
    pub sslmode: String,
    /// Optional SSH bastion. `#[serde(default)]` keeps legacy `connections.json` (no `ssh`
    /// field) deserializing as `None`. Carries no secret — the key passphrase / bastion
    /// password live in the keychain under `ssh:<conn_id>`.
    #[serde(default)]
    pub ssh: Option<SshConfig>,
}

fn default_sslmode() -> String {
    "prefer".into()
}
fn default_engine() -> String {
    "postgres".into()
}

impl ConnConfig {
    /// A clone with `host`/`port` replaced — used to retarget the driver URL at an SSH tunnel's
    /// local forward (`127.0.0.1:<ephemeral>`) instead of the remote bastion-side address.
    pub fn with_host_port(&self, host: &str, port: u16) -> ConnConfig {
        let mut c = self.clone();
        c.host = host.to_string();
        c.port = port;
        c
    }

    /// Engine-specific connection URL (no password for sqlite; password from keychain otherwise).
    pub fn url_with_password(&self, password: &str) -> String {
        match self.engine.as_str() {
            "sqlite" => self.database.clone(), // file path; SqliteDriver builds the sqlite:// url
            "redis" => {
                let auth = if password.is_empty() {
                    String::new()
                } else {
                    format!(":{}@", urlencode(password))
                };
                let dbn = if self.database.is_empty() { "0" } else { &self.database };
                format!("redis://{auth}{}:{}/{}", self.host, self.port, dbn)
            }
            "mongodb" => {
                let auth = if self.username.is_empty() {
                    String::new()
                } else {
                    format!("{}:{}@", urlencode(&self.username), urlencode(password))
                };
                format!("mongodb://{auth}{}:{}", self.host, self.port)
            }
            "sqlserver" => {
                // ADO connection string for tiberius. azure-sql-edge uses a self-signed cert.
                let dbn = if self.database.is_empty() { "master" } else { &self.database };
                format!(
                    "Server=tcp:{},{};User Id={};Password={};Database={};TrustServerCertificate=true;Encrypt=false",
                    self.host, self.port, self.username, password, dbn
                )
            }
            "mysql" => format!(
                "mysql://{}:{}@{}:{}/{}",
                urlencode(&self.username),
                urlencode(password),
                self.host,
                self.port,
                urlencode(&self.database),
            ),
            _ => {
                // psql / TablePlus behavior: an empty database connects to the
                // database named after the user. Sending an empty dbname instead
                // produces an opaque connect failure (later masked as a pool
                // timeout), so default it here.
                let dbn = if self.database.is_empty() {
                    &self.username
                } else {
                    &self.database
                };
                format!(
                    "postgres://{}:{}@{}:{}/{}?sslmode={}",
                    urlencode(&self.username),
                    urlencode(password),
                    self.host,
                    self.port,
                    urlencode(dbn),
                    self.sslmode
                )
            }
        }
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

    /// Export connections as a shareable JSON bundle. Secret-free by construction —
    /// passwords live in the keychain, never in `ConnConfig`, so a recipient re-enters them.
    pub fn export_bundle(&self) -> Result<String, String> {
        serde_json::to_string_pretty(&self.connections).map_err(|e| e.to_string())
    }

    /// Merge a bundle into the store (upsert by id). Returns how many were imported.
    pub fn import_bundle(&mut self, json: &str) -> Result<usize, String> {
        let incoming: Vec<ConnConfig> = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let n = incoming.len();
        for c in incoming {
            self.upsert(c);
        }
        Ok(n)
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
            engine: "postgres".into(),
            host: "127.0.0.1".into(),
            port: 5432,
            database: "app".into(),
            username: "postgres".into(),
            sslmode: "prefer".into(),
            ssh: None,
        };
        let store = ConnStore {
            connections: vec![cfg.clone()],
        };
        let json = serde_json::to_string(&store).unwrap();
        let back: ConnStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.connections[0], cfg);
    }

    #[test]
    fn pg_url_defaults_empty_database_to_username() {
        // psql / TablePlus behavior: an empty database connects to the database
        // named after the user. Empty `database` must NOT produce `…/?sslmode=…`
        // (which masks the real cause behind a connect error).
        let cfg = ConnConfig {
            id: "x".into(),
            name: "x".into(),
            color: None,
            engine: "postgres".into(),
            host: "h".into(),
            port: 5432,
            database: "".into(),
            username: "postgres".into(),
            sslmode: "prefer".into(),
            ssh: None,
        };
        let url = cfg.url_with_password("pw");
        assert!(url.contains("@h:5432/postgres?"), "expected db=username, got {url}");
    }

    #[test]
    fn url_encodes_special_chars_in_password() {
        let cfg = ConnConfig {
            id: "x".into(),
            name: "x".into(),
            color: None,
            engine: "postgres".into(),
            host: "h".into(),
            port: 5432,
            database: "d".into(),
            username: "u".into(),
            sslmode: "require".into(),
            ssh: None,
        };
        let url = cfg.url_with_password("p@ss/word");
        assert!(url.contains("p%40ss%2Fword"));
        assert!(url.contains("sslmode=require"));
    }

    #[test]
    fn bundle_export_import_roundtrip_is_secret_free() {
        let mut store = ConnStore::default();
        store.upsert(ConnConfig {
            id: "1".into(),
            name: "PG".into(),
            color: None,
            engine: "postgres".into(),
            host: "h".into(),
            port: 5432,
            database: "d".into(),
            username: "u".into(),
            sslmode: "prefer".into(),
            ssh: None,
        });
        let bundle = store.export_bundle().unwrap();
        // No password field exists on ConnConfig, so a bundle can never carry secrets.
        assert!(!bundle.to_lowercase().contains("password"));

        let mut other = ConnStore::default();
        assert_eq!(other.import_bundle(&bundle).unwrap(), 1);
        assert_eq!(other.connections[0].name, "PG");
    }

    #[test]
    fn legacy_json_without_ssh_field_deserializes_as_none() {
        // Saved files predating Slice 5 have no `ssh` key — must still load.
        let json = r#"{
            "id": "old",
            "name": "Legacy",
            "engine": "postgres",
            "host": "h",
            "port": 5432,
            "database": "d",
            "username": "u"
        }"#;
        let cfg: ConnConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.ssh.is_none());
        assert_eq!(cfg.sslmode, "prefer"); // existing defaults still apply
    }

    #[test]
    fn with_host_port_retargets_url_to_local_forward_postgres() {
        let cfg = ConnConfig {
            id: "x".into(),
            name: "x".into(),
            color: None,
            engine: "postgres".into(),
            host: "db.internal".into(),
            port: 5432,
            database: "app".into(),
            username: "u".into(),
            sslmode: "prefer".into(),
            ssh: Some(crate::ssh::SshConfig {
                enabled: true,
                host: "bastion".into(),
                port: 22,
                username: "deploy".into(),
                auth: crate::ssh::SshAuth::Agent,
            }),
        };
        // Through a tunnel the driver must target 127.0.0.1:<local>, never the remote host.
        let url = cfg.with_host_port("127.0.0.1", 54999).url_with_password("pw");
        assert!(url.contains("127.0.0.1:54999"), "url was: {url}");
        assert!(!url.contains("db.internal"), "url leaked remote host: {url}");
    }

    #[test]
    fn with_host_port_retargets_url_to_local_forward_mysql() {
        let cfg = ConnConfig {
            id: "x".into(),
            name: "x".into(),
            color: None,
            engine: "mysql".into(),
            host: "db.internal".into(),
            port: 3306,
            database: "app".into(),
            username: "u".into(),
            sslmode: "prefer".into(),
            ssh: None,
        };
        let url = cfg.with_host_port("127.0.0.1", 60001).url_with_password("pw");
        assert!(url.starts_with("mysql://"));
        assert!(url.contains("127.0.0.1:60001"), "url was: {url}");
        assert!(!url.contains("db.internal"));
    }

    #[test]
    fn bundle_stays_secret_free_with_ssh_config_present() {
        let mut store = ConnStore::default();
        store.upsert(ConnConfig {
            id: "1".into(),
            name: "PG via SSH".into(),
            color: None,
            engine: "postgres".into(),
            host: "db.internal".into(),
            port: 5432,
            database: "app".into(),
            username: "u".into(),
            sslmode: "prefer".into(),
            ssh: Some(crate::ssh::SshConfig {
                enabled: true,
                host: "bastion".into(),
                port: 22,
                username: "deploy".into(),
                // KeyFile path is fine to share; passphrase is None and never serialized.
                auth: crate::ssh::SshAuth::KeyFile {
                    path: "~/.ssh/id_ed25519".into(),
                    passphrase: None,
                },
            }),
        });
        let bundle = store.export_bundle().unwrap().to_lowercase();
        assert!(!bundle.contains("passphrase"), "bundle leaked passphrase field: {bundle}");
        assert!(!bundle.contains("password"), "bundle leaked password: {bundle}");
        // Re-import works and preserves the (secret-free) ssh config.
        let mut other = ConnStore::default();
        assert_eq!(other.import_bundle(&store.export_bundle().unwrap()).unwrap(), 1);
        assert!(other.connections[0].ssh.as_ref().unwrap().enabled);
    }

    #[test]
    fn ssh_secret_account_derivation() {
        assert_eq!(ssh_secret_account("conn-9"), "ssh:conn-9");
    }

    #[test]
    fn upsert_replaces_existing_by_id() {
        let mut store = ConnStore::default();
        let mut cfg = ConnConfig {
            id: "1".into(),
            name: "A".into(),
            color: None,
            engine: "postgres".into(),
            host: "h".into(),
            port: 5432,
            database: "d".into(),
            username: "u".into(),
            sslmode: "prefer".into(),
            ssh: None,
        };
        store.upsert(cfg.clone());
        cfg.name = "B".into();
        store.upsert(cfg.clone());
        assert_eq!(store.connections.len(), 1);
        assert_eq!(store.connections[0].name, "B");
    }
}
