//! SSH tunneling for TableIQ (Slice 5).
//!
//! A connection may be tunneled through an SSH bastion via a real, in-process local
//! port-forward (russh, pure Rust). The DB driver connects to a local socket
//! (`127.0.0.1:<ephemeral>`); bytes are pumped over a `direct-tcpip` channel to the remote
//! `host:port`. Secrets (key passphrase / bastion password) live in the OS keychain under
//! `ssh:<conn_id>`, never on disk and never in the export bundle.

use serde::{Deserialize, Serialize};

pub mod ssh_config;
pub mod tunnel;

pub use tunnel::Tunnel;

/// How we authenticate to the SSH bastion.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SshAuth {
    /// Password auth; the password is read from the keychain (`ssh:<conn_id>`), never stored here.
    Password,
    /// Private-key auth. `path` is the key file on disk; `passphrase` (if any) arrives over
    /// IPC from the connection form and is never serialized to disk / the bundle — it is
    /// persisted only in the OS keychain under `ssh:<conn_id>`, and `maybe_open_tunnel`
    /// reads it back from there. Inbound-only by design, so `skip_serializing` (not
    /// `skip_serializing_if`, which would still have written a populated one).
    ///
    /// Tag is `"keyfile"` (not snake_case `key_file`) to match the TS client
    /// (`src/lib/ssh.ts`) — otherwise key-auth SSH configs fail to deserialize at
    /// the IPC boundary.
    #[serde(rename = "keyfile")]
    KeyFile {
        path: String,
        #[serde(default, skip_serializing)]
        passphrase: Option<String>,
    },
    /// Delegate to the running ssh-agent (no stored secret).
    #[default]
    Agent,
}

/// SSH bastion config embedded (optionally) on a `ConnConfig`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SshConfig {
    #[serde(default)]
    pub enabled: bool,
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub auth: SshAuth,
}

impl SshConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled {
            if self.host.trim().is_empty() || self.host.chars().any(|c| c.is_whitespace() || matches!(c, '/' | '\\')) {
                return Err("SSH server must be a hostname, IP address, or SSH alias".into());
            }
            if self.port == 0 { return Err("SSH port must be between 1 and 65535".into()); }
        }
        Ok(())
    }
}

fn default_ssh_port() -> u16 {
    22
}

/// Keychain account under which SSH secrets (password / key passphrase) are stored — mirrors
/// the DB-password pattern where the account is the connection id.
pub fn ssh_secret_account(conn_id: &str) -> String {
    format!("ssh:{conn_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_tunnels_validate_host_and_port_before_connecting() {
        let mut cfg = SshConfig { enabled: true, host: "bastion".into(), port: 22, username: "u".into(), auth: SshAuth::Agent };
        assert!(cfg.validate().is_ok());
        cfg.host = " ".into(); assert!(cfg.validate().is_err());
        cfg.host = "bastion".into(); cfg.port = 0; assert!(cfg.validate().is_err());
        cfg.enabled = false; assert!(cfg.validate().is_ok());
    }

    #[test]
    fn ssh_secret_account_is_namespaced_by_conn_id() {
        assert_eq!(ssh_secret_account("abc"), "ssh:abc");
        assert_eq!(ssh_secret_account("conn-123"), "ssh:conn-123");
    }

    #[test]
    fn ssh_config_serde_roundtrip_password() {
        let cfg = SshConfig {
            enabled: true,
            host: "bastion.example.com".into(),
            port: 22,
            username: "deploy".into(),
            auth: SshAuth::Password,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: SshConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn ssh_config_serde_roundtrip_keyfile() {
        let cfg = SshConfig {
            enabled: true,
            host: "10.0.0.1".into(),
            port: 2222,
            username: "ec2-user".into(),
            auth: SshAuth::KeyFile {
                path: "~/.ssh/id_ed25519".into(),
                // Never serialized (see the field's docs), so only `None` round-trips —
                // which is all the stored config ever holds.
                passphrase: None,
            },
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: SshConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn deserializes_frontend_keyfile_tag() {
        // The TS client (src/lib/ssh.ts) sends `mode: "keyfile"`. The Rust enum
        // must accept that exact tag, or saving a key-auth SSH connection fails to
        // deserialize at the IPC boundary.
        let json = r#"{"mode":"keyfile","path":"/Users/me/Downloads/key.pem"}"#;
        let auth: SshAuth = serde_json::from_str(json).expect("frontend keyfile tag must parse");
        match auth {
            SshAuth::KeyFile { path, .. } => assert_eq!(path, "/Users/me/Downloads/key.pem"),
            other => panic!("expected KeyFile, got {other:?}"),
        }
    }

    #[test]
    fn ssh_config_serde_roundtrip_agent() {
        let cfg = SshConfig {
            enabled: false,
            host: "h".into(),
            port: 22,
            username: "u".into(),
            auth: SshAuth::Agent,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: SshConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn keyfile_passphrase_is_omitted_from_json_when_none() {
        let cfg = SshConfig {
            enabled: true,
            host: "h".into(),
            port: 22,
            username: "u".into(),
            auth: SshAuth::KeyFile {
                path: "/k".into(),
                passphrase: None,
            },
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("passphrase"), "json was: {json}");
    }

    /// The "secret-free on disk" invariant, enforced against a POPULATED passphrase.
    /// `skip_serializing_if = "Option::is_none"` only ever omitted the field when it was
    /// already empty — a passphrase that arrived over IPC would have been written straight
    /// into connections.json and the export bundle.
    #[test]
    fn a_populated_keyfile_passphrase_is_never_serialized() {
        let cfg = SshConfig {
            enabled: true,
            host: "h".into(),
            port: 22,
            username: "u".into(),
            auth: SshAuth::KeyFile {
                path: "/k".into(),
                passphrase: Some("hunter2-the-real-secret".into()),
            },
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("passphrase"), "json leaked the field: {json}");
        assert!(!json.contains("hunter2"), "json leaked the secret: {json}");
    }

    #[test]
    fn default_auth_is_agent() {
        assert_eq!(SshAuth::default(), SshAuth::Agent);
    }
}
