# QA report: engine profiles (0.8.3), branch `RudreshNarwal/pending-items-select` @ 341a997

Report-only run, 2026-10-04. No tracked file was edited; `git status` is clean at the end.

## Rig

- Bridge: `src-tauri/target/debug/examples/ipc_bridge` run directly (no cargo), `TIQ_CONFIG_DIR=/tmp/tiq-qa-profiles-1`, port 1430. Started as PID 8194; killed and restarted once (PID 25745, same binary) for scenario J, because the bridge holds `connections.json` in memory and did not see the hand-edited file.
- Frontend: `VITE_IPC_BRIDGE=1 pnpm vite --host 127.0.0.1 --port 1420 --strictPort` (PIDs 8195/8197). Every command went to `/__ipc/<cmd>`; the mock was never used.
- Browser: gstack `browse`, state file pinned to `.gstack/browse.json` in this worktree, 1440x900 unless stated.
- Request and response bodies were captured with a `window.fetch` wrapper installed in the page (read-only logging).
- Databases: PostgreSQL 55432, MySQL 8.4.11 53306, MariaDB 11.8.9 53307, TiDB v8.5.7 54000, CockroachDB 56257. All containers were already healthy; none was started, stopped or changed.
- No keychain prompt appeared at any point; saves and connects with passwords returned in milliseconds.

All screenshot paths below are under `/Users/rudy/conductor/workspaces/Table-IQ/stuttgart/.context/qa/`.

## Verdicts

| | Scenario | Verdict |
|---|---|---|
| A | New-connection sheet | PASS |
| B | Picking a profile | PASS |
| C | Paste URL, CockroachDB | PASS, with the two known failures recorded and one wording deviation |
| D | Detection fills an empty profile | PASS |
| E | MariaDB is "real" | PASS |
| F | Genuine engines unchanged | PASS (SSL `require` connected over TLS) |
| G | No transactions | PASS, plus one backend finding from an extra probe |
| H | MCP settings | PASS, live MCP endpoint exercised |
| I | Search | PASS |
| J | Unknown profile id | PASS |
| K | Accessibility and design | PASS (report-only), three low-severity notes |

No scenario failed. Findings that are not scenario failures are listed in "Findings" below.

---

## A. New-connection sheet: PASS

- Seven tiles: Postgres, MySQL, SQL Server, SQLite, DuckDB, Redis, MongoDB. Tooltips: PostgreSQL, **MySQL / MariaDB**, SQL Server, SQLite, DuckDB, Redis, MongoDB.
- Native `<select>` with accessible name "Compatible database"; first option "Or a compatible database…" is disabled and selected.
- Group "PostgreSQL-compatible": CockroachDB, TimescaleDB, YugabyteDB, then AlloyDB (Beta), Amazon Redshift (Beta), Materialize (Beta), Neon (Beta), openGauss (Beta), QuestDB (Beta), RisingWave (Beta), Supabase (Beta).
- Group "MySQL-compatible": Dolt, GreatSQL, MariaDB, TiDB, then Apache Doris (Beta), Manticore Search (Beta), OceanBase (Beta), PlanetScale (Beta), PolarDB-X (Beta), SelectDB (Beta), StarRocks (Beta), Vitess (Beta).
- The seven verified products are exactly the `tested: true` ones and come first in each group. 23 products in total.
- Screenshot: `A-sheet-picker.png`.

## B. Picking a profile: PASS

- TiDB: port 4000, user `root`, header "TiDB" with the red "Ti" icon, no engine tile pressed, SSL mode select visible (`prefer`), capability line "Not available for TiDB: backup and restore, user management, agent access." Screenshot `B-tidb-picked.png`.
- Amazon Redshift: header "Amazon Redshift · Beta", line "Not available for Amazon Redshift: backup and restore, user management, agent access. Beta: not yet verified against a live server.", port 5439, database `dev`, SSL `require`. The `root` user preset from TiDB was cleared (Redshift has no user preset). Screenshot `B-redshift-beta.png`.
- Typed user `qa_user_typed`, then clicked the PostgreSQL tile: port 5432, database empty, SSL `prefer`, select back on the placeholder, Postgres tile pressed, user still `qa_user_typed`. Screenshot `B-back-to-postgres.png`.

## C. Paste URL, CockroachDB: PASS (known failures recorded)

