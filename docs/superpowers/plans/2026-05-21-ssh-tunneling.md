# Slice 5 — SSH Tunneling (Plan)

Bite-sized TDD tasks. RED (write test, watch fail) → GREEN (implement). Commit per task group.

## T0 — Spec + plan
- [x] Write spec + plan. Commit.

## T1 — Cargo deps
- [ ] Add `russh` (latest 0.4x/0.5x that compiles) + `russh-keys` if separate. `cargo build`
      must succeed before any logic. Commit on green.

## T2 — SshConfig + serde (RED→GREEN)
- [ ] Tests in `ssh/mod.rs` (or `ssh/tunnel.rs`): serde roundtrip for `SshConfig` with each
      `SshAuth` variant; `ssh_secret_account("abc") == "ssh:abc"`.
- [ ] Implement `SshConfig`, `SshAuth`, `ssh_secret_account`.

## T3 — ConnConfig integration (RED→GREEN)
- [ ] Tests in `connections.rs`: legacy JSON without `ssh` deserializes (ssh == None);
      `with_host_port("127.0.0.1", 12345)` makes `url_with_password` target the local forward
      (postgres + mysql); export bundle with an SSH config present has no "passphrase"/"password".
- [ ] Add `#[serde(default)] ssh: Option<SshConfig>` to `ConnConfig`; add `with_host_port`;
      add `ssh_secret_account` re-export. Fix existing struct literals across the crate.

## T4 — ssh_config Host-alias resolver (RED→GREEN)
- [ ] Tests: parse a fixture string, resolve a Host alias to HostName/Port/User; miss falls back.
- [ ] Implement `ssh::ssh_config::resolve_host_alias(text, alias)` (small parser).

## T5 — Tunnel skeleton + live test gating (RED→GREEN where possible)
- [ ] Implement `Tunnel` (russh client, direct-tcpip forward, local listener, byte pump,
      `local_addr`, `close`/`Drop`). Host-key: accept-and-record with eprintln fingerprint.
- [ ] Live integration test gated behind `TIQ_SSH_TEST_HOST` (skips when unset).

## T6 — Wire into connect.rs (GREEN)
- [ ] `ConnState.tunnels: Mutex<HashMap<String, Tunnel>>`. In `connect`, if ssh enabled: open
      tunnel, rewrite host/port, connect driver against local addr, store tunnel. `disconnect`
      + `delete_connection` drop the tunnel. Pull ssh secret from `ssh:<id>` keychain account.
- [ ] `save_connection` stores the ssh secret under `ssh:<id>` when provided.

## T7 — Frontend (RED→GREEN)
- [ ] `src/lib/ssh.ts`: `defaultSshConfig()`, `sshConfigValid()`. Tests in `ssh.test.ts`.
- [ ] `ConnConfig` TS type gets `ssh?`. `ConnectionManager.tsx` collapsible "SSH Tunnel"
      section (enable toggle, host, port, user, auth mode, key path, password/passphrase).
- [ ] Browser mock: add an SSH-enabled mock connection so the section renders headless.

## T8 — Green gate + commit
- [ ] `cargo build` + `cargo test` (manifest) green; `npm test` + `npm run build` green.
      Commit. Do NOT merge.
