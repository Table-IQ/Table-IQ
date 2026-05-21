//! A live SSH local port-forward (russh).
//!
//! `Tunnel::connect` authenticates to a bastion, binds `127.0.0.1:0`, and for every inbound
//! TCP connection opens a `direct-tcpip` channel to the remote `host:port` and pumps bytes
//! both ways. `local_addr()` is what the DB driver connects to. Dropping the tunnel (or
//! `close()`) stops the listener and disconnects the SSH session.
//!
//! Host-key handling (v1): accept-and-record. The bastion key fingerprint is logged via
//! `eprintln!` so the trust decision is visible. This is a documented limitation (no
//! `known_hosts` pinning yet) — see the spec.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use russh::client::{self, Handle, Handler};
use russh::keys::key;
use tokio::io::copy_bidirectional;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use super::{SshAuth, SshConfig};

/// The remote endpoint (the DB) reachable from the bastion.
#[derive(Debug, Clone)]
pub struct RemoteTarget {
    pub host: String,
    pub port: u16,
}

/// russh client handler. Accepts-and-records the server host key (v1).
struct TunnelHandler;

#[async_trait]
impl Handler for TunnelHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // v1: accept-and-record. We do NOT silently swallow this — the fingerprint is surfaced
        // so the user can see exactly what was trusted. Follow-up: known_hosts pinning.
        eprintln!(
            "[tableiq ssh] accepting bastion host key (fingerprint {})",
            server_public_key.fingerprint()
        );
        Ok(true)
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

        let mut handle = client::connect(config, (cfg.host.as_str(), cfg.port), TunnelHandler)
            .await
            .map_err(|e| format!("ssh connect to bastion failed: {e}"))?;

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
}