- Synthetic paste of `cockroachdb://root@127.0.0.1:56257/defaultdb?sslmode=disable` into the URL field: the paste was prevented from landing as text, engine postgres, profile `cockroachdb`, port 56257, user root, database defaultdb, SSL disable, name prefilled "defaultdb". Screenshot `C-url-pasted.png`.
- Network: `test_connection` 200, `save_connection` 200 (saved with `"profile":"cockroachdb"`), `connect` 200, `list_objects` 200.
- Title bar "CockroachDB : qa3-crdb : defaultdb", lobby header purple "Cr" icon and "CockroachDB · 127.0.0.1:56257", status bar "Connected · CockroachDB". Screenshot `C-crdb-connected.png`.
- Rail: appears only with two or more open connections; it shows a generic database glyph for every engine and the product only in the tooltip ("qa3-tidb-plain · TiDB · test"). Screenshot `C-rail-two-open.png`.
- Created `qa3_t` from the query tab (`run_query` 200, 3 rows affected); the schema browser listed it; the grid showed 3 rows and the chip "editable". Screenshot `C-crdb-grid.png`.
- Unknown stats: `estimate_rows` returned `null` and `table_stats` returned `[]`, both 200. Nothing errors. **Deviation from the expected wording:** no "—" is rendered anywhere for these. The grid footer shows the exact count ("1–3 of 3 rows"), the Inspector shows "ROWS 3", and the lobby simply omits the on-disk and largest-table parts ("1 table · 0 views", "Stats · just now"). Screenshot `C-crdb-lobby-stats.png`.
- Staged `beta` to `beta2` in row 2, "1 unsaved change", Commit: `apply_changes` 200 `{"updated":1,"deleted":0,"inserted":0}`. Verified with an in-app `SELECT` and with `cockroach sql`: row 2 is `beta2`. Screenshots `C-crdb-staged.png`, `C-crdb-committed.png`, `C-crdb-verify-query.png`.
- Explain (known): `explain_query` 400, the user sees `EXPLAIN: at or near "json": syntax error`.
- Stop on `SELECT pg_sleep(20)` (known), exactly what the user sees:
  1. `cancel_query` 400 `"unknown function: pg_cancel_backend()"`.
  2. The result pane and the status bar show `unknown function: pg_cancel_backend()` in red; the Stop button reads "Stopping…" and Run all / Run in tx / Explain stay disabled. Screenshot `C-crdb-stop.png`.
  3. The query is not cancelled. It runs the full 20 s and `run_query` returns 200 with one row (`exec_ms` 20007).
  4. After it finishes the toolbar comes back, but the "Result 1" pane still shows the red cancel error instead of the returned row, and the status bar still shows the error next to "1 columns". Screenshot `C-crdb-stop-after.png`.
- Not offered: the More menu has only "New Table…", "Import CSV / JSON…", "Diff with another connection…" (no Backup or Restore); the command palette has no "Database User Management". Screenshots `C-crdb-more-menu.png`, `C-crdb-palette-user.png`.

## D. Detection fills an empty profile: PASS

- `qa3-tidb-plain` created with the plain MySQL tile at 54000, root, db `test`; first `save_connection` carried `"profile":null`.
- `connect` 200 returned `"profile":"tidb","caps":{"transactions":true,"backup":false,"user_mgmt":false}`.
- One extra `save_connection` 200 followed, with `"profile":"tidb"` and `"password":null,"sshSecret":null` (keeps whatever is stored). `connections.json` then had `profile: tidb`.
- Lobby "Ti" icon and "TiDB · 127.0.0.1:54000", title bar "TiDB : qa3-tidb-plain : test". Screenshot `D-tidb-connected.png`.
- Created `qa3_t`, staged and committed `beta2`: `apply_changes` 200 `{"updated":1,…}`, verified with the mysql CLI against TiDB. Screenshots `D-tidb-grid.png`, `D-tidb-staged.png`.
- More menu has no Backup or Restore; palette has no user management.
- After disconnect the list row's accessible name is "TiDB qa3-tidb-plain (local) 127.0.0.1:54000/test" with the red Ti icon; the edit sheet's select shows TiDB (first option "Plain MySQL") with both notes. Screenshots `D-list-after-detect.png`, `D-tidb-edit-sheet.png`.
- Reconnect: `connect` 200, no second detection save.
- TiDB has no password, so the keychain half of this check was done on MariaDB (scenario E): after the detection save with `password:null`, disconnect and reconnect from the list succeeded with the stored password.

