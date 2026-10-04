# QA re-check 2: engine profiles fix round, branch `RudreshNarwal/pending-items-select` @ 50b124c

Report-only, 2026-10-04. No tracked file edited, no git state changed (`git status` clean).

## Rig

- Bridge `src-tauri/target/debug/examples/ipc_bridge` (prebuilt, no cargo), `TIQ_CONFIG_DIR=/tmp/tiq-qa-profiles-2`, port 1430 (PID 24706). Vite with `VITE_IPC_BRIDGE=1` on 1420 (pnpm 24707, node 24709). Ports were free at start. The mock was never used; every command went to `/__ipc/<cmd>`.
- Browser: gstack browse pinned to this worktree's `.gstack/browse.json`, 1440x900. A `window.fetch` wrapper (`/tmp/tiq-qa-profiles-2/hook.js`) logged request and response bodies, read-only.
- Databases: PostgreSQL 55432, MySQL 53306, MariaDB 53307, TiDB 54000, CockroachDB 56257, all already up and untouched apart from the `qa4_t` scratch table on PostgreSQL (created, then dropped).
- No keychain prompt appeared; saves and connects returned in milliseconds.
- Technique notes reused from the first report: MCP config lives at `<config dir>/mcp.json` (`{enabled, port, row_cap, connections:[{id, writes}]}`; the file does not exist until the first `mcp_set_config`); paste is a synthetic `ClipboardEvent` on the URL field; the grid is driven with PointerEvent/MouseEvent dispatch at canvas coordinates; Settings opens with Meta+, then the "MCP" button.

Screenshots are under `/Users/rudy/conductor/workspaces/Table-IQ/stuttgart/.context/qa2/`.

## Verdicts

| # | Check | Verdict |
|---|---|---|
| 1 | Focus on a picked profile select | PASS |
| 2 | Disabled MCP checkbox look | PASS |
| 3 | Stale MCP share pruned | PASS (pruned on panel load) |
| 4 | `sslmode=allow` on paste | PASS |
| 5 | Detection still fills an empty profile | PASS |
| 6 | Genuine PostgreSQL CSV import | PASS |
| 7 | Bundle import normalises profiles | PASS |
| 8 | Smoke regression | PASS |

No FAILs.

## 1. Focus on a picked profile select: PASS

The select is `select[aria-label="Compatible database"]`; focus was reached with a real Tab keypress from the MongoDB tile (`:focus-visible` true). Computed values:

| State | theme | border-color | box-shadow (the ring) | outline |
|---|---|---|---|---|
| nothing picked, unfocused | light | rgb(228,228,231) | none | none |
| nothing picked, focused | light | rgb(228,228,231) | 2px ring, primary at 25% alpha | none |
| nothing picked, focused | dark | rgb(88,91,207) | 2px ring, primary at 25% alpha | none |
| TiDB picked, unfocused | light | rgb(79,70,229) | 3px ring, primary at 15% alpha (`oklab(0.5106 0.0279 -0.2283 / 0.15) 0 0 0 3px`) | none |
| TiDB picked, focused | light | rgb(79,70,229) | 3px ring, primary at 40% alpha (`oklab(0.5106 0.0279 -0.2283 / 0.4) 0 0 0 3px`) | none |
| TiDB picked, unfocused | dark | rgb(99,102,241) | 3px ring, primary at 15% alpha (`oklab(0.5854 0.0253 -0.2025 / 0.15) 0 0 0 3px`) | none |
| TiDB picked, focused | dark | rgb(99,102,241) | 3px ring, primary at 40% alpha (`oklab(0.5854 0.0253 -0.2025 / 0.4) 0 0 0 3px`) | none |

Focused is the stronger state in both themes (same 3px width, alpha 0.40 against 0.15), which fixes the earlier finding where the focused ring was the smaller one. With nothing picked, focus is clearly visible in both themes.

Screenshots: `1-light-picked-unfocused.png`, `1-light-picked-focused.png`, `1-dark-picked-unfocused.png`, `1-dark-picked-focused.png`, `1-light-nothing-picked-focused.png`, `1-dark-nothing-picked-focused.png`.

Observation, not a failure: in the "picked, unfocused" shots the MongoDB tile holds focus (the Tab started there), so the tile ring is also visible. Dark unfocused ring at 15% alpha is faint but it is the unfocused state by design.

## 2. Disabled MCP checkbox look: PASS

Settings, MCP, dark theme, connections `qa4-tidb` (TiDB profile) and `qa4-pg` (plain PostgreSQL):

| Row | disabled | checked | opacity | cursor | aria-describedby |
|---|---|---|---|---|---|
| qa4-tidb | true | false | 0.5 | not-allowed | `mcp-note-<id>` |
| qa4-pg | false | false | 1 | default | none |

