//! Saved connections: metadata in a JSON file under the app config dir; passwords in
//! the OS keychain (never on disk). Postgres only for Slice 1.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ssh::SshConfig;
pub use crate::ssh::ssh_secret_account;

pub(crate) const KEYRING_SERVICE: &str = "ai.rhobots.tableiq";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default = "default_engine")]
    pub engine: String, // "postgres" | "mysql" | "sqlite" | "duckdb" | "sqlserver" | …
    pub host: String,
    pub port: u16,
    pub database: String, // for sqlite/duckdb: the file path (duckdb also `:memory:`)
    pub username: String,
    #[serde(default = "default_sslmode")]
    pub sslmode: String,
    /// Optional SSH bastion. `#[serde(default)]` keeps legacy `connections.json` (no `ssh`
    /// field) deserializing as `None`. Carries no secret — the key passphrase / bastion
    /// password live in the keychain under `ssh:<conn_id>`.
    #[serde(default)]
    pub ssh: Option<SshConfig>,
    /// TablePlus-style environment tag ("local" | "development" | "testing" | "staging" |
    /// "production"); tints the titlebar pill. `serde(default)` keeps old configs loading.
    #[serde(default)]
    pub env: Option<String>,
    /// The product behind a MySQL- or Postgres-compatible server ("tidb", "cockroachdb" ...),
    /// a key of `profiles.json`. `engine` stays the wire protocol; `None` is plain MySQL/Postgres.
    #[serde(default)]
    pub profile: Option<String>,
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

    /// Engines whose `database` is a local file path: no host, no password, no SSH tunnel.
    pub fn is_file_engine(&self) -> bool {
        matches!(self.engine.as_str(), "sqlite" | "duckdb")
    }

    /// Engine-specific connection URL (no password for file engines; password from keychain otherwise).
    pub fn url_with_password(&self, password: &str) -> String {
        match self.engine.as_str() {
            // A file path (or DuckDB's `:memory:`); the driver opens it directly.
            "sqlite" | "duckdb" => self.database.clone(),
            "redis" => {
                let auth = if password.is_empty() {
                    String::new()
                } else {
                    format!(":{}@", urlencode(password))
                };
                let dbn = if self.database.is_empty() { "0" } else { &self.database };
                format!("redis://{auth}{}:{}/{}", urlencode_host(&self.host), self.port, dbn)
            }
            // A pasted connection string kept whole (SRV, authSource, replicaSet, TLS …): `host`.
            "mongodb" if self.host.starts_with("mongodb://") || self.host.starts_with("mongodb+srv://") => {
                mongo_url_with_password(&self.host, &self.username, password)
            }
            "mongodb" => {
                let auth = if self.username.is_empty() {
                    String::new()
                } else {
                    format!("{}:{}@", urlencode(&self.username), urlencode(password))
                };
                format!("mongodb://{auth}{}:{}", urlencode_host(&self.host), self.port)
            }
            "mysql" => format!(
                "mysql://{}:{}@{}:{}/{}?ssl-mode={}",
                urlencode(&self.username),
                urlencode(password),
                urlencode_host(&self.host),
                self.port,
                urlencode(&self.database),
                mysql_ssl_mode(&self.sslmode),
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
                    urlencode_host(&self.host),
                    self.port,
                    urlencode(dbn),
                    urlencode(&self.sslmode)
                )
            }
        }
    }
}

/// A MongoDB connection string with the keychain password filled in — only where the string has
/// none of its own: `user@h` becomes `user:pw@h`, `h` becomes `username:pw@h` when a username
/// is saved. Everything else (options, several hosts, `+srv`) passes through untouched.
fn mongo_url_with_password(url: &str, username: &str, password: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("mongodb", url));
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let (user, hosts) = match authority.rsplit_once('@') {
        Some((u, _)) if u.contains(':') => return url.to_string(), // carries its own password
        Some((u, h)) => (u.to_string(), h),
        None => (urlencode(username), authority),
    };
    if password.is_empty() || user.is_empty() {
        return url.to_string();
    }
    format!("{scheme}://{user}:{}@{hosts}{tail}", urlencode(password))
}

/// The sqlx `ssl-mode` for a saved SSL mode (libpq or MySQL spelling). Unknown fails closed.
pub(crate) fn mysql_ssl_mode(saved: &str) -> &'static str {
    match saved.trim().to_ascii_lowercase().as_str() {
        "disable" | "disabled" => "DISABLED",
        "prefer" | "preferred" | "allow" => "PREFERRED",
        "verify-ca" | "verify_ca" => "VERIFY_CA",
        "verify-full" | "verify_full" | "verify_identity" | "verify-identity" => "VERIFY_IDENTITY",
        _ => "REQUIRED",
    }
}

