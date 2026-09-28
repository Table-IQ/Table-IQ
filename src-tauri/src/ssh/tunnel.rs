//! A live SSH local port-forward (russh).
//!
//! `Tunnel::connect` authenticates to a bastion, binds `127.0.0.1:0`, and for every inbound
//! TCP connection opens a `direct-tcpip` channel to the remote `host:port` and pumps bytes
//! both ways. `local_addr()` is what the DB driver connects to. Dropping the tunnel (or
//! `close()`) stops the listener and disconnects the SSH session.
//!
//! Host-key handling: trust on first use, pinned thereafter. `~/.ssh/known_hosts` is
//! consulted first (the user's own trust decisions), then TableIQ's own `known_hosts` under
//! `app_dir()`, which is also where an unknown host is learned — we never write to the
//! user's file. A host that is recorded with a DIFFERENT key is refused outright: that is
//! what an interception looks like, and continuing would hand the bastion password and all
//! database traffic to whoever is in the middle.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use russh::client::{self, Handle, Handler};
use russh::keys::key;
use tokio::io::copy_bidirectional;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use super::{SshAuth, SshConfig};
use crate::lk;

/// The remote endpoint (the DB) reachable from the bastion.
#[derive(Debug, Clone)]
pub struct RemoteTarget {
    pub host: String,
    pub port: u16,
}

/// TableIQ's own `known_hosts`. Learned keys go here, never into the user's `~/.ssh`.
fn app_known_hosts() -> PathBuf {
    crate::appdir::app_dir().join("known_hosts")
}

/// The files checked, in order: the user's own trust first, then TableIQ's.
fn known_hosts_files() -> Vec<PathBuf> {
    let mut files = Vec::with_capacity(2);
    if let Some(home) = dirs::home_dir() {
        files.push(home.join(".ssh").join("known_hosts"));
    }
    files.push(app_known_hosts());
    files
}

/// What to do with the host key a bastion just offered.
#[derive(Debug, PartialEq)]
enum Verdict {
    /// Recorded in one of the known_hosts files and it matches.
    Known,
    /// Not recorded anywhere — trust on first use.
    Learn,
    /// Recorded with a different key, or the recorded entry is unreadable. Never connect;
    /// the payload is the message the user sees.
    Changed(String),
}

/// Decide `key`'s fate against `files`, in order. The first file that has an opinion wins,
/// so the user's `~/.ssh/known_hosts` can never be overridden by TableIQ's own store.
fn verdict(host: &str, port: u16, key: &key::PublicKey, files: &[PathBuf]) -> Verdict {
    for file in files {
        // russh cannot tell "no such file" from "cannot open it": both end up as an empty
        // list of recorded keys, i.e. `Ok(false)`. Falling through on the second would TOFU
        // a key even though this file may pin the host, so open it ourselves first and only
        // skip a genuinely absent one.
        match std::fs::File::open(file) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Verdict::Changed(format!(
                    "Refusing to connect: {} exists but cannot be read ({e}). It may record \
                     the host key for {host}:{port} — fix its permissions, or remove it if \
                     you no longer want it consulted.",
                    file.display()
                ))
            }
        }
        // Compare key MATERIAL, never the algorithm name. `check_known_hosts_path` reports
        // `KeyChanged` only when the offered key's name matches a recorded entry's
        // (russh-keys 0.45 `lib.rs:488`: `(false, _) => Ok(false)`), so an attacker who
        // offered a different algorithm for a pinned host read as "not recorded here" and
        // was trusted on first use. Reading the entries ourselves also fixes the mirror
        // bug: a recorded RSA entry always parses back as `rsa-sha2-256`, so an RSA bastion
        // that negotiated another hash never matched its own pin and was re-learned on
        // every connect. `PublicKey`'s `PartialEq` compares the RSA modulus and ignores the
        // hash tag, which is exactly the identity we want.
        let recorded = match russh::keys::known_host_keys_path(host, port, file) {
            Ok(entries) => entries,
            // The host may well be recorded here but we cannot read the entry (an
            // unsupported key type). Learning past it would silently bypass the pinning.
            Err(e) => {
                return Verdict::Changed(format!(
                    "Refusing to connect: TableIQ cannot read the recorded SSH host key for \
                     {host}:{port} in {}: {e}",
                    file.display()
                ))
            }
        };
        if recorded.is_empty() {
            continue; // not recorded in this file — ask the next one
        }
        if recorded.iter().any(|(_, known)| known == key) {
            return Verdict::Known;
        }
        // The host IS pinned here and none of its keys match what was offered.
        return Verdict::Changed(changed_message(host, port, key, file, &recorded));
    }
    Verdict::Learn
}