## E. MariaDB is "real": PASS

- `qa3-maria-plain`, MySQL tile, 53307, root / tableiq. `connect` 200 returned `"profile":"mariadb","caps":{"transactions":true,"backup":true,"user_mgmt":false}`, then the detection `save_connection` 200 with `"profile":"mariadb","password":null`.
- Title bar and status bar read MariaDB, icon "Ma". Screenshot `E-maria-connected.png`.
- More menu includes "Backup Database…" and "Restore Database…". User management is not offered (the profile table has `userMgmt: false` for MariaDB).
- Grid edit and commit: `apply_changes` 200, verified with the mariadb CLI. Screenshot `E-maria-committed.png`.
- Reconnect from the list after the detection save: `connect` 200 (stored password intact).

## F. Genuine engines unchanged: PASS

PostgreSQL (`qa3-pg`):
- `connect` 200 `"profile":null,"caps":{"transactions":true,"backup":true,"user_mgmt":true}`; no detection save.
- Label "PostgreSQL", icon Pg. More menu has Backup and Restore; palette has "Database User Management"; query tab has "Run in tx".
- Stop on `SELECT pg_sleep(20)`: `cancel_query` 200, `run_query` 400 "Query cancelled", toolbar back at once. Screenshot `F-pg-stop.png`.
- Grid edit and commit: `apply_changes` 200, verified with psql. Screenshots `F-pg-grid.png`, `F-pg-committed.png`.
- Edit sheet: select shows "Plain PostgreSQL" first and selected, with the note "TableIQ checks the server on connect and fills this in if the server identifies itself." Screenshot `F-pg-edit-sheet.png`.