/// Percent-encode a host for the URL authority. Everything `urlencode` keeps, plus `:` `[` `]`
/// so an IPv6 literal (`[::1]`) — which is already URL-legal — survives intact. What matters
/// is that `@`, `/`, `?`, `#` and `&` do *not*: a stored host of `h@evil.example.com` used to
/// push the real host into the userinfo and hand the password to whatever followed the `@`.
fn urlencode_host(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' | b'['
            | b']' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
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
    /// Set when the file exists but could not be read, so `save` must refuse rather than
    /// overwrite data we never saw. Never serialized — it describes this load, not the store.
    #[serde(skip)]
    unreadable: bool,
}

impl ConnStore {
    fn path() -> PathBuf {
        crate::appdir::app_dir().join("connections.json")
    }

    pub fn load() -> Self {
        Self::load_at(&Self::path())
    }

    /// Load from an explicit path. A file that exists but does not parse is moved aside
    /// rather than silently replaced by the next save — see `appdir::quarantine`.
    ///
    /// The only read failure that may yield an empty store is "there is no file yet".
    /// Anything else (EACCES, a transient I/O fault, invalid UTF-8 — which is exactly what a
    /// half-written file looks like) means the user's connections are still on disk and we
    /// simply cannot see them; loading empty would let the next save atomically overwrite
    /// the lot with `{"connections":[]}`, orphaning every keychain entry behind it. Mark the
    /// store unreadable so `save` refuses instead.
    fn load_at(p: &Path) -> Self {
        let text = match fs::read_to_string(p) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(e) => {
                crate::applog::log(
                    "error",
                    &format!(
                        "{} could not be read ({e}); leaving it untouched — saved connections \
                         are unavailable until it can be read again",
                        p.display()
                    ),
                );
                return Self { connections: Vec::new(), unreadable: true };
            }
        };
        match serde_json::from_str(&text) {
            Ok(store) => store,
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

    /// Merge a bundle into the store. Returns how many were imported.
    ///
    /// An imported connection **never** takes over an existing id. Bundles are meant to be
    /// shared (that is the whole point of their being secret-free), while the keychain keys
    /// the password to the connection id — so an upsert-by-id let a bundle carrying the
    /// victim's own `"id"` silently retarget their saved "Prod DB" at another host, and the
    /// next connect handed the real production password over in cleartext. A colliding id
    /// gets a fresh one and lands as its own visible, password-less entry instead.
    pub fn import_bundle(&mut self, json: &str) -> Result<usize, String> {
        let incoming: Vec<ConnConfig> = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let n = incoming.len();
        for mut c in incoming {
            if c.id.is_empty() || self.connections.iter().any(|e| e.id == c.id) {
                c.id = uuid::Uuid::new_v4().to_string();
            }
            self.connections.push(c);
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
pub(crate) mod tests {
    use super::*;

    pub(crate) fn cfg(engine: &str) -> ConnConfig {
        ConnConfig {
            env: None, profile: None, id: "x".into(), name: "x".into(), color: None,
            engine: engine.into(), host: "h".into(), port: 3306, database: "d".into(),
            username: "u".into(), sslmode: "prefer".into(), ssh: None,
        }
    }

    /// A pasted Mongo URL (kept whole in `host`) gets the keychain password only where it has none.
    #[test]
    fn mongo_url_host_takes_the_keychain_password_only_when_absent() {
        let url = |host: &str, user: &str, pw: &str| {
            ConnConfig { engine: "mongodb".into(), host: host.into(), username: user.into(), ..cfg("mongodb") }.url_with_password(pw)
        };
        let srv = "mongodb+srv://u@c.example.net/app?authSource=admin&w=majority";
        assert_eq!(url(srv, "u", "p@ss/1"), "mongodb+srv://u:p%40ss%2F1@c.example.net/app?authSource=admin&w=majority");
        assert_eq!(url("mongodb://a:1,b:2/?replicaSet=rs0", "u", "pw"), "mongodb://u:pw@a:1,b:2/?replicaSet=rs0");
        assert_eq!(url("mongodb://u:own@h/app", "u", "other"), "mongodb://u:own@h/app", "its own password wins");
        assert_eq!(url(srv, "u", ""), srv, "no saved password: untouched");
        assert_eq!(url("mongodb://h?tls=true", "", "pw"), "mongodb://h?tls=true", "no user, no credentials");
        // Without a scheme in `host` the field-built URL is unchanged.
        assert_eq!(url("h", "u", "pw"), "mongodb://u:pw@h:3306");
    }

    #[test]
    fn legacy_json_without_profile_deserializes_as_none() {
        let json = r#"{"id":"o","name":"L","engine":"mysql","host":"h","port":3306,"database":"d","username":"u"}"#;
        let c: ConnConfig = serde_json::from_str(json).unwrap();
        assert!(c.profile.is_none());
    }

    /// The saved SSL mode never reached the MySQL driver. An unknown value fails closed.
    #[test]
    fn mysql_url_carries_the_ssl_mode() {
        let mut c = cfg("mysql");
        for (saved, sent) in [
            ("disable", "DISABLED"), ("DISABLED", "DISABLED"),
            ("prefer", "PREFERRED"), ("allow", "PREFERRED"), ("Preferred", "PREFERRED"),
            ("require", "REQUIRED"), ("REQUIRED", "REQUIRED"),
            ("verify-ca", "VERIFY_CA"), ("VERIFY_CA", "VERIFY_CA"),
            ("verify-full", "VERIFY_IDENTITY"), ("verify_identity", "VERIFY_IDENTITY"),
            ("nonsense&x=1", "REQUIRED"),
        ] {
            c.sslmode = saved.into();
            assert!(c.url_with_password("pw").ends_with(&format!("/d?ssl-mode={sent}")), "{saved}");
        }
    }

    #[test]
    fn config_roundtrips_through_json() {
        let cfg = ConnConfig {
            env: None,
            profile: None,
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
            ..Default::default()
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
            env: None,
            profile: None,
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
    fn file_engines_use_the_database_field_as_a_path() {
        let mut cfg = ConnConfig {
            env: None,
            profile: None,
            id: "x".into(),
            name: "x".into(),
            color: None,
            engine: "duckdb".into(),
            host: "h".into(),
            port: 0,
            database: "/tmp/a b?.duckdb".into(),
            username: "".into(),
            sslmode: "prefer".into(),
            ssh: None,
        };
        assert!(cfg.is_file_engine());
        assert_eq!(cfg.url_with_password("pw"), "/tmp/a b?.duckdb");
        cfg.database = ":memory:".into();
        assert_eq!(cfg.url_with_password(""), ":memory:");
        cfg.engine = "sqlite".into();
        assert!(cfg.is_file_engine());
        for e in ["postgres", "mysql", "sqlserver", "redis", "mongodb"] {
            cfg.engine = e.into();
            assert!(!cfg.is_file_engine(), "{e} is not a file engine");
        }
    }

    #[test]
    fn url_encodes_special_chars_in_password() {
        let cfg = ConnConfig {
            env: None,
            profile: None,
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
            env: None,
            profile: None,
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
            env: None,
            profile: None,
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
            env: None,
            profile: None,
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
            env: None,
            profile: None,
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

    /// A `connections.json` that exists but does not parse must be moved aside, never
    /// silently replaced by the next save — it is the user's entire set of connections.
    #[test]
    fn corrupt_store_is_quarantined_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("connections.json");
        std::fs::write(&p, b"{ truncated").unwrap();

        let store = ConnStore::load_at(&p);
        assert!(store.connections.is_empty(), "a corrupt file loads as empty");
        assert!(!p.exists(), "the corrupt file must be moved out of the way");
        let aside: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("connections.json.corrupt-"))
            .collect();
        assert_eq!(aside.len(), 1, "expected exactly one quarantined file, saw {aside:?}");
        assert_eq!(std::fs::read_to_string(dir.path().join(&aside[0])).unwrap(), "{ truncated");
    }

    #[test]
    fn missing_store_loads_empty_without_quarantining() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("connections.json");
        assert!(ConnStore::load_at(&p).connections.is_empty());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    /// A bundle is the feature's secret-free selling point, so people share them. Upserting
    /// **by id** meant a bundle carrying `{"id":"<victim's Prod DB id>","host":"evil…"}`
    /// silently retargeted the victim's existing entry — while the keychain password stayed
    /// keyed to that same id, so the next connect sent the real production password, in
    /// cleartext (`sslmode: disable`), to the attacker's host.
    #[test]
    fn import_never_takes_over_an_existing_connection() {
        let mut store = ConnStore::default();
        store.upsert(ConnConfig {
            env: None,
            profile: None,
            id: "prod-1".into(),
            name: "Prod DB".into(),
            color: None,
            engine: "postgres".into(),
            host: "db.internal".into(),
            port: 5432,
            database: "app".into(),
            username: "svc".into(),
            sslmode: "verify-full".into(),
            ssh: None,
        });
        let hostile = r#"[{"id":"prod-1","name":"Prod DB","engine":"postgres",
            "host":"evil.example.com","port":5432,"database":"app","username":"svc",
            "sslmode":"disable"}]"#;
        assert_eq!(store.import_bundle(hostile).unwrap(), 1);

        let victim = store.connections.iter().find(|c| c.id == "prod-1").expect("victim kept");
        assert_eq!(victim.host, "db.internal", "the saved connection was retargeted");
        assert_eq!(victim.sslmode, "verify-full", "TLS was downgraded on the saved connection");
        assert_eq!(store.connections.len(), 2, "the import must land as its own entry");
        let imported = store.connections.iter().find(|c| c.host == "evil.example.com").unwrap();
        assert_ne!(imported.id, "prod-1", "the import reused the victim's keychain id");
    }

    /// A non-colliding id is kept, so re-importing your own bundle onto a fresh machine
    /// still round-trips one entry rather than duplicating it.
    #[test]
    fn import_keeps_ids_that_do_not_collide() {
        let mut store = ConnStore::default();
        let bundle = r#"[{"id":"fresh","name":"PG","engine":"postgres","host":"h","port":5432,
            "database":"d","username":"u","sslmode":"prefer"}]"#;
        store.import_bundle(bundle).unwrap();
        assert_eq!(store.connections[0].id, "fresh");
    }

    /// `host` and `sslmode` were the only fields interpolated into the URL unencoded, so a
    /// stored `sslmode` of `disable&host=attacker` appended extra sqlx connect options and a
    /// `host` containing `@` moved the real host into the userinfo — taking the password with it.
    #[test]
    fn url_encodes_host_and_sslmode() {
        let cfg = ConnConfig {
            env: None,
            profile: None,
            id: "x".into(),
            name: "x".into(),
            color: None,
            engine: "postgres".into(),
            host: "h@evil.example.com".into(),
            port: 5432,
            database: "d".into(),
            username: "u".into(),
            sslmode: "disable&host=attacker".into(),
            ssh: None,
        };
        let url = cfg.url_with_password("pw");
        assert!(url.contains("@h%40evil.example.com:5432/"), "host not encoded: {url}");
        assert!(url.ends_with("sslmode=disable%26host%3Dattacker"), "sslmode not encoded: {url}");
    }

    /// Encoding the host must not break an IPv6 literal, which is already URL-legal.
    #[test]
    fn url_keeps_an_ipv6_literal_host_intact() {
        let cfg = ConnConfig {
            env: None,
            profile: None,
            id: "x".into(),
            name: "x".into(),
            color: None,
            engine: "mysql".into(),
            host: "[::1]".into(),
            port: 3306,
            database: "d".into(),
            username: "u".into(),
            sslmode: "prefer".into(),
            ssh: None,
        };
        assert!(cfg.url_with_password("pw").contains("@[::1]:3306/"), "IPv6 host was mangled");
    }

    /// The *parse*-failure path quarantines carefully. The *read*-failure path did not:
    /// EACCES, a transient I/O error, or invalid UTF-8 — exactly what a half-written file
    /// looks like — silently yielded an empty store, and the next save atomically overwrote
    /// the user's entire connection set with `{"connections":[]}`, orphaning every keychain
    /// entry behind it.
    #[test]
    fn an_unreadable_store_refuses_to_save_over_itself() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("connections.json");
        // Not valid UTF-8, so `read_to_string` fails before serde_json ever sees it.
        let bytes: &[u8] = b"{\"connections\": [\xff\xfe";
        std::fs::write(&p, bytes).unwrap();

        let store = ConnStore::load_at(&p);
        assert!(store.connections.is_empty(), "nothing could be read");
        let err = store.save_at(&p).expect_err("saving would have destroyed the file");
        assert!(err.contains("could not be read"), "unhelpful error: {err}");
        assert_eq!(std::fs::read(&p).unwrap(), bytes, "the user's file was modified");
    }

    /// The one read failure that *is* safe to treat as an empty store: there is no file yet.
    #[test]
    fn a_missing_store_still_saves() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("connections.json");
        assert_eq!(ConnStore::load_at(&p).save_at(&p), Ok(()));
        assert!(p.exists());
    }

    /// A quarantined (unparseable) file is out of the way, so saving over the now-missing
    /// path is safe and must keep working.
    #[test]
    fn a_quarantined_store_still_saves() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("connections.json");
        std::fs::write(&p, b"{ truncated").unwrap();
        assert_eq!(ConnStore::load_at(&p).save_at(&p), Ok(()));
    }

    #[test]
    fn upsert_replaces_existing_by_id() {
        let mut store = ConnStore::default();
        let mut cfg = ConnConfig {
            env: None,
            profile: None,
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