/// The message for a changed key: the host, both fingerprints, and the file to edit.
fn changed_message(
    host: &str,
    port: u16,
    offered: &key::PublicKey,
    file: &std::path::Path,
    entries: &[(usize, key::PublicKey)],
) -> String {
    let (line, recorded) = entries
        .first()
        .map(|(l, k)| (*l, k.fingerprint()))
        .unwrap_or((0, "unavailable".to_string()));
    format!(
        "SSH host key for {host}:{port} has CHANGED — refusing to connect. Someone may be \
         intercepting this connection.\n  expected SHA256:{recorded}\n  received SHA256:{}\n\
         If this change is expected, remove line {line} for {host} from {} and reconnect.",
        offered.fingerprint(),
        file.display()
    )
}

/// russh client handler. Verifies the server host key against known_hosts (see module docs).
struct TunnelHandler {
    host: String,
    port: u16,
    /// Set when we refuse a key. `Tunnel::connect` swaps it in for russh's opaque
    /// "unknown key" error so the user sees what actually happened.
    rejection: Arc<Mutex<Option<String>>>,
}

#[async_trait]
impl Handler for TunnelHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &key::PublicKey,
    ) -> Result<bool, Self::Error> {
        match verdict(&self.host, self.port, server_public_key, &known_hosts_files()) {
            Verdict::Known => Ok(true),
            Verdict::Learn => {
                let store = app_known_hosts();
                if let Err(e) = russh::keys::learn_known_hosts_path(
                    &self.host,
                    self.port,
                    server_public_key,
                    &store,
                ) {
                    *lk(&self.rejection) =
                        Some(format!("cannot record the SSH host key in {}: {e}", store.display()));
                    return Ok(false);
                }
                crate::applog::log(
                    "warn",
                    &format!(
                        "trusting NEW ssh host key for {}:{} (SHA256:{}) — recorded in {}",
                        self.host,
                        self.port,
                        server_public_key.fingerprint(),
                        store.display()
                    ),
                );
                Ok(true)
            }
            Verdict::Changed(msg) => {
                *lk(&self.rejection) = Some(msg);
                Ok(false)
            }
        }
    }
}

/// A live, owned SSH tunnel. Drop closes it.
pub struct Tunnel {
    local_addr: SocketAddr,
    accept_task: JoinHandle<()>,
    session: Arc<Handle<TunnelHandler>>,
}

impl Tunnel {
    /// Open a tunnel to `cfg.host:cfg.port` (the bastion), forwarding to `target`.
    ///
    /// `secret` carries the bastion password (Password auth) or the key passphrase (KeyFile
    /// auth, may be empty); it is unused for Agent auth.
    pub async fn connect(
        cfg: &SshConfig,
        secret: &str,
        target: RemoteTarget,
    ) -> Result<Self, String> {
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(3600)),
            ..Default::default()
        });

        // A refused host key surfaces here as a generic russh error; `rejection` carries the
        // real reason out of the handler so the user sees it instead.
        let rejection: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let handler = TunnelHandler {
            host: cfg.host.clone(),
            port: cfg.port,
            rejection: Arc::clone(&rejection),
        };
        let mut handle = client::connect(config, (cfg.host.as_str(), cfg.port), handler)
            .await
            .map_err(|e| match lk(&rejection).take() {
                Some(msg) => msg,
                None => format!("ssh connect to bastion failed: {e}"),
            })?;

        authenticate(&mut handle, cfg, secret).await?;

        let session = Arc::new(handle);

        // Bind an ephemeral local port; the kernel chooses it.
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("bind local forward failed: {e}"))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| format!("local_addr failed: {e}"))?;

        let accept_task = spawn_accept_loop(listener, session.clone(), target, local_addr);

        Ok(Self {
            local_addr,
            accept_task,
            session,
        })
    }

    /// The `127.0.0.1:<ephemeral>` address the DB driver should connect to.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stop the listener and disconnect the SSH session.
    pub async fn close(&self) {
        self.accept_task.abort();
        let _ = self
            .session
            .disconnect(russh::Disconnect::ByApplication, "tableiq closing tunnel", "")
            .await;
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        // Stop accepting new forwards immediately. The SSH session's background task is dropped
        // with the last Arc; existing pumps end when their sockets close.
        self.accept_task.abort();
    }
}

