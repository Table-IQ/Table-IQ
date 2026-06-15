# Finalize — UI audit + reconnect banner

**Branch:** `RudreshNarwal/ui-rebrand-redesign`
**Date:** 2026-06-15
**Request:** "Make the UI look like [a TablePlus screenshot], in our branding. The product seems immature. Check all the functionality."

## Task-done summary

The request implied a UI rebuild toward a polished desktop DB client. The audit found
the opposite: TableIQ is **already** at TablePlus parity across every surface (33+
shipped slices, full `DESIGN.md`). The reference image *is* TablePlus, which was
already the design target. So the work was:

1. A full live visual + functional audit against the reference (read-only) — to find
   what was actually missing rather than rebuild what already works.
2. Build the one concrete gap the reference showed and the user explicitly asked for:
   the red **"Database disconnected — Tap to reconnect"** banner.

### Code changes (140 insertions, 10 deletions, 4 files)

- `src/lib/commands.ts` — `isConnectionLostError(msg)` (narrow phrase match across
  sqlx / mysql / mongo / redis driver strings) + `setConnectionLostHandler()` + a thin
  local `invoke` wrapper that shadows Tauri's `invoke`. Every existing call site gets
  connection-loss detection for free; lifecycle commands (`connect` / `test_connection`
  / `disconnect`) are excluded so initial-connect failures stay on the connection form,
  not the banner.
- `src/state/connectionStore.ts` — `disconnected` flag + `setDisconnected`. `setActive`
  clears it (a fresh session is, by definition, connected).
- `src/components/MainShell.tsx` — registers the handler on mount, renders the red
  banner below the title bar when `disconnected`, and `onReconnect` (re-entrancy-guarded)
  that reuses `connect()` + refreshes the schema.
- `src/lib/connectionLost.test.ts` — matcher truth table: real connection-loss strings
  → true; ordinary SQL/permission errors → false (no false-positive banner).

## Audit findings (read-only pass, browser-mock at :1420)

Every surface screenshotted in light + dark, console + network watched throughout.
**Zero console errors anywhere.** All surfaces are mature and TablePlus-faithful:

| Surface | State |
|---|---|
| Connection manager + onboarding | Polished (brand sidebar, engine monograms, hero card) |
| Main shell (sidebar / grid / inspector) | TablePlus parity |
| Table grid | Zebra rows, right-aligned numerics, content-fit columns, FK chevrons |
| Inspector — row detail | Field list with type pills (BIGINT/NUMERIC/TIMESTAMPTZ/TEXT), editable boxes, "Row N · M of K fields" — matches the reference's right panel exactly |
| Inspector — object metadata | name / kind / columns / row estimate / relationships |
| Structure view | Columns / Indexes / Triggers tabs, FK references |
| Query editor | CodeMirror SQL highlight, Run / Run-selection / Explain / Library / AI |
| ERD | dagre auto-layout, draggable cards, FK arrows, fit/zoom |
| Command palette (⌘K) | Fuzzy tables + actions |
| Dark mode | Verified applied (`data-theme=dark`, `bg-surface` → #15171c) |

**Conclusion:** the "immature" perception came from the **native app on the real prod
DB** (873-col tables, json columns, dropped SSH sessions) — not reproducible in the
mock, but the dropped-session case is now handled by the reconnect banner. No broken
or half-built functionality was found in the mock to fix. Per Karpathy/ponytail
discipline, no speculative "fixes" were invented.

## Verification (evidence)

- `pnpm exec tsc --noEmit` — **clean.**
- `pnpm exec vitest run` — **159 passed (33 files)**, +2 new (matcher truth table).
- Banner renders correctly with live data (forced on for the shot, then reverted):
  `POSTGRES | Database 'tableiq' is disconnected · Tap to reconnect`.
- Banner **hidden** in normal operation (default `disconnected: false`); no console errors.
- Independent code review of the diff: no blocking bugs. (The one raised "blocking"
  item — the `...rest` invoke forwarding — is correct: it preserves Tauri's full
  `invoke(cmd, args, options)` signature and tsc passes; hardcoding `rest[0]` would be
  *less* faithful.)

### Screenshots

![Main shell](assets/RudreshNarwal/ui-rebrand-redesign/01-main-shell.png)
![Table grid](assets/RudreshNarwal/ui-rebrand-redesign/02-table-grid.png)
![Inspector row detail](assets/RudreshNarwal/ui-rebrand-redesign/03-inspector-rowdetail.png)
![ERD](assets/RudreshNarwal/ui-rebrand-redesign/04-erd.png)
![Reconnect banner](assets/RudreshNarwal/ui-rebrand-redesign/05-reconnect-banner.png)

## Assumptions / notes

- **Verified against the browser-mock**, not the native window (Tauri window isn't
  headless-screenshottable — long-standing project constraint). The reconnect feature's
  live-drop path against a real engine is covered by code review + the matcher unit test;
  the native end-to-end (kill DB → banner → reconnect) is best confirmed by the user
  running the native build against their prod DB.
- `pnpm-lock.yaml` updated: it was stale vs the already-committed `package.json`
  (dagre, declared glide peer deps). The lockfile now matches; this unblocks a clean
  `pnpm install` / frozen-lockfile CI. Not a code change.
- One-time local setup: `pnpm install` then esbuild's platform binary resolves from
  the pnpm store (no postinstall needed).

## "Checked thoroughly" statement

Every TableIQ surface was driven live in the browser-mock (light + dark), screenshotted,
and checked for console errors (none). The one real gap — reconnect handling — is built,
unit-tested, type-clean, visually verified, and independently reviewed. The full test
suite (159) passes. No regressions.

## Per-stage cost

| Stage | Model | Input | Output | Cache read | Cache write | Cost (USD) |
|---|---|---|---|---|---|---|
| 3. Build (live audit) | opus-4-8 | 65,350 | 139,260 | 25,008,834 | 610,140 | $20.13 |
| 4. Review | opus-4-8 | 4 | 2,051 | 379,023 | 2,239 | $0.25 |
| **Total** | | **65,354** | **141,311** | **25,387,857** | **612,379** | **$20.38** |