Class is `disabled:cursor-not-allowed disabled:opacity-50`. The disabled box is visibly dimmer next to the enabled one. The note "Not available over MCP yet: TableIQ's agent guard has been reviewed for PostgreSQL, MySQL and MariaDB only." is shown under the `qa4-tidb` row. Screenshot: `2-mcp-dark.png` (also `7-mcp-rows.png`, which shows four disabled rows and two enabled ones).

## 3. Stale MCP share pruned: PASS

Timeline, with the panel closed and the page reloaded each time:

1. Hand-wrote `/tmp/tiq-qa-profiles-2/mcp.json` with `{"id": "<qa4-tidb id>", "writes": true}`.
2. Reload, Settings, MCP. `mcp_get_config` returned the stale entry (`connections:[{d366dc1d…, writes:true}]`). **Straight afterwards, with no click, `mcp_set_config` was sent with `connections:[]` (twice, StrictMode double effect), and the file on disk had `connections: []`.** So the entry disappears on panel load, not on the next change. The `qa4-tidb` row stayed unchecked and disabled. Screenshot `3-mcp-stale-opened.png`.
3. Ticked `qa4-pg`: `mcp_set_config` payload `connections:[{f8487b59…, writes:false}]`; the file matched.
4. Shareable entries survive: wrote a config with the stale `qa4-tidb` (writes true) AND `qa4-pg` (writes true), reloaded and opened the panel. `mcp_set_config` payload and the file then held only `[{f8487b59…, writes:true}]`; the `qa4-pg` row was checked, amber, "Writes allowed" ticked. `qa4-tidb` removed, `qa4-pg` kept. Screenshot `3-mcp-reopen-pg-writes.png`.

Observation, not part of the check: deleting a connection in the app does not remove its MCP share. After I deleted `qa4-pg` the file still held its id with `writes:true` (I reset the file by hand in cleanup). The server refuses unknown ids, so nothing leaks, and the panel prunes non-shareable ids only for connections that still exist, so an orphan stays. Low severity; probably `src/components/McpSettings.tsx` or the delete path.

## 4. `sslmode=allow` on paste: PASS

Synthetic paste into the URL field of a new-connection sheet (default prevented each time), reading the SSL mode select afterwards:

| Pasted | SSL mode shows |
|---|---|
| `postgres://u@db.example.com/app?sslmode=allow` | `allow` (not blank, not `require`) |
| `postgres://u@mydb.abc.eu-west-1.rds.amazonaws.com/app?sslmode=allow` | `allow` |
| `postgres://u@mydb.abc.eu-west-1.rds.amazonaws.com/app` | `require` (unchanged behaviour) |
| `mysql://u@h/db?ssl-mode=REQUIRED` | `require`, MySQL tile |
| extra: RDS host with `?sslmode=prefer` | `prefer` |

Screenshots: `4-paste-allow.png`, `4-paste-rds-allow.png`, `4-paste-rds-nossl.png`, `4-paste-mysql-required.png`.

Minor, outside the check: on repeated pastes the Name field keeps the first prefilled value (`app`) instead of following the newest URL. Probably intended (it only prefills an empty name).

## 5. Detection still fills an empty profile: PASS

- `qa4-tidb-plain`, MySQL tile, 54000, root, db `test`. First `save_connection` carried `"profile":null`.
- Sheet Connect: `save_connection` (profile null), then `connect` 200 returned `"profile":"tidb","caps":{"transactions":true,"backup":false,"user_mgmt":false}`, then exactly one more `save_connection` with `"profile":"tidb"`, `password:null`, `sshSecret:null`.
- After Disconnect the list row's `aria-label` is "TiDB" with the red Ti icon; the edit sheet's select shows TiDB. Screenshots `5-tidb-plain-connected.png`, `5-after-disconnect.png`.
- Edited the NAME to `qa4-tidb-renamed` in the sheet and saved: `save_connection` carried the new name and `"profile":"tidb"`. Reconnect (sheet Connect): `save_connection` with the new name, then `connect` returned name `qa4-tidb-renamed`, profile `tidb`, and **no second detection save**. `connections.json` holds `qa4-tidb-renamed / tidb`. The name survives. Screenshot `5-renamed-reconnected.png`.

## 6. Genuine PostgreSQL import: PASS

- `qa4-pg`: `CREATE TABLE public.qa4_t (id INT PRIMARY KEY, name VARCHAR(20))` via the query tab, `run_query` 200.
- File `/tmp/tiq-qa-profiles-2/qa4_t.csv`: `id,name` / `1,alpha` / `2,beta` / `4,O'Br\ien`.
- Driven through the real UI: More, "Import CSV / JSON…", `B upload` onto the dialog's `<input type=file>`, Into table `public.qa4_t`, columns auto-mapped, Import. Requests: `parse_import` 200, then `import_rows` 200 with rows `[["1","alpha"],["2","beta"],["4","O'Br\\ien"]]` returning `{"inserted":3}`; the dialog said "Imported 3 rows".
- Query `SELECT id,name,length(name),encode(convert_to(name,'UTF8'),'hex')` returned row 4 with length 8 and hex `4f2742725c69656e`, which equals the bytes of `O'Br\ien` (checked with `xxd`). Byte-for-byte. Screenshots `6-import-done.png`, `6-verify-query.png`.

