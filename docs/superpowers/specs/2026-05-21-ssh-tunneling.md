# Slice 5 — SSH Tunneling (Spec)

Tunnel a DB connection through an SSH bastion via a real, in-process **local port-forward**
(russh, pure Rust). When a connection has SSH enabled, open the tunnel first, point the DB
driver at the local forwarded address (`127.0.0.1:<ephemeral>`), and tear the tunnel down on
disconnect. Non-SSH connections are completely unchanged.

## Goal
Reach databases that only listen on a private network behind an SSH jump host, without shelling
out to the system `ssh` binary and without changing any driver code — the driver just connects
to a local socket.

## SshConfig shape
```rust
struct SshConfig {
    enabled: bool,
    host: String,          // bastion host
    port: u16,             // bastion ssh port (default 22)
    username: String,
    auth: SshAuth,         // how we authenticate to the bastion
}

enum SshAuth {
    Password,                                        // password from keychain
    KeyFile { path: String, passphrase: Option<String> }, // passphrase from keychain (never on disk)
    Agent,                                           // ssh-agent (no secret stored)
}
```
`SshConfig`/`SshAuth` derive `Serialize`/`Deserialize`. They are embedded on `ConnConfig` as
`#[serde(default)] ssh: Option<SshConfig>`, so existing saved `connections.json` still
deserializes (missing field ⇒ `None`).

### Secrets handling (mirrors the existing password pattern)
- `ConnConfig` carries **no** SSH secret. The `KeyFile.passphrase` and `Password` mode's
  password are **never** serialized to disk and **never** included in the export bundle.
- SSH secrets live in the OS keychain under a derived account `ssh:<conn_id>` (see
  `connections::ssh_secret_account`). This mirrors the DB password living under `<conn_id>`.
- Result: the export/import bundle stays secret-free by construction (verified by test) even
  when an SSH config is present, because `SshAuth::KeyFile.passphrase` is the only secret-shaped
  field and it is only ever populated transiently from the keychain during a connect, never
  saved into the stored `ConnConfig`.

## Auth modes
- **Password** — interactive-style password auth to the bastion; password read from keychain
  (`ssh:<conn_id>`).
- **KeyFile** — private key at `path`, decrypted with an optional `passphrase` (keychain).
- **Agent** — delegate to the running `ssh-agent` (no stored secret). On platforms/sessions
  without an agent this fails fast with a clear error.

### ~/.ssh/config resolution (optional helper)
A minimal Host-alias lookup (`ssh::ssh_config::resolve_host_alias`) parses `~/.ssh/config`
for `Host <alias>` blocks and pulls `HostName`, `Port`, `User`, `IdentityFile`. If a user
enters a Host alias as the bastion host, we expand it. This is intentionally a small parser
(no `Match`, no `Include`, no token expansion beyond `~`); anything unmatched falls back to
using the literal values the user typed. Documented limitation, noted in the plan.

## Lifecycle
1. `connect(id)`: load `ConnConfig`. If `cfg.ssh` is `Some(s)` with `s.enabled`:
   a. Resolve secrets from keychain (`ssh:<conn_id>` ⇒ password or key passphrase).
   b. Open a `Tunnel` to the bastion that forwards to the **remote** `cfg.host:cfg.port`.
   c. Take `tunnel.local_addr()` → `127.0.0.1:<ephemeral>`.
   d. Build the driver URL against the **local** addr (host/port rewritten), not the remote.
   e. Connect the driver; store the live `Tunnel` in `ConnState.tunnels[id]`.
2. `disconnect(id)` / `delete_connection(id)`: drop the driver session **and** the tunnel.
   `Tunnel::Drop` stops the local listener and disconnects the SSH session.

### URL rewriting
The host/port substitution is a pure function `ConnConfig::with_host_port(host, port)` that
returns a clone with `host`/`port` replaced; the connect path calls
`cfg.with_host_port("127.0.0.1", local_port).url_with_password(pw)`. For sqlite (a file, no
host) SSH is a no-op. This keeps URL building in one place and makes the rewrite unit-testable
without any network.

## Tunnel internals
- `Tunnel::connect(SshConfig, secret, RemoteTarget) -> Tunnel`:
  - russh client connects to the bastion and authenticates per `SshAuth`.
  - Binds `TcpListener` on `127.0.0.1:0` (kernel picks an ephemeral port).
  - Spawns an accept loop: for each inbound TCP connection, opens a russh
    `channel_open_direct_tcpip(remote_host, remote_port, "127.0.0.1", local_port)` and pumps
    bytes bidirectionally (tokio copy both ways) until either side closes.
- `local_addr() -> SocketAddr` returns the bound local address.
- `close()` / `Drop` aborts the accept task and disconnects the SSH handle.

### Host-key handling (v1 limitation — documented, not hidden)
v1 uses **accept-and-record**: the bastion's host key is accepted on first use and logged via
`eprintln!` (host + key fingerprint) so the user can see what was trusted. This is a known
weakness (no TOFU pinning / no `known_hosts` verification yet) and is called out here and in the
UI copy. It is **not** silent: we never pretend verification happened. A follow-up slice should
add `known_hosts` pinning. We deliberately do not implement blind-accept-without-logging.

## Testing approach
Pure / no-network unit tests (run here + CI):
- `SshConfig`/`SshAuth` serde roundtrip (all three auth variants).
- `ConnConfig` with `ssh: None` still deserializes from legacy JSON (missing field).
- `ssh_secret_account(id)` derivation == `"ssh:<id>"`.
- `~/.ssh/config` Host-alias resolution (parse a fixture string, find alias, fall through on miss).
- URL rewriting: `with_host_port("127.0.0.1", p)` makes `url_with_password` target the local
  forward, not the remote host, for postgres/mysql.
- Export bundle stays secret-free with an SSH config present (no "passphrase"/"password" in JSON).

Live tunnel integration test — **gated** behind `TIQ_SSH_TEST_HOST` (plus `_PORT`/`_USER`/
`_KEY`). A real bastion is not available in this sandbox/CI, so it is skipped by default. The
byte-pump and listener-binding logic is structured so the non-network parts (config, URL
rewrite, secret derivation) are fully unit-tested without a server. The gating is honest: when
the env var is unset the test returns early (documented).

Frontend (vitest): pure helpers `defaultSshConfig()` and `sshConfigValid()` get unit tests
(defaults, validation per auth mode). The `ConnectionManager` SSH section renders in browser
(non-Tauri) mode via the existing mock backend.

## Out of scope
- `known_hosts` pinning / TOFU UI (follow-up).
- ProxyJump chains / multiple hops, `Match`/`Include` in ssh_config.
- Forwarding for sqlite (file engine — SSH ignored).
