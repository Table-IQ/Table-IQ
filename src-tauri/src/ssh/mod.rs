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
    /// Private-key auth. `path` is the key file on disk; `passphrase` (if any) comes from the
    /// keychain at connect time and is never serialized to disk / the bundle.
    ///
    /// Tag is `"keyfile"` (not snake_case `key_file`) to match the TS client
    /// (`src/lib/ssh.ts`) — otherwise key-auth SSH configs fail to deserialize at
    /// the IPC boundary.
    #[serde(rename = "keyfile")]
    KeyFile {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
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
                // A populated passphrase is never persisted from the stored config, but it must
                // still round-trip when present transiently.
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

    #[test]
    fn default_auth_is_agent() {
        assert_eq!(SshAuth::default(), SshAuth::Agent);
    }
}