## 7. Bundle import normalises profiles: PASS

- "Export Bundle" downloads `tableiq-connections.json` (no passphrase). A headless download is not capturable here, so I called the same backend command (`export_connections`) through the vite proxy to learn the format: a JSON array of connection configs with `id,name,color,engine,host,port,database,username,sslmode,ssh,env,profile`.
- Wrote `/tmp/tiq-qa-profiles-2/bundle.json` with `qa4-b-empty` (`"profile":""`) and `qa4-b-wrong` (`"profile":"cockroachdb"` on engine `mysql`), both to MySQL 53306.
- Imported with the welcome screen's real "Import Bundle" file input: `import_connections` 200 returned `2`.
- `list_connections` returns `profile: null` for both; `connections.json` has `"profile": null` for both. In Settings, MCP the rows read "MySQL · tableiq" (not CockroachDB), and both checkboxes are enabled (opacity 1, no unavailable note). Screenshot `7-mcp-rows.png`.

## 8. Smoke regression: PASS

- `qa4-pg`: connect 200 `"profile":null`, all three caps true, no detection save. Opened `qa4_t` (chip "editable"), staged `beta` to `beta2`, "1 unsaved change", Commit: `apply_changes` 200 `{"updated":1,"deleted":0,"inserted":0}`. A `SELECT` in the query tab returns `beta2`. Screenshots `8-pg-staged.png`, `8-pg-committed.png`, `8-pg-verify.png`.
- `qa4-crdb` created by pasting `cockroachdb://root@127.0.0.1:56257/defaultdb?sslmode=disable`: tile blank, profile select shows CockroachDB, port 56257, user root, db defaultdb, SSL `disable`; `save_connection` carried `"profile":"cockroachdb"`. Connect 200 `"profile":"cockroachdb","caps":{"transactions":true,"backup":false,"user_mgmt":false}`; title bar and status bar read CockroachDB; `list_objects` 200 returned the schemas (system schemas present; `public` is empty so the sidebar says "No objects"), `table_stats` and `foreign_keys` 200. Screenshot `8-crdb-connected.png`.
- Console across the whole run: no JS errors. One `[error] Failed to load resource … 400`, which is the `get_table_window` 400 ("Query cancelled", 17 B) that every table open produces before the real 200, the dev double-effect artefact already recorded in the first report. It is the only non-2xx request in the 640 logged.

Rig mistake, mine, no product impact: while verifying the commit I ran my `setsql` helper while the editor click had timed out, so Cmd+A, Backspace and typed text landed in the table grid and staged "6 unsaved changes". I pressed Discard; no `apply_changes` was sent, and a later query confirmed the table was intact (`alpha`, `beta2`, `O'Br\ien`). The screenshot `8-pg-verify.png` was re-taken after that.

## Other observations (not failures)

- The first browse daemon started with the shell cwd in `/tmp/tiq-qa-profiles-2`, which made `screenshot` refuse the worktree path; I stopped that daemon (PID 25543, mine) and restarted from the worktree.
- The MCP panel fires `mcp_token` and `mcp_get_config` twice per open (StrictMode double effect); no keychain "already exists" error appeared this time.
- The inspector keeps the pre-edit value (`beta`) after a committed grid edit, already recorded as Finding 13 in the first report.

## Cleanup

- `DROP TABLE public.qa4_t` on PostgreSQL, run in-app (`run_query` 200). Confirmed absent in PostgreSQL, MySQL and MariaDB via `information_schema` counts of 0. No `qa4_t` was ever created on TiDB or CockroachDB.
- All six `qa4-` connections deleted in the app (`qa4-tidb-renamed`, `qa4-tidb`, `qa4-pg`, `qa4-crdb`, `qa4-b-empty`, `qa4-b-wrong`); `connections.json` is `{"connections": []}`; no keychain account matching their ids remains.
- MCP config reset to disabled with no connections.
- Stopped only my processes: bridge 24706, vite 24707 and 24709, browse daemon 27194 (and the earlier 25543). Ports 1420, 1430 and 7431 are free. Docker untouched (59 containers up).
- Left behind:
  - One keychain item from opening the MCP panel: service `ai.rhobots.tableiq`, account `mcp:token:<hash of /tmp/tiq-qa-profiles-2>`. Three `mcp:token:*` accounts exist now (`1a2f5476` from the first run, `1b54cdb5`, `192f52e3`); I did not read secrets and did not delete any, so I cannot say which of the latter two is mine.
  - `/tmp/tiq-qa-profiles-2/` (scratch config, logs, helper scripts, the CSV and bundle files).
  - `.gstack/` (browser state) and `.context/qa2/` (screenshots and this report), both ignored by git.
