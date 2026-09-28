# Finalize: New/Edit Connection sheet redesign

- **Branch:** `RudreshNarwal/connection-sheet`
- **Base:** local `main` @ `152e9e9` (Connected Home merged)
- **Date:** 2026-09-28
- **Design:** approved mockup v2, panels A and B: `~/.gstack/projects/RudreshNarwal-Table-IQ/designs/connection-sheet-20260928/mockup-v2.html`. The user said "do as you want".
- **Plan:** `docs/superpowers/plans/2026-09-28-connection-sheet.md`

## Before and after
| Before | Approved design |
|---|---|
| ![before](assets/connection-sheet/conn-new-current.jpg) | ![mockup](assets/connection-sheet/mockup-v2.jpg) |

## What shipped
- **Header:** the engine icon plus "New Connection", or the connection name when editing. It shows "● Unsaved changes" when there are edits. When editing, a ⋯ menu offers Copy as URL (no password), Duplicate and Delete.
- **Paste a URL:** `postgres://`, `postgresql://`, `mysql://`, `mariadb://`, `sqlserver://`, `mssql://`, `redis://`, `mongodb://`, `sqlite://` and `duckdb://` fill every field.
  - Pasting into Host also works.
  - Encoded passwords decode, and a raw `%` doesn't break parsing.
  - A password never lands in Host.
  - `mongodb+srv://` and `rediss://` get a clear "not supported yet" note.
  - When editing, a pasted URL never changes the engine.
- **Engine tiles:** 7 of them, for new connections only.
- **Environment:** a one-click segmented control tinted with the environment colours. Choosing Prod shows "commits ask for confirmation".
- **Colour:** six preset dots. A custom colour you already had survives as a seventh.
- **Server section:**
  - Host with a narrow Port beside it.
  - User and Password, with a show/hide eye.
  - Database, plus SSL mode for Postgres.
  - Redis and Mongo keep their own field variants.
  - SQLite and DuckDB show a File section with Browse… (the native dialog in the app).
- **Validation:** out-of-range ports flag immediately. An empty required field shows its error only after you've left it. Save and Connect stay disabled, with the reason in their tooltip.
- **SSH tunnel:** a switch that expands the tunnel fields. Auth is a "Password / agent | Key file" choice, with Browse… for the key and a passphrase field. The paragraph of fine print is now one line.
- **Footer**, pinned to the bottom:
  - Test ⌘T, with the result inline ("✓ Connected in 97 ms").
  - On failure, a one-line summary plus the full error in a box with Copy.
  - Discard when there are edits, Save ⌘S, and Connect ⌘↵.
- **Unsaved-edits safety:** closing the sheet with Esc, × or the backdrop, pressing ⌘N, or duplicating all ask "Discard unsaved changes?" when there are edits.
- **Password after Save:** it stays in the field, so Test still works.
- **No backend changes.**

10 commits: 10 files changed, +1636 / −462.

## Tests
- **vitest:** 769/769 pass.
- **New suites:**
  - `connUrl.test.ts`: every scheme, encoded and raw `%`, round-trips and validation.
  - `ConnectionSheet.test.tsx` and `ConnectionSheet.regression-1.test.tsx`.
  - `isDirty` cases in `ConnectionManager.test.ts`, covering the secret snapshot and normalised SSH/env.
- **tsc:** clean.

## Review
- **Task 1 (URL lib):** clean on the first review.
- **Task 2 (sheet):** one round. Two weak test assertions from the plan were tightened, plus accessibility fixes: error text linked to its field, ring-inset focus, and the Copy button kept in view.
- **Task 3 (wiring):** one round. The plan's "clear the password after Save" broke Save→Test, because the backend test has no keychain fallback. The password is now kept, and a snapshot tracks changes. The same round fixed ⌘↵ firing twice in the URL field, a delete with no error, copy feedback, reverted changes still reading dirty, and a failed connect leaving the sheet dirty.
- **Final whole-branch review (Opus): "with fixes".**
  - A raw `%` in a pasted URL threw an error, and a URL that failed to parse landed in Host with its password.
  - ⌘N replaced a sheet with unsaved edits without asking.
  - Six minors: the sheet's local state leaking between opens, pasting a URL changing the engine in edit mode, the unsupported `srv`/`rediss` schemes, Copy URL turning the fields green, Duplicate over unsaved edits, and no refresh after a connect-save.
  - All were fixed in one wave, and a scoped re-review confirmed all 8.

