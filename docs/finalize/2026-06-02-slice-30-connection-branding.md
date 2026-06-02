# Finalize — Slice 30: Connection Manager + branding overhaul + connect bug

Date: 2026-06-02 · Branch: `slice-30-connection-branding` · Base: `main`

## Task

Fix the real connect bug, polish the connection form to UI/UX best practice +
TablePlus parity, refresh the logo to an AI star, and add TablePlus-style test
feedback. Driven autonomously through the full pipeline. The user also asked, as
a follow-up, to connect into the DB and verify the earlier Slice 29 internal
pages still match.

## What shipped

### 1. The connect bug — real error instead of "pool timed out"  (the #1 ask)
Connecting to an unreachable Postgres (nothing on `127.0.0.1:5432` — port is
refused here) hung ~30s then reported `db error: pool timed out while waiting for
an open connection`. Root cause: `PgPoolOptions::connect` with sqlx's default 30s
`acquire_timeout` retries the failed connection and returns the opaque
`PoolTimedOut`, masking the real cause.

- `PgDriver` / `MySqlDriver::connect` now **probe with a single direct connection
  first**, so the true error (`connection refused`, `password authentication
  failed`, `database "x" does not exist`) surfaces immediately; pool
  `acquire_timeout` cut to 10s.
- `ConnConfig::url_with_password` (postgres): an empty `database` now defaults to
  the **username** (psql/TablePlus behavior) instead of emitting `…/?sslmode=…`.
- Offline-deterministic Rust tests: closed-port probe returns the real error in
  **~0.01s** (was a 30s mask); empty-db URL defaults correctly.

### 2. Branding — AI sparkle logo
`BrandMark` + `glyph.svg` + `icon-tile.svg`: the flat circle "dot over the T"
became a 4-point **AI sparkle** in an indigo→violet gradient (DESIGN.md reserves
violet/gradient for AI). Per-instance gradient id via `useId()`.

### 3. Connection form (UI/UX + TablePlus parity)
- **Consistent fields** — inputs, selects, and number fields all `h-[34px]`,
  aligned 2-col grid (root font is 12px, so explicit px height; rem `h-*` shrank).
  Fixes the mismatched-height UX bug.
- **Bigger close** — `IconClose` in an `h-8 w-8` target (was a tiny `×`).
- **SSH section** — native **PEM / key-file picker** (`pickKeyFile` → Tauri dialog
  plugin) with a Browse button + manual-path fallback; keychain note on secrets.
- **Test feedback** — TablePlus-style: green "Connection OK" banner + green field
  rings on success; red banner with the **real error** on failure; clears on edit.
- **Color tag** — larger ringed dot in the list + bigger swatch with a focus ring.
- **SSL modes** — added `allow` / `verify-ca` / `verify-full` (cert-verifying
  modes sqlx already supports); number-input spinners hidden.

### 4. Follow-up — internal pages still match (Slice 29)
Connected into the workspace and re-verified: the **centered connection capsule**,
**grid auto-fit + zebra + right-aligned numerics**, and the **single bottom bar**
from Slice 29 all render intact, now with the new sparkle logo in the top bar. No
regression.

## Counter-proposals (deferred, by design)
- **SSL client Key/Cert/CA files + Negotiation**: deferred — needs `ConnConfig` +
  sqlx-TLS wiring; rarely used. (verify-ca/verify-full server modes shipped.)
- **Explicit "store in keychain" toggle + dedicated environment/Tag field**:
  TableIQ already stores every secret in the OS keychain transparently (arguably
  better than a toggle), and derives the env tag from color/host. A real Tag field
  needs a `ConnConfig` change — noted as a follow-up. Communicated via placeholder
  + the larger color dot for now.

## Files

| Area | Files |
|---|---|
| Bug | `src-tauri/src/drivers/postgres.rs`, `drivers/mysql.rs`, `connections.rs` |
| Dialog plugin | `Cargo.toml`, `src/lib.rs`, `capabilities/default.json`, `package.json` |
| Branding | `src/components/Brand.tsx`, `assets/brand/glyph.svg`, `icon-tile.svg` |
| Form | `src/components/ConnectionManager.tsx`, `src/lib/commands.ts`, `src/index.css` |