/// Authenticate to the bastion per the configured auth mode.
async fn authenticate(
    handle: &mut Handle<TunnelHandler>,
    cfg: &SshConfig,
    secret: &str,
) -> Result<(), String> {
    let ok = match &cfg.auth {
        // An empty password falls back to the ssh-agent (TablePlus's "leave empty
        // to use ~/.ssh/config" behavior) instead of attempting empty-password auth.
        SshAuth::Password if secret.is_empty() => {
            authenticate_agent(handle, &cfg.username).await?
        }
        SshAuth::Password => handle
            .authenticate_password(cfg.username.clone(), secret.to_string())
            .await
            .map_err(|e| format!("ssh password auth error: {e}"))?,
        SshAuth::KeyFile { path, passphrase } => {
            // Prefer an explicit passphrase from the config; otherwise the keychain secret.
            let pass = passphrase
                .as_deref()
                .filter(|s| !s.is_empty())
                .or(if secret.is_empty() { None } else { Some(secret) });
            let expanded = super::ssh_config::expand_tilde(path);
            let key = russh::keys::load_secret_key(&expanded, pass)
                .map_err(|e| format!("load key {expanded} failed: {e}"))?;
            handle
                .authenticate_publickey(cfg.username.clone(), Arc::new(key))
                .await
                .map_err(|e| format!("ssh publickey auth error: {e}"))?
        }
        SshAuth::Agent => authenticate_agent(handle, &cfg.username).await?,
    };
    if ok {
        Ok(())
    } else {
        Err("ssh authentication rejected by bastion".to_string())
    }
}