## QA (real IPC bridge, Docker Postgres)
Full flow through the real Rust commands:
- **Paste** `postgres://postgres:table%69q@127.0.0.1:55432/tableiq?sslmode=disable`: every field filled, and the password decoded to 7 characters.
- **Prod + ⌘T:** `test_connection` returned 200 in 90 ms, and the sheet shows "✓ Connected in 97 ms" with green fields.
- **⌘S:** `save_connection` 200, then `list_connections` 200. The sheet switches to edit mode. Testing again after Save passes, and the password is kept.
- **Port 70000:** an inline "1–65535"; Connect disabled with the title "Port: 1–65535".
- **Unsaved edits:** Esc and ⌘N both ask "Discard unsaved changes?". Discard restores the port.
- **Raw `%` URL pasted into Host:** the fields filled correctly and nothing raw landed in Host.
- **SSH switch:** expands the tunnel section.
- **⌘↵:** `save_connection` → `connect` → `list_objects`, all 200. The lobby opens with the PRODUCTION pill.
- **Dark mode**, the Redis variant and the SQLite variant.

| Issue | Severity | Status |
|---|---|---|
| ISSUE-001: a fresh SQLite form showed a red "Choose a database file" before any input | low (UX) | fixed (3891eac), regression test (dab818e) |

**Health: 97 → 100** (Visual, Functional, UX, Console, Content scored).

**Not verifiable in the browser build:** the native Browse… file and key pickers, and the native confirm dialog's button labels. The bridge falls back to `window.confirm`.

| | |
|---|---|
| New, empty ![](assets/connection-sheet/cs-new.jpg) | URL pasted ![](assets/connection-sheet/cs-pasted.jpg) |
| Prod + test passed ![](assets/connection-sheet/cs-tested.jpg) | Edit, invalid port, unsaved ![](assets/connection-sheet/cs-invalid.jpg) |
| SSH expanded ![](assets/connection-sheet/cs-ssh.jpg) | Connected, PRODUCTION pill ![](assets/connection-sheet/cs-connected.jpg) |
| Redis (dark) ![](assets/connection-sheet/cs-redis-dark.jpg) | SQLite, no premature error ![](assets/connection-sheet/issue-001-after.jpg) |

## Rulings (each with its cost if wrong)
1. Subagent-driven build for a 3-task plan, because the sheet is large UI work. Cost if wrong: extra review tokens.
2. The header engine icon stays the shared 32px circle. Cost if wrong: a small difference from the mockup.
3. Two weak test assertions from the plan were tightened. Cost if wrong: none.
4. The edit-mode password placeholder is "Keep saved password", because the longer text got cut off. Cost if wrong: slightly less explicit wording.
5. Engine tiles appear only for new connections. Cost if wrong: to change engine you duplicate the connection.
6. No Redis username field, since the backend ignores it. Cost if wrong: none today.
7. Typed secrets stay after Save, and dirtiness tracks changes since the save. Cost if wrong: secrets stay in memory while the sheet is open, as before.
8. Two accepted deviations: `translate-none` on the sheet wrapper and a capture-phase key listener. Each fixes a measured bug. Cost if wrong: none.
9. Parked: "Saved" still tints the fields green. Cost if wrong: it can look like a passed test.
10. Empty-field errors wait until the field has been visited (QA). Cost if wrong: the error appears later.

## Deferred (TODOS)
- The SSH port and host aren't validated. This predates the redesign.
- A pasted SQLite URL with `?query` folds the query into the path.
- The "Copied" confirmation has no `aria-live` announcement.
- Testing a saved connection with a blank password fails, because the backend has no keychain fallback. This predates the redesign.

## Creds gate
No secrets are in the diff. The QA connection used the local Docker test password (`tableiq`) in a temporary config directory (`/tmp/tiq-qa-conn`).
