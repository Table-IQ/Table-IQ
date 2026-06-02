# Spec — Slice 30: Connection Manager + branding overhaul + connect bug

Date: 2026-06-02 · Branch: `slice-30-connection-branding`

## Reference

User-supplied screenshots: TableIQ connection manager (3 shots) + TablePlus's
PostgreSQL connection dialog. Goal: fix a real connect bug, polish the connection
form to UI/UX best practice + TablePlus parity, refresh the logo, and give
TablePlus-style test feedback. Run autonomously.

## Problems (observed)

1. **Connect bug — masked error.** Connecting to `127.0.0.1:5432` (no server
   there — port is refused) shows `db error: pool timed out while waiting for an
   open connection` after a ~30s hang. Root cause: `PgDriver::connect` uses
   `PgPoolOptions::connect(url)` with sqlx's default 30s `acquire_timeout`; on a
   connection error the pool **retries until timeout** and returns the generic
   `PoolTimedOut`, masking the real cause (connection refused / auth failed /
   database does not exist). Empty `database` also yields `…/?sslmode=…` (no db).
2. **Logo.** The "dot over the T" is a plain circle; user wants an AI **star**.
   T color (indigo) + dot (violet) "not up to the mark."
3. **Close ×** on the edit side-sheet is too small.
4. **Form field sizing.** Inputs (`py-1.5` text), `<select>`, and `type=number`
   render at different heights/widths — inconsistent vertical rhythm. UX bug.
5. **SSH section.** No PEM key-file picker (path is hand-typed); keychain
   affordance unclear; color-tag dot barely visible.
6. **Test feedback.** Errors show as small muted text; no green/red signal.
   TablePlus highlights success green and failure red with a message.

## Goals

### A. Backend bug fix (Rust)
- **A1** `ConnConfig::url_with_password` (postgres): empty `database` → default to
  `username` (psql / TablePlus behavior). Pure, unit-tested.
- **A2** `PgDriver::connect` (+ MySQL): **surface the real error fast** — validate
  with a single direct `PgConnection::connect` before building the pool so the
  true error (`connection refused`, `password authentication failed`, `database
  "x" does not exist`) propagates instead of `pool timed out`. Set
  `acquire_timeout` to ~10s. Unit/integration test: connecting to a closed port
  returns a real connection error (not "pool timed out"), quickly.

### B. Branding (frontend)
- **B1** Replace the circle in `BrandMark` + `glyph.svg` + `icon-tile.svg` with a
  4-point AI **sparkle/star**; refine colors (T = brand indigo; star = vivid
  violet→indigo AI accent). Wordmark unchanged in structure.

### C. Connection form (frontend)
- **C1** Shared field system: inputs, selects, and number fields all **32px tall**,
  same radius/border/focus; aligned 2-col grid; consistent label spacing.
- **C2** Bigger, clearer **close ×** (icon, larger hit area).
- **C3** SSH section (TablePlus parity): **PEM key-file picker** (native Tauri
  dialog when available, manual path fallback), keychain note on secret fields,
  clearer grid. **Color-tag dot** more visible (larger + ring).
- **C4** **Test feedback**: success → green banner "Connection OK" + green ring on
  fields; failure → red banner with the real error + red ring; busy → spinner.
  Mirrors TablePlus.

## Counter-proposals (deferred, documented in report)
- SSL client **Key/Cert/CA** files + Negotiation: defer — needs `ConnConfig` +
  sqlx-TLS wiring; rarely used. Note as follow-up.
- Dedicated `environment` Tag field (vs color-derived tag): keep color-derived.

## Non-goals
- No change to the data grid, query view, or windowing.
- SSH tunnel timeouts unchanged (placeholder bastion is user error; the faster DB
  timeout already helps).

## Testable logic
- Rust: `url_with_password` empty-db default (pure); `PgDriver::connect` real-error
  surfacing (closed-port integration, offline-deterministic).
- TS: SSH config validity already tested; add nothing unless logic added.

## Verification
- `cargo test` (connections + driver) green; `cargo build` clean.
- `tsc` + vitest green.
- `/browse` screenshots: new logo, consistent fields, bigger ×, SSH PEM picker,
  green/red test feedback (light + dark). Run the Tauri app to confirm the real
  connect error now shows.