/// Try every identity offered by the running ssh-agent.
async fn authenticate_agent(
    handle: &mut Handle<TunnelHandler>,
    username: &str,
) -> Result<bool, String> {
    use russh::keys::agent::client::AgentClient;
    let mut agent = AgentClient::connect_env()
        .await
        .map_err(|e| format!("no ssh-agent: {e}"))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| format!("ssh-agent request_identities failed: {e}"))?;
    if identities.is_empty() {
        return Err("ssh-agent has no identities loaded".to_string());
    }
    for key in identities {
        let (returned, res) = handle.authenticate_future(username, key, agent).await;
        agent = returned;
        if res.map_err(|e| format!("ssh-agent auth error: {e}"))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Spawn the accept loop: each inbound TCP connection gets its own direct-tcpip channel + pump.
fn spawn_accept_loop(
    listener: TcpListener,
    session: Arc<Handle<TunnelHandler>>,
    target: RemoteTarget,
    local_addr: SocketAddr,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let (mut inbound, peer) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break, // listener closed
            };
            let session = session.clone();
            let target = target.clone();
            let originator_ip = local_addr.ip().to_string();
            let originator_port = peer.port() as u32;
            tokio::spawn(async move {
                let channel = match session
                    .channel_open_direct_tcpip(
                        target.host.clone(),
                        target.port as u32,
                        originator_ip,
                        originator_port,
                    )
                    .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[tableiq ssh] direct-tcpip open failed: {e}");
                        return;
                    }
                };
                let mut stream = channel.into_stream();
                // Pump bytes both ways until either side closes.
                let _ = copy_bidirectional(&mut inbound, &mut stream).await;
            });
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::SshAuth;

    /// A throwaway host key. Each call is a different key, which is exactly what a
    /// man-in-the-middle would present.
    fn pubkey() -> key::PublicKey {
        russh::keys::key::KeyPair::generate_ed25519()
            .expect("generate ed25519")
            .clone_public_key()
            .expect("public half")
    }

    /// `known_host_keys_path` answers `Ok(vec![])` when it cannot open the file, which is
    /// indistinguishable from "no entry for this host" — so an unreadable `~/.ssh/known_hosts`
    /// fell through and TOFU'd whatever key was offered, even though that file may pin this
    /// very bastion. It must refuse instead.
    #[test]
    fn unreadable_known_hosts_is_refused_not_learned() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user_known_hosts");
        let app = dir.path().join("known_hosts");
        let recorded = pubkey();
        russh::keys::learn_known_hosts_path("bastion", 22, &recorded, &user).unwrap();
        std::fs::set_permissions(&user, std::fs::Permissions::from_mode(0o000)).unwrap();

        let impostor = pubkey();
        match verdict("bastion", 22, &impostor, &[user.clone(), app]) {
            Verdict::Changed(msg) => {
                assert!(msg.contains(&user.display().to_string()), "no file named in: {msg}")
            }
            v => panic!("an unreadable known_hosts must never fall through to TOFU, got {v:?}"),
        }
    }

    /// A *missing* file is not an unreadable one: it just means "ask the next file".
    #[test]
    fn a_missing_known_hosts_file_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let files = [dir.path().join("nope"), dir.path().join("known_hosts")];
        assert_eq!(verdict("h", 22, &pubkey(), &files), Verdict::Learn);
    }

    /// This is what a user reads while an interception may be under way; it must not be
    /// littered with stray runs of spaces from a wrapped format string.
    #[test]
    fn refusal_messages_are_cleanly_spaced() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("known_hosts");
        let recorded = pubkey();
        russh::keys::learn_known_hosts_path("bastion", 22, &recorded, &app).unwrap();
        let Verdict::Changed(msg) = verdict("bastion", 22, &pubkey(), &[app]) else {
            panic!("expected Changed");
        };
        for l in msg.lines() {
            assert!(!l.trim_start().contains("  "), "stray run of spaces in: {l:?}");
        }
    }

    /// An unrecorded host is trusted on first use and recorded in TableIQ's own store;
    /// the same key is `Known` afterwards.
    #[test]
    fn unknown_host_is_learned_then_known() {
        let dir = tempfile::tempdir().unwrap();
        let files = vec![dir.path().join("user_known_hosts"), dir.path().join("known_hosts")];
        let k = pubkey();
        assert_eq!(verdict("bastion.example.com", 2222, &k, &files), Verdict::Learn);
        russh::keys::learn_known_hosts_path("bastion.example.com", 2222, &k, &files[1]).unwrap();
        assert_eq!(verdict("bastion.example.com", 2222, &k, &files), Verdict::Known);
    }

    /// The whole point of the task: a DIFFERENT key for a recorded host is a hard error,
    /// and the message names the host, both fingerprints, and the file to edit.
    #[test]
    fn changed_key_is_a_hard_error_naming_host_fingerprints_and_file() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("known_hosts");
        let files = vec![dir.path().join("user_known_hosts"), app.clone()];
        let recorded = pubkey();
        russh::keys::learn_known_hosts_path("bastion.example.com", 22, &recorded, &app).unwrap();

        let impostor = pubkey();
        match verdict("bastion.example.com", 22, &impostor, &files) {
            Verdict::Changed(msg) => {
                assert!(msg.contains("bastion.example.com"), "no host in: {msg}");
                assert!(msg.contains(&recorded.fingerprint()), "no expected fingerprint in: {msg}");
                assert!(msg.contains(&impostor.fingerprint()), "no received fingerprint in: {msg}");
                assert!(msg.contains(&app.display().to_string()), "no file to edit in: {msg}");
            }
            v => panic!("a changed host key must never be trusted, got {v:?}"),
        }
    }

    /// `~/.ssh/known_hosts` is authoritative and is consulted first: a mismatch there is
    /// refused even though TableIQ's own store would have accepted the key.
    #[test]
    fn user_known_hosts_wins_over_the_app_store() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user_known_hosts");
        let app = dir.path().join("known_hosts");
        let files = vec![user.clone(), app.clone()];
        let recorded = pubkey();
        let offered = pubkey();
        russh::keys::learn_known_hosts_path("db.internal", 22, &recorded, &user).unwrap();
        russh::keys::learn_known_hosts_path("db.internal", 22, &offered, &app).unwrap();
        match verdict("db.internal", 22, &offered, &files) {
            Verdict::Changed(msg) => assert!(
                msg.contains(&user.display().to_string()),
                "the user's known_hosts must be the file named: {msg}"
            ),
            v => panic!("expected Changed from the user's known_hosts, got {v:?}"),
        }
    }

    /// TableIQ never writes to the user's `~/.ssh/known_hosts` — a learned key lands in
    /// its own store.
    #[test]
    fn learning_writes_only_to_the_app_store() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user_known_hosts");
        let app = dir.path().join("known_hosts");
        let k = pubkey();
        assert_eq!(verdict("h", 22, &k, &[user.clone(), app.clone()]), Verdict::Learn);
        russh::keys::learn_known_hosts_path("h", 22, &k, &app).unwrap();
        assert!(!user.exists(), "the user's known_hosts must not be created");
        assert!(app.exists());
    }

    /// An RSA host key, whose recorded form parses back with a DIFFERENT algorithm name
    /// than the one the session negotiates (`ssh-rsa` vs `rsa-sha2-256`/`512`).
    fn rsa_pubkey(hash: russh::keys::key::SignatureHash) -> key::PublicKey {
        russh::keys::key::KeyPair::generate_rsa(2048, hash)
            .expect("generate rsa")
            .clone_public_key()
            .expect("public half")
    }

    /// THE BYPASS: `check_known_hosts_path` only reports `KeyChanged` when the offered key's
    /// ALGORITHM NAME matches a recorded entry; a different algorithm returns `Ok(false)` =
    /// "not recorded here". A pinned host that suddenly offers an RSA key instead of its
    /// ed25519 one therefore fell through to `Learn` and the attacker's key was trusted.
    #[test]
    fn a_pinned_host_offering_a_different_algorithm_is_changed_not_learned() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("known_hosts");
        let files = vec![dir.path().join("user_known_hosts"), app.clone()];
        let recorded = pubkey(); // ed25519, pinned
        russh::keys::learn_known_hosts_path("bastion.example.com", 22, &recorded, &app).unwrap();

        // The man in the middle offers RSA instead — a different algorithm name.
        let impostor = rsa_pubkey(russh::keys::key::SignatureHash::SHA2_256);
        match verdict("bastion.example.com", 22, &impostor, &files) {
            Verdict::Changed(msg) => {
                assert!(msg.contains("bastion.example.com"), "no host in: {msg}");
                assert!(msg.contains(&impostor.fingerprint()), "no received fingerprint in: {msg}");
            }
            v => panic!("a pinned host must never be re-learned for a new algorithm, got {v:?}"),
        }
    }

    /// The same bug's other half: an RSA bastion never matched its OWN pin, because the
    /// recorded entry parses as one hash name while the offered key carries the negotiated
    /// one. It was re-learned on every single connect, growing known_hosts without bound.
    #[test]
    fn an_rsa_host_matches_its_own_pin_on_the_second_connect() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("known_hosts");
        let files = vec![dir.path().join("user_known_hosts"), app.clone()];
        // A recorded entry always parses back as `rsa-sha2-256` (russh-keys `parse_public_key`
        // hardcodes that default), so a session that negotiated `rsa-sha2-512` never matched.
        let k = rsa_pubkey(russh::keys::key::SignatureHash::SHA2_512);

        assert_eq!(verdict("rsa.example.com", 22, &k, &files), Verdict::Learn);
        russh::keys::learn_known_hosts_path("rsa.example.com", 22, &k, &app).unwrap();
        assert_eq!(
            verdict("rsa.example.com", 22, &k, &files),
            Verdict::Known,
            "the same RSA key must be recognised, not re-learned every connect"
        );
    }

    #[test]
    fn remote_target_constructs() {
        let t = RemoteTarget {
            host: "db.internal".into(),
            port: 5432,
        };
        assert_eq!(t.host, "db.internal");
        assert_eq!(t.port, 5432);
    }

    /// Live tunnel integration test — GATED behind env vars; skipped when unset (e.g. CI / this
    /// sandbox, where no real bastion exists). Honest gating per the spec.
    ///
    /// To run:
    ///   TIQ_SSH_TEST_HOST=bastion TIQ_SSH_TEST_USER=me TIQ_SSH_TEST_KEY=~/.ssh/id_ed25519 \
    ///   TIQ_SSH_TEST_REMOTE_HOST=127.0.0.1 TIQ_SSH_TEST_REMOTE_PORT=5432 \
    ///   cargo test --manifest-path src-tauri/Cargo.toml -- --ignored ssh_tunnel_live
    #[tokio::test]
    #[ignore]
    async fn ssh_tunnel_live() {
        let host = match std::env::var("TIQ_SSH_TEST_HOST") {
            Ok(h) => h,
            Err(_) => {
                eprintln!("TIQ_SSH_TEST_HOST unset — skipping live SSH tunnel test");
                return;
            }
        };
        let port = std::env::var("TIQ_SSH_TEST_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(22u16);
        let user = std::env::var("TIQ_SSH_TEST_USER").expect("TIQ_SSH_TEST_USER required");
        let key = std::env::var("TIQ_SSH_TEST_KEY").expect("TIQ_SSH_TEST_KEY required");
        let remote_host =
            std::env::var("TIQ_SSH_TEST_REMOTE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let remote_port = std::env::var("TIQ_SSH_TEST_REMOTE_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5432u16);

        let cfg = SshConfig {
            enabled: true,
            host,
            port,
            username: user,
            auth: SshAuth::KeyFile {
                path: key,
                passphrase: None,
            },
        };
        let tunnel = Tunnel::connect(
            &cfg,
            "",
            RemoteTarget {
                host: remote_host,
                port: remote_port,
            },
        )
        .await
        .expect("tunnel opens");
        let addr = tunnel.local_addr();
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
        assert_ne!(addr.port(), 0);
        // A TCP connection to the local forward should succeed (the bastion reaches the remote).
        let _conn = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect through tunnel");
        tunnel.close().await;
    }

    /// Full-path live test: open the tunnel AND connect Postgres through it with the
    /// app's own `PgDriver`, then run a real query. Success proves key auth +
    /// bastion + forward + DB auth all work. GATED on env; skipped when unset.
    ///
    /// To run (secrets stay in the env, never the repo):
    ///   TIQ_SSH_TEST_HOST=13.126.235.15 TIQ_SSH_TEST_USER=ubuntu \
    ///   TIQ_SSH_TEST_KEY=~/Downloads/key.pem \
    ///   TIQ_DB_HOST=172.17.0.1 TIQ_DB_PORT=5432 TIQ_DB_USER=postgres \
    ///   TIQ_DB_PASSWORD=… TIQ_DB_NAME=cartiq \
    ///   cargo test --manifest-path src-tauri/Cargo.toml -- --ignored ssh_pg_live
    #[tokio::test]
    #[ignore]
    async fn ssh_pg_live() {
        use crate::drivers::postgres::PgDriver;
        use crate::drivers::SqlDriver;

        let host = match std::env::var("TIQ_SSH_TEST_HOST") {
            Ok(h) => h,
            Err(_) => {
                eprintln!("TIQ_SSH_TEST_HOST unset — skipping live SSH+PG test");
                return;
            }
        };
        let cfg = SshConfig {
            enabled: true,
            host,
            port: std::env::var("TIQ_SSH_TEST_PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(22),
            username: std::env::var("TIQ_SSH_TEST_USER").expect("TIQ_SSH_TEST_USER"),
            auth: SshAuth::KeyFile {
                path: std::env::var("TIQ_SSH_TEST_KEY").expect("TIQ_SSH_TEST_KEY"),
                passphrase: None,
            },
        };
        let db_host = std::env::var("TIQ_DB_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let db_port: u16 = std::env::var("TIQ_DB_PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(5432);
        let db_user = std::env::var("TIQ_DB_USER").unwrap_or_else(|_| "postgres".into());
        let db_pw = std::env::var("TIQ_DB_PASSWORD").unwrap_or_default();
        let db_name = std::env::var("TIQ_DB_NAME").unwrap_or_default();

        let tunnel = Tunnel::connect(&cfg, "", RemoteTarget { host: db_host, port: db_port })
            .await
            .expect("tunnel opens");
        let local = tunnel.local_addr();
        let url = format!(
            "postgres://{}:{}@127.0.0.1:{}/{}?sslmode=prefer",
            db_user,
            db_pw,
            local.port(),
            db_name
        );
        let drv = PgDriver::connect(&url).await.expect("postgres connects through tunnel");
        let dbs = drv.list_databases().await.expect("a real query runs over the tunnel");
        eprintln!("CONNECTED · {} databases visible (incl. target {db_name})", dbs.len());
        assert!(!dbs.is_empty(), "expected at least one database");
        tunnel.close().await;
    }

    /// Reproduce the inline-edit path end-to-end against a live table: detect the
    /// PK, read one row, then apply an edit setting a column to ITS CURRENT VALUE
    /// (a no-op write — zero data change) and assert rows_affected == 1. This is
    /// exactly what the grid does on a cell edit, so a failure here is the bug.
    /// GATED on env; reads TIQ_EDIT_TABLE / TIQ_EDIT_PK / TIQ_EDIT_COL.
    #[tokio::test]
    #[ignore]
    async fn ssh_apply_edit_live() {
        use crate::drivers::postgres::PgDriver;
        use crate::drivers::{EditSpec, SqlDriver};
        use sqlx::{Connection, Row};

        let host = match std::env::var("TIQ_SSH_TEST_HOST") {
            Ok(h) => h,
            Err(_) => {
                eprintln!("TIQ_SSH_TEST_HOST unset — skipping live apply_edit test");
                return;
            }
        };
        let cfg = SshConfig {
            enabled: true,
            host,
            port: std::env::var("TIQ_SSH_TEST_PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(22),
            username: std::env::var("TIQ_SSH_TEST_USER").expect("TIQ_SSH_TEST_USER"),
            auth: SshAuth::KeyFile {
                path: std::env::var("TIQ_SSH_TEST_KEY").expect("TIQ_SSH_TEST_KEY"),
                passphrase: None,
            },
        };
        let db_host = std::env::var("TIQ_DB_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let db_port: u16 = std::env::var("TIQ_DB_PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(5432);
        let db_user = std::env::var("TIQ_DB_USER").unwrap_or_else(|_| "postgres".into());
        let db_pw = std::env::var("TIQ_DB_PASSWORD").unwrap_or_default();
        let db_name = std::env::var("TIQ_DB_NAME").unwrap_or_default();
        let schema = std::env::var("TIQ_EDIT_SCHEMA").unwrap_or_else(|_| "public".into());
        let table = std::env::var("TIQ_EDIT_TABLE").unwrap_or_else(|_| "accounts_user".into());
        let pk_col = std::env::var("TIQ_EDIT_PK").unwrap_or_else(|_| "id".into());
        let col = std::env::var("TIQ_EDIT_COL").unwrap_or_else(|_| "is_superuser".into());

        let tunnel = Tunnel::connect(&cfg, "", RemoteTarget { host: db_host, port: db_port })
            .await
            .expect("tunnel opens");
        let url = format!(
            "postgres://{}:{}@127.0.0.1:{}/{}?sslmode=prefer",
            db_user, db_pw, tunnel.local_addr().port(), db_name
        );

        // 1) PK detection (drives `editable` in the UI).
        let drv = PgDriver::connect(&url).await.expect("connect");
        let pk = drv.primary_key(&schema, &table).await.expect("primary_key query");
        eprintln!("PK({table}) = {pk:?}");
        assert!(!pk.is_empty(), "no PK detected → grid would be read-only");

        // 2) Read one row's pk + target column as text (how the grid sees them).
        let mut conn = sqlx::postgres::PgConnection::connect(&url).await.unwrap();
        let q = format!(
            "SELECT \"{pk_col}\"::text AS pk, \"{col}\"::text AS val FROM \"{schema}\".\"{table}\" LIMIT 1"
        );
        let row = sqlx::query(&q).fetch_one(&mut conn).await.expect("read a row");
        let pkv: Option<String> = row.get("pk");
        let cur: Option<String> = row.get("val");
        let _ = conn.close().await;
        eprintln!("row {pk_col}={pkv:?} {col}={cur:?}");

        // 3) Apply a no-op edit (col := its current value) — the exact grid path.
        let edit = EditSpec {
            pk: vec![(pk_col.clone(), pkv)],
            column: col.clone(),
            new_value: cur.clone(),
            orig_value: cur.clone(),
        };
        let affected = drv.apply_edit(&schema, &table, &edit).await.expect("apply_edit runs");
        eprintln!("apply_edit rows_affected = {affected}");
        assert_eq!(affected, 1, "edit did not affect exactly one row");
        tunnel.close().await;
    }
}