## Verification — checked thoroughly

- **Rust**: `cargo test --lib` → **88 passed** (incl. 2 new bug tests); `cargo
  build` clean (no warnings). Native Tauri app rebuilt with the dialog plugin and
  relaunched successfully.
- **Frontend**: `tsc --noEmit` clean; **137 vitest tests** pass.
- **Code review** (2 agents: correctness + design): all surfaced findings either
  fixed (engine-select stale status + neutral border, Browse try/catch, per-
  instance gradient id, SSL verify modes, number spinners, default-color const,
  color focus ring) or deferred-by-design (above).
- **`/browse` QA** (browser-mock, light + dark, zero console errors): new logo,
  consistent fields, bigger close, SSH PEM Browse button, green test banner, all 6
  SSL modes in the DOM; **internal workspace pages** (capsule + grid + bottom bar)
  verified intact.

## Screenshots

Logo (sparkle) · connection form with SSH + PEM Browse · green test feedback:

![logo](assets/slice-30/logo-sparkle-light.png)
![editform](assets/slice-30/editform-ssh-light.png)
![test](assets/slice-30/test-feedback-green.png)

Form dark / final light:

![dark](assets/slice-30/form-dark.png)
![final](assets/slice-30/form-final-light.png)

Internal pages still match (Slice 29) — new logo in the capsule, grid + bottom bar intact:

![capsule](assets/slice-30/workspace-capsule-newlogo.png)
![grid](assets/slice-30/workspace-grid-intact.png)
![bottombar](assets/slice-30/workspace-bottombar-intact.png)

## Per-stage cost

> Attribution is approximate (session-window bucketing across two slices in one
> session; the Brainstorm bucket absorbs spec/plan/QA windows).

| Stage | Model | Input | Output | Cache read | Cache write | Cost (USD) |
|---|---|---|---|---|---|---|
| 1. Brainstorm (+spec/plan/QA) | opus | 65,278 | 278,107 | 68,540,952 | 1,615,147 | $154.93 |
| 4. TDD-backend | opus | 1,712 | 16,322 | 12,390,649 | 27,527 | $20.35 |
| 5. Frontend | opus | 7,153 | 78,754 | 35,633,153 | 83,449 | $61.03 |
| 7. Review | opus | 22,733 | 39,423 | 27,340,514 | 57,738 | $45.39 |
| **Total** | | **~96.9k** | **~412.6k** | **~143.9M** | **~1.78M** | **~$282** |

## Gate

Backend + frontend tests green, builds clean, QA clean, internal pages verified
matching. **Clean → merging to main** (autonomous build mode).

## Addendum — SSH section rebuilt to TablePlus + a real keyfile bug

After review, the SSH section was reworked to match TablePlus exactly (the Auth
dropdown had buried the PEM picker):

- **Server / Port / User**, an always-visible **Password** (empty → ssh-agent),
  and a **"Use SSH key (PEM / OpenSSH)"** checkbox that reveals the key-file
  picker (path + **Browse…** + Clear) and an optional passphrase. No dropdown.
- Split the SSH secret into password vs key-passphrase state (both shown, like
  TablePlus); only the active method's secret is sent.
- Backend: an empty SSH password falls back to the ssh-agent.

**Real bug found + fixed (latent):** `SshAuth` used `rename_all = "snake_case"`,
so the `KeyFile` wire tag was `"key_file"`, but the TS client sends `"keyfile"`.
Saving/connecting *any* key-auth SSH connection failed at the IPC boundary with
`unknown variant 'keyfile'`. Pinned the tag to `"keyfile"` (+ a test that
deserializes the exact frontend JSON). **This would have blocked the cartiq SSH
connection entirely.** 89 Rust + 137 frontend tests green.

Created the `cartiq - dev server` connection (Server `13.126.235.15:22`, user
`ubuntu`, key `~/Downloads/prod-rhobots.pem`, DB `cartiq`) in the app store. The
actual connect is left to the user: it targets a production bastion and the DB
password is masked in the reference screenshot.

![ssh-password](assets/slice-30/ssh-tableplus-password.png)
![ssh-keyfile](assets/slice-30/ssh-tableplus-keyfile.png)