MySQL (`qa3-mysql`):
- `connect` 200 `"profile":null`, all three caps true; label "MySQL"; Backup, Restore, user management and "Run in tx" present.
- Stop on `SELECT SLEEP(20)`: `cancel_query` 200; the statement returned after 1558 ms as a normal one-row result (MySQL's interrupted SLEEP returns 1), so the user sees "1 row · 1558 ms" rather than "Query cancelled". Screenshot `F-mysql-stop.png`.
- Grid edit and commit: `apply_changes` 200, verified with the mysql CLI. Screenshot `F-mysql-committed.png`.
- Edit sheet: "Plain MySQL" first and selected, the detection note, and an SSL mode select. Screenshot `F-mysql-edit-sheet.png`.
- **SSL mode `require` against the local MySQL: succeeds over TLS.** `test_connection` 200 ("✓ Connected in 29 ms"), `connect` 200, the title bar gains an "SSL" segment ("MySQL : SSL : qa3-mysql : tableiq"), and `SHOW SESSION STATUS LIKE 'Ssl_cipher'` in the query tab returned `TLS_AES_256_GCM_SHA384`. The hint text was therefore not exercised. Screenshots `F-mysql-ssl-require-test.png`, `F-mysql-ssl-require-connected.png`.

## G. No transactions: PASS

- Sheet with "Apache Doris (Beta)": line "Not available for Apache Doris: grid editing, backup and restore, user management, agent access. Beta: not yet verified against a live server." Screenshot `G-notx-sheet.png`.
- `qa3-notx` pointed at MySQL 53306. `connect` 200 `"profile":"doris","caps":{"transactions":false,"backup":false,"user_mgmt":false}`; labels read "Apache Doris".
- Table grid: chip `read-only · no transactions`, hint "Read-only: Apache Doris has no multi-statement transactions. Run INSERT or UPDATE from a query tab." Screenshot `G-notx-grid.png`.
- Cell edit: select plus Enter opens no editor (focus stays on the canvas), typing stages nothing, Discard and Commit stay disabled.
- "+ Row": the button is rendered but disabled (not removed).
- Delete and Backspace on a selected cell: nothing staged.
- Context menu: "Add row", "Duplicate row", "Set value" and "Delete row" are disabled; Copy, Filter and "Copy DELETE statement" remain. There is no Paste item in this menu. Screenshot `G-notx-context-menu.png`.
- Paste: a synthetic paste event staged nothing. A real clipboard read is denied in the headless browser (`NotAllowedError … Read permission denied`), so a true ⌘V could not be driven; that part is a rig limit.
- Inspector: row fields render as static values, no input or textarea other than the field search box. Screenshot `G-notx-cell-selected.png`.
- Query tab: Run, Run all, Explain; no "Run in tx". Plain `UPDATE qa3_t SET name='gamma2' WHERE id=3`: `run_query` 200, 1 row affected, verified with the CLI. Screenshot `G-notx-update-from-query.png`.
- Forged staged commit, same request the frontend sends (`POST /__ipc/apply_changes` with `{id, schema, table, changes:[{kind:"update", edit:{…}}]}`): **400** `"Apache Doris has no multi-statement transactions, so staged changes can't be committed together. Run the statements from a query tab."` Data unchanged.
- Extra probe, see Finding 1: a forged `apply_edit` on the same session returned 200 and wrote the row.

## H. MCP settings: PASS

- Rows on first open: `qa3-crdb`, `qa3-tidb-plain`, `qa3-notx` have an unchecked, disabled checkbox, no "Allow writes" checkbox, and the note "Not available over MCP yet: TableIQ's agent guard has been reviewed for PostgreSQL, MySQL and MariaDB only." `qa3-maria-plain`, `qa3-pg`, `qa3-mysql` are enabled. Screenshot `H-mcp-settings.png`.
- Sharing `qa3-pg`: `mcp_set_config` 200 with `connections:[{id: <pg>, writes:false}]`; the row is checked and gains "Allow writes". Ticking it turns the row amber with "Writes allowed". Screenshots `H-mcp-pg-shared.png`, `H-mcp-pg-writes.png`.
- Never amber: I wrote a config by hand that listed the TiDB, CockroachDB and Doris connections with `writes:true`. On reopening, those rows were still unchecked, disabled, without a writes checkbox and with a transparent background; only `qa3-pg` was amber. Screenshot `H-mcp-forged-config.png`.
- Live MCP server (enabled in the panel, `http://127.0.0.1:7431/mcp`, served by the bridge process):
  - No token: HTTP 401.
  - `initialize` OK (`serverInfo` tableiq 0.8.2); tools: `describe_table`, `list_connections`, `list_databases`, `list_tables`, `run_query`.
  - `list_connections`: only `qa3-pg`.
  - `list_tables` and `run_query` against `qa3-tidb-plain` (not shared): "Connection `…` is not shared over MCP. The user can allow it in TableIQ → Settings → MCP." (`isError:true`).
  - `run_query` on `qa3-pg`: `{"columns":["count"],"rows":[["3"]],"truncated":false}`.
  - With the hand-written config: `list_connections` still returned only `qa3-pg`; every tool on the three returned "TiDB / CockroachDB / Apache Doris cannot be shared over MCP yet: TableIQ's agent guard has been reviewed for PostgreSQL, MySQL and MariaDB only. Ask the user to run this in a TableIQ query tab."
  - A connection saved as plain MySQL, pointing at TiDB, never connected in the UI and shared through the panel (`qa3-tidb-fresh`): `run_query` and `list_tables` were refused with the TiDB message, so the MCP path detects on its own connect. Screenshot `H-mcp-fresh-tidb-shared.png`.
  - The same connection relabelled `profile: mariadb`: MCP still refused it as TiDB. A UI-path `connect` returned `"profile":"mariadb","caps":{"transactions":true,"backup":false,"user_mgmt":false}` (saved label wins, capabilities are the more restrictive).

## I. Search: PASS

- Welcome list search: "tidb" finds qa3-tidb-plain; "cockroach" finds qa3-crdb; "maria" finds qa3-maria-plain; "doris" finds qa3-notx; "postgres" finds qa3-pg and qa3-crdb; "mysql" finds the four MySQL-family rows. Screenshot `I-list-search-tidb.png`.
- ⌘O picker (also opened with the real ⌘O key): same results, rows read "TiDB · 127.0.0.1:54000/test" and so on. Screenshots `I-picker-open.png`, `I-picker-tidb.png`.
- "beta" matches nothing in the picker (the Beta flag is not a search term).

## J. Unknown profile id: PASS

- Set `qa3-mysql`'s `profile` to `"from-the-future"` in `/tmp/tiq-qa-profiles-1/connections.json`, restarted the bridge, reloaded.
- It loads: `list_connections` returns the id unchanged; the row draws the plain MySQL icon and label. Screenshot `J-list-unknown-profile.png`.
- Edit sheet: the select has "from-the-future (unknown to this version)" selected, after "Plain MySQL"; capability line "Not available for from-the-future: agent access." Screenshot `J-sheet-unknown-profile.png`.
- Save keeps the id: `save_connection` 200 with `"profile":"from-the-future"`, and the file still has it after Save and after Connect.
- Connect: `connect` 200 `"profile":null`, all caps true; title "MySQL : SSL : qa3-mysql"; Backup, Restore, user management and "Run in tx" are present. Screenshot `J-connected-unknown-profile.png`.
- MCP row: unchecked, disabled, note "Not available over MCP: this connection's profile (from-the-future) was saved by a newer TableIQ. Update TableIQ to manage it." Screenshot `J-mcp-unknown-profile.png`.
- A hand-written MCP share for it is refused by the server: "This connection cannot be shared over MCP: its profile (from-the-future) was saved by a newer TableIQ. Update TableIQ to manage it."
- Restored by deleting the connection in cleanup.

## K. Accessibility and design: PASS (report-only)

- Keyboard: Tab order is URL field, the seven tiles, the "Compatible database" select, Name, Environment. The select is a native control with an accessible name and `aria-describedby` on the capability line (and the detection note in edit mode).
- Icons: in the welcome list the icon is `role="img"` with `aria-label` set to the product ("TiDB qa3-tidb-plain (local) …" is the row's accessible name). In the sheet header and on the tiles the icon is `aria-hidden` with a `title`, and the label is in adjacent text. This matches DESIGN.md "Engine identifier".
- MCP rows: the disabled checkbox carries `aria-describedby` pointing at its note.
- Contrast, measured: capability line and MCP note 7.73:1 in light and 7.44:1 in dark. White monogram on every one of the 23 profile fills is at least 4.60:1. Fill against the dark surface is at least 2.22:1, which meets DESIGN.md's 2:1 rule; MariaDB (2.24), Manticore (2.22) and PlanetScale (2.78) are under 3:1.
- 1100x700: the sheet body scrolls, no horizontal overflow, tiles do not clip, the capability line wraps to two lines; the MCP notes wrap inside the dialog. Screenshots `K-sheet-1100x700-dark-doris.png`, `K-sheet-1100x700-light-redshift.png`, `K-mcp-1100x700-dark.png`, `K-mcp-dark.png`.
- Against DESIGN.md "Connection sheet": header, tile row, select, ring on the select in place of the tile, capability line, edit-mode "Plain …" option and note all match.
- Notes: Findings 5, 6 and 7.

---

## Findings (none is a scenario failure)

Feature-related:

1. **`apply_edit` is not gated on a no-transactions session (backend).** On `qa3-notx` (caps `transactions:false`), `POST /__ipc/apply_edit {id, schema:"tableiq", table:"qa3_t", edit:{pk:[["id","1"]], column:"name", new_value:"FORGED1", orig_value:"alpha"}}` returned 200 `{"rows_affected":1}` and the row changed. Only `apply_changes` calls `refuse_no_transactions` (`src-tauri/src/ipc/connect.rs:999`). `insert_row` and `delete_row` look ungated too by reading; I did not call them. The UI gate holds everywhere I looked (grid, context menu, Inspector), so this is reachable only by a forged call, and a single statement may be intended to pass. Product behaviour, low severity; worth a decision.
2. **Stale MCP share entries cannot be removed from the panel.** Entries for a non-shareable connection stay in the MCP config and are written back on every other toggle (the `mcp_set_config` sent when I ticked "Allow writes" on `qa3-pg` still carried the three hand-written ids with `writes:true`). The row is unchecked and disabled, so the user cannot clear it. The server refuses those ids today, so nothing leaks; the risk is a connection shared with writes while it was plain MySQL, later detected as TiDB, and silently shared again if that product becomes shareable. Likely file: `src/components/McpSettings.tsx`. Low severity.
3. **MCP advertises a connection it will refuse.** For a connection saved as plain MySQL that is really TiDB and has not been connected in the UI, `list_connections` over MCP lists it and the panel shows it checked and enabled, while every tool call is refused as TiDB. The MCP-path detection is not saved back to the connection. Low severity, behaviour is safe.
4. **CockroachDB Stop leaves a wrong end state (known cause).** Beyond the known `pg_cancel_backend` error: the button sits on "Stopping…" until the query ends by itself, and afterwards the result pane shows the cancel error instead of the row the query returned. Product behaviour. Likely `src/components/QueryView.tsx` plus the Postgres driver's cancel.
5. **Focus is not distinguishable on the select once a product is picked.** Picked and unfocused: primary border and a 3px ring at 15% alpha. Picked and focused: primary border and a 2px ring at 25% alpha. The focused state is the smaller ring. With nothing picked, focus is clearly visible. `src/components/ConnectionSheet.tsx`, the select's class at line 345. Screenshot `K-sheet-select-focused-picked-dark.png`. Low severity (WCAG 2.4.7).
6. **Dark theme: disabled and enabled unchecked MCP checkboxes look alike.** The note under the row carries the meaning, and the row name is not dimmed. Screenshot `K-mcp-dark.png`. Cosmetic.
7. **Several profile fills are near-identical blues** (OceanBase, SelectDB, GreatSQL, AlloyDB, openGauss, RisingWave, Doris). Only the monogram tells them apart. Cosmetic.
8. **Wording deviations from the test plan, no error involved:** unknown row estimate and table size do not read "—" (they are omitted); "+ Row" and the edit items of the context menu are disabled, not removed.
9. **Version still reads 0.8.2** in the welcome sidebar, `package.json` and the MCP `serverInfo`, while `CHANGELOG.md` has "0.8.3 — unreleased".

Present on `main` too, outside this feature (checked with `git show main:…`):

10. Test in the edit sheet sends an empty password when the field shows "Keep saved password": `test_connection` 400 "Access denied for user 'root'@'…' (using password: NO)". `src/components/ConnectionManager.tsx:290`.
11. The welcome screen's "+" (New connection) button sits partly under the `tiq-titlebar-drag` strip (`absolute inset-x-0 top-0 h-9`, `ConnectionManager.tsx:431`); `elementFromPoint` at the button's centre returns the strip and a headless click on it times out. "Create Connection" works.
12. An empty MySQL-family database lists a schema literally named "database" (`src-tauri/src/drivers/mysql.rs:649`); seen on TiDB's empty `test` and MariaDB's empty `tableiq` before the scratch table existed (`D-tidb-connected.png`, `E-maria-connected.png`: "database · 0 tables").

Not compared with `main`:

13. The Inspector's row detail keeps the pre-edit value once a grid edit is staged and still after it is committed (seen on CockroachDB, TiDB, MariaDB, PostgreSQL and MySQL: grid `beta2`, Inspector `beta`; `C-crdb-staged.png`, `F-pg-committed.png`).
14. First open of the MCP panel showed a red "Platform secure storage failure: The specified item already exists in the keychain." Two `mcp_token` calls ran at once (one 200, one 400) while the token was being created. The app mounts under `React.StrictMode`, so this is probably a dev-mode double effect; a second open was clean. Screenshot `H-mcp-settings.png`.

QA-rig artefacts, not product bugs:

- Every table open logs one `get_table_window` 400 "Query cancelled" followed by a 200 for the same request key (double effect in dev).
- Console `NotAllowedError … Clipboard read` in the headless browser.
- After the bridge restart the MCP server was not running although the config said enabled; the bridge only starts it on `mcp_set_config`.
- The bridge caches `connections.json`, so scenario J needed a bridge restart.

## Cleanup

- `qa3_t` dropped on all five engines through the app's query tab (`run_query` 200 each) and confirmed absent with each database's CLI. No other table was touched.
- All seven `qa3-` connections deleted in the app (`delete_connection` 200 each); `connections.json` is `{"connections": []}`; their keychain entries are gone (checked by attribute lookup, no secret read).
- MCP config in the scratch dir reset to disabled with no connections; port 7431 closed.
- Stopped only what I started: bridge PID 8194 (earlier) and 25745, vite PIDs 8195 and 8197, and the browse daemon. Ports 1420, 1430 and 7431 are free.
- Docker containers untouched and still up.
- Theme toggle returned to "auto".
- Left behind:
  - One keychain item created by opening the MCP panel: service `ai.rhobots.tableiq`, account `mcp:token:1a2f5476` (the MCP token scoped to this scratch config dir; the real app's token uses a different account). The app has no way to delete it and I did not remove it with the `security` CLI.
  - `/tmp/tiq-qa-profiles-1/` (scratch config, logs, helper scripts; the copy of the MCP token I used for curl was deleted).
  - `.gstack/` in the worktree (browser state, ignored by its own `.gitignore`) and `.context/qa/` (64 screenshots and this report, ignored).
