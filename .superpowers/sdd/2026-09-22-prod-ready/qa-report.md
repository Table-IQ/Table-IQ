# Prod-ready QA — consolidated end-to-end pass

**Date:** 2026-09-24
**Branch:** `RudreshNarwal/fix-sql-tab-and-ddl-arrows` (from 777a59a)
**Harness:** real Rust backend via `ipc_bridge` (`TIQ_CONFIG_DIR=/tmp/tiq-qa`) + `VITE_IPC_BRIDGE=1 pnpm dev` on :1420, driven with the gstack `browse` CLI.
**Databases:** live Postgres :55432, MySQL 8.4 :53306, SQL Server :51433 — all seeded from `docker/*/qa.sql`.

Every request below was confirmed on the wire. The page is instrumented with a
`fetch` recorder so the *exact* JSON sent to `/__ipc/<cmd>` and the response are
captured per action — a green screen with a 4xx behind it cannot hide.

Connection ids used: `qa-pg`, `qa-mysql`, `qa-mssql` (deleted at the end of the run).

---

## 1. "SQL Query tab is not working fine as per TablePlus"

### Postgres (`qa-pg`)

| # | Check | Result | Evidence |
|---|---|---|---|
| 1.1 | ⌘↵ with cursor immediately after a mid-script `;` runs that statement | **PASS** | sent `["SELECT 1 AS a"]`; doc unchanged (no blank line inserted) |
| 1.2 | ⌘↵ with cursor after the *final* `;` (the old "found nothing" bug) | **PASS** | sent `["SELECT 2 AS b"]` |
| 1.3 | ⇧⌘↵ on a mixed script → per-statement result tabs | **PASS** | `Result 1`, `Result 2`, `Messages` tabs rendered |
| 1.4 | rows-affected messages per statement | **PASS** | `UPDATE … → 5 rows affected`, `CREATE TABLE → 0 rows affected` |
| 1.5 | inline error with a doc line number | **PASS** | banner `ERROR syntax error at or near "SELCT" … Line 5` |
| 1.6 | clicking the error jumps the caret to that line | **PASS** | `.cm-activeLine` = `SELCT * FROM orders;`, gutter = `5` |
| 1.7 | execution stops at the first failure | **PASS** | 5-statement script produced 4 metas, last one the error |
| 1.8 | duplicate-column join shows correct values under `id` / `id (2)` | **PASS** | app `300/100, 299/99, 298/98, 297/97` == `psql` output exactly |
| 1.9 | NULL renders differently from `''` | **PASS** | NULL = italic grey `NULL`; `''` = empty cell (see `qa-dupcol` shots) |
| 1.10 | SQL + results survive switching tabs | **PASS** | SQL byte-identical after switching to a table tab and back; `Result 1` still mounted |
| 1.11 | SQL survives a full page reload | **PASS** | persisted to `tiq:tabs:qa-pg`; restored verbatim on reconnect |
| 1.12 | Cancel on a long query | **PASS** | `pg_sleep(30)` → Stop → `cancel_query` 200, `run_query` 400 `"Query cancelled"`, status "Query cancelled", Run button returns |
| 1.13 | BEGIN without COMMIT rolls back **and says so** | **PASS** | notice `Transaction was not committed and has been rolled back`; `psql` confirms `fresh.id=1` still `v1` |
| 1.14 | "Run in tx" with a failing statement rolls back | **PASS** | notice shown; `psql` confirms `fresh.id=2` still `v2` (the `UPDATE` reported "1 row affected" but did not persist) |

Console errors across the whole flow: `[]`. Network: only `run_query` /
`get_result_window` / `cancel_query`, all 200 except the deliberate 400s.

**Note (not a bug):** result *sets* do not survive a page reload — only the SQL
does. Result sets live in backend memory keyed by `result_id` and the tab state
persists no `result_id`. TablePlus behaves the same way after a restart.

### MySQL (`qa-mysql`)

| # | Check | Result | Evidence |
|---|---|---|---|
| 1.15 | ⇧⌘↵ on a mixed script | **PASS** | 4 statements sent individually, backticks preserved |
| 1.16 | rows-affected per statement | **PASS** | `UPDATE … → 3 rows affected` |
| 1.17 | inline error with a doc line number | **PASS** | MySQL says "at line 1" (statement-relative); UI correctly shows **Line 4** (doc-relative) |
| 1.18 | dialect-aware splitting: `;` inside a backtick identifier | **PASS** | `SELECT 1 AS \`a;b\`; SELECT 2 AS z;` split into exactly 2 statements, not 3 |
| 1.19 | ⌘↵ after `;` with backtick identifiers | **PASS** | sent `["SELECT 1 AS \`a;b\`"]` |

**Minor finding (no fix, deferred):** `src/lib/sqlSelection.ts::extractRunTarget`
hardcodes the postgres dialect and carries a `// ponytail:` note about it — but
it is **dead code**. The only caller is its own test file; `QueryView` uses
`runTargets(doc, sqlDialect, …)` which threads the real dialect. No user-visible
impact. Deleting it is a refactor, out of scope for this pass.

---

## 2. "Table loading and column loading is not proper"

All against `qa-pg` unless noted.

| # | Check | Result | Evidence |
|---|---|---|---|
| 2.1 | `public` is the default schema | **PASS** | schema combobox `public [selected]` on connect; footer `public · 13 objects` |
| 2.2 | `fresh` (500 rows, never ANALYZEd) shows rows | **PASS** | `open_table` → `{"count":500,"exact":true}`; pager `1–300 of 500` — the `reltuples = 0` estimate did not leak into the UI |
| 2.3 | view `customer_orders` shows rows | **PASS** | `{"count":300,"exact":true}`, pager `1–300 of 300` |
| 2.4 | `json_first` opens (json first column, no btree opclass) | **PASS** | `{"count":20,"exact":true}`, rows render, clear "Read-only — this table has no primary key" banner |
| 2.5 | paging through `wide` (1,000,000 rows) | **PASS** | `1–300` → `301–600` → `601–900` → `901–1,200 of 1,000,000`; exactly **one** `get_table_window` per page at offsets 300/600/900 |
| 2.6 | header-click sort asc → desc → off | **PASS** | `order: [{column:"id",desc:false}]` → `desc:true` → `null`; grid shows `id ▲ 1..5`, `id ▼ 500..496`, then unsorted with no indicator |
| 2.7 | filter on a bigint column | **PASS** | `id > 195` → `{"count":5,"exact":true}`, pager `1–5 of 5`; `psql` `select count(*) … where id > 195` = **5** |
| 2.8 | editing a cell | **FAIL → FIXED** | see **Bug 1** |
| 2.9 | editing a cell that is NULL | **FAIL → FIXED** | see **Bug 1**; after the fix `orig_value: null`, `rows_affected: 1`, psql confirms |
| 2.10 | Structure → Columns | **PASS** | 11 columns with PK badge, types, nullability, DEFAULT expressions, `users.id` reference link, and the `balance` column comment |
| 2.11 | Structure → Indexes | **PASS** | `customers_pkey` (PRIMARY, `id`), `customers_uuid_key` (UNIQUE, `uuid`) with full definitions |
| 2.12 | Structure → Triggers | **PASS** | `orders` shows `trg_orders_set_note` / `BEFORE INSERT`; `customers` correctly shows `Triggers 0` |
| 2.13 | Refresh picks up a table created from a Query tab | **PASS** | `CREATE TABLE qa_refresh_probe` in a Query tab → sidebar hit count 0 → click "Refresh schema" → one `list_objects`, sidebar hit count 1 |
| 2.14 | switching tables does not carry a filter across | **PASS** | with `id > 195` live on `customers`, opening `orders` sent `filters: null` and showed `1–300 of 300` |

### MySQL (`qa-mysql`)
`customers` loads `1–200 of 200`; Structure shows Columns 7 / Indexes 3 / Triggers 0 with
`enum(...)`, `decimal(12,2)`, `json`, `blob` types and the column comment. ERD: 5 relationships,
all correctly anchored.

### SQL Server (`qa-mssql`)
Connects to `master`, `switch_database` → `tableiq` re-introspects (schemas `auth`, `billing`,
`dbo`). `dbo.customers` loads `1–200 of ~200` — the `~` correctly marks the `sys.partitions`
estimate. ERD: 6 relationships, all correctly anchored. Multi-statement run splits
`SELECT 1 AS [a;b]` as one statement and maps the error to doc **Line 3**.

**Deferred (not a bug, but worth knowing):** a table tab's filter/sort/scroll is discarded when
you switch away and back. This is deliberate — `MainShell.tsx:455` renders table/ERD tabs only
while active because "each one holds a server-side cursor", and only query tabs stay mounted.
The stated requirement (no filter leaking *across* tables) passes. Keeping the filter would mean
lifting it into the tabs store, which is a behaviour change, not a QA fix.

---

## 3. "DDL diagram arrows are not proper"

The ERD draws edges in the same layout coordinate space as the cards and inside the same
pan/zoom layer, so I verified anchoring **in screen space**: for every edge I mapped its path
endpoints through `getScreenCTM()` and compared them against the `getBoundingClientRect()` of
the column row they should meet.

| # | Check | Result | Evidence |
|---|---|---|---|
| 3.1 | arrowheads land on the correct column rows at default zoom (49%) | **PASS** | 6/6 edges, `dy = 0.00`, all `insideRow: true` |
| 3.2 | …after zooming in (129%) | **PASS** | 6/6, `dy = 0.00` |
| 3.3 | …after zooming out to the 40% floor | **PASS** | 6/6, `dy = 0.00` |
| 3.4 | …after panning | **PASS** | 6/6, `dy = 0.00` at `translate(277.681px, -70px) scale(0.4)` |
| 3.5 | …after dragging a card (`orders`, which carries 3 edges) | **PASS** | 6/6, `dy = 0.00` |
| 3.6 | composite FK `shipments → order_items` is ONE edge | **PASS** | backend returns **7** FK column rows; the ERD header reads **6 relationships** and one line carries the label `order_id, line_no` |
| 3.7 | self-FK `auth.users.manager_id` is a visible loop | **PASS** | loop bulges off the right edge from `manager_id` and returns with an arrowhead on `id` |
| 3.8 | cross-schema `billing.invoices → public.orders` present | **PASS** | crow's foot on `invoices.order_id`, arrowhead on `orders.id` |
| 3.9 | composite label legibility | **FAIL → FIXED** | see **Bug 2** |

Same screen-space probe on MySQL (5 edges) and SQL Server (6 edges): 0 mis-anchored.

---

## 4. "Run a certain query on the database and see the result"

A five-aggregate, three-table join with `GROUP BY` / `HAVING` / `ORDER BY` / `LIMIT`:

```sql
SELECT c.id AS customer_id, c.name, count(DISTINCT o.id) AS orders,
       sum(oi.qty) AS units, round(sum(oi.qty * oi.price), 2) AS revenue
FROM customers c
JOIN orders o ON o.customer_id = c.id
JOIN order_items oi ON oi.order_id = o.id
GROUP BY c.id, c.name
HAVING sum(oi.qty * oi.price) > 300
ORDER BY revenue DESC, c.id
LIMIT 8;
```

**PASS** — all 8 rows byte-identical to `psql` in the container, including the `1240.20`
trailing zero (numeric scale preserved, not float-rounded) and the one `units = 16` outlier
among the `20`s. App: `qa-verified-aggregation.png`. Ground truth:
`qa-verified-aggregation-psql.txt`.

---

## 5. Production behaviour

| # | Check | Result | Evidence |
|---|---|---|---|
| 5.1 | a bad filter shows an error | **PASS** | `id > not-a-number` → `Failed to open customers: invalid input syntax for type bigint: "not-a-number"`, plus "A filter is applied. If the engine rejected it, clear it to get back to the table." |
| 5.2 | …that you can clear | **PASS** | "Clear filters" → one `get_table_window` 200 → `1–200 of 200`, error gone |
| 5.3 | …and does NOT flood the database | **PASS** | exactly **2** requests (`open_table` 400, `get_table_window` 400) and **no further traffic over 18 s** — confirmed in the recorder, no retry loop |
| 5.4 | a failed page surfaces instead of silently retrying | **PASS** | one page read stubbed to HTTP 400 → `Failed to open wide: cursor closed: the server dropped this page` with a "Try again" button, status dot `error`, and **exactly 1** request. "Try again" recovered to `301–600 of 1,000,000` |
| 5.5 | the error boundary catches a render crash without blanking the window | **PASS** | forced a real render-time `TypeError` in the ERD tab → per-tab fallback panel with the message and a Reload button; sidebar, tab bar, status bar and inspector all still painted, and switching to the `customers` tab still worked |

Note on 5.5: two earlier attempts to crash the app through `run_query` were *absorbed* rather
than crashing — a `columns: null` and a `columns: [null]` payload both failed inside
`commands.ts`'s response mapping and surfaced as an ordinary query error. That is good
defensive behaviour at the IPC boundary. The crash that did reach render came through
`foreignKeys()`, which returns its payload unmapped.

---

## Bugs found

### Bug 1 — editing any cell was silently impossible (FIXED, commit `c18029b`)

**Symptom.** Click a cell in a table grid, press Enter or a printable key, double-click it —
nothing happens. The cell highlights, no editor appears, no error is shown to the user. Applies
to every table on every engine, and to the query results grid too.

**Root cause.** `glide-data-grid` renders every overlay cell editor through
`createPortal(..., document.getElementById("portal"))`. In
`data-grid-overlay-editor.js`:

```js
const portalElement = document.getElementById("portal");
if (portalElement === null) {
    console.error('Cannot open Data Grid overlay editor, because portal not found. …');
    return null;
}
```

`index.html` mounted only `<div id="root">`. `DataGrid.tsx` correctly sets
`allowOverlay: editable`, so the editor component mounted and bailed on every keystroke —
`browse console --errors` carried glide's message verbatim.

**Fix.** Mount `<div id="portal">` as the last child of `<body>` in `index.html`, per the
glide-data-grid API docs.

**Test.** `src/components/gridPortal.test.ts` — asserts `index.html` contains
`<div id="portal"` and that it comes after `#root`. Failed before the fix (`expected -1 to be
greater than 269`), passes after. (vitest runs in node here with no jsdom, so the guard is on
the HTML that mounts the element.)

**Re-verified in the real app.** Editing `customers.nickname` on row 3 (a NULL) now opens the
editor pre-filled empty, stages as "1 unsaved", and Commit sends
`{"pk":[["id","3"]],"column":"nickname","new_value":"qa-was-null","orig_value":null}` →
`{"rows_affected":1}`; `psql` returns `qa-was-null`. A non-NULL edit sends
`orig_value: "nick2"` (correct optimistic-concurrency guard) and also lands. Both reverted.

Before/after: `qa-cell-edit-before-fix.png`, `qa-cell-edit-after-fix.png`.

**Why the previous round missed it.** The browser mock never renders a real grid editor, so the
path was never exercised.

### Bug 2 — the composite FK label read `order_id,,line_no` (FIXED, commit `b9aa7c0`)

**Symptom.** The one edge for `shipments → order_items` is labelled with its two columns, but
on screen it read `order_id,,line_no` — an apparent double comma, exactly the kind of thing
"DDL diagram arrows are not proper" describes.

**Root cause.** Not the text: `document.querySelector('svg text').textContent` is
`"order_id, line_no"`. The `<text>` uses a knockout halo (`stroke="var(--bg)"`,
`paintOrder="stroke"`) to hide the connector behind it, but at `strokeWidth={3}` the halo
reached only ~1.5px around each glyph and left the ~4px inter-word space uncovered. The edge's
own bezier passes straight through that gap, and the sliver of purple stroke that showed read
as a second comma.

**Fix.** `strokeWidth` 3 → 7 in `src/components/ErdView.tsx` so the knockout also covers the
spaces.

**Test.** None — this is SVG paint order with no jsdom in the repo, so there is nothing pure to
assert. Verified visually at 200% zoom: `qa-erd-label-before-fix.png` vs
`qa-erd-label-after-fix.png`.

### Bug 3 — SQL Server integration tests deadlocked against each other (FIXED, commit `c18029b`)

**Symptom.** `cargo test` with all three live URLs failed roughly one run in three:

```
sqlserver_driver_browse_query_edit … FAILED
pk: Db("Token error: 'Transaction (Process ID 76) was deadlocked on lock resources
 with another process and has been chosen as the deadlock victim.' … (code: 1205)")
```

**Root cause.** All nine tests in `tests/sqlserver.rs` work inside the same `master` database
and several run DDL. Under cargo's default parallelism that DDL deadlocks against another
test's `sys.*` catalog read. Confirmed: 9/9 pass with `--test-threads=1`, and failed on run 2
of 3 without it.

**Fix.** One `tokio::sync::Mutex` shared by the file; each test takes the guard for its body.
Test-only, no new dependency. Four consecutive parallel runs now pass.

Not a product bug, but it made the commit gate non-deterministic, which is why it is fixed
rather than deferred. CI is unaffected either way — `.github/workflows/ci.yml` deliberately
leaves `TIQ_MSSQL_URL` unset.

---

## Deferred findings

1. **Table tab state is discarded on tab switch** (filter, sort, scroll). Deliberate, documented
   at `src/components/MainShell.tsx:455`. The requirement under test (no filter carried *across*
   tables) passes. Restoring state per tab is a feature change, not a bug fix.
2. **Result sets do not survive a page reload** — only the SQL does. Result sets live in backend
   memory keyed by `result_id` and the persisted tab carries no `result_id`. TablePlus behaves
   the same after a restart.
3. **`src/lib/sqlSelection.ts::extractRunTarget` is dead code.** It carries a `// ponytail:` note
   about hardcoding the postgres dialect, which reads like a live bug, but its only caller is its
   own test file — `QueryView` uses `runTargets(doc, sqlDialect, …)`, which threads the real
   dialect (verified on MySQL backticks and SQL Server brackets). Deleting it is a refactor,
   out of scope here.
4. **Duplicate `get_table_window` on table open** — two identical requests fire on mount. This is
   React 19 `StrictMode` double-invoking effects in dev (`src/main.tsx` wraps the app in
   `<React.StrictMode>`); it does not happen in a production build. Paging, sorting and filtering
   all issue exactly one request each.
5. **gstack onboarding prompts skipped.** `gstack-skill-start` asked for an upgrade
   (1.87.4.0 → 1.88.1.0), CLAUDE.md routing rules, and an artifacts-sync privacy choice. All
   three were deferred rather than answered: upgrading gstack mid-pass would have restarted the
   tooling this QA run depends on.

---

## Gates

| Gate | Result |
|---|---|
| `pnpm exec tsc --noEmit` | clean |
| `pnpm exec vitest run` | **282 passed** / 42 files (280 before, +2 from the new portal test) |
| `cargo test` with `TIQ_PG_URL` + `TIQ_MYSQL_URL` + `TIQ_MSSQL_URL` | **204 passed, 0 failed, 4 ignored** |

Console errors across the whole session: none, other than the two deliberately induced
failures and glide's portal error that led to Bug 1.

---

## Health assessment

**Ship-ready on the four complaints, with one caveat.**

Three of the user's four complaints were already correct in this build and the QA simply
confirmed it against live data: the SQL tab's ⌘↵/⇧⌘↵ semantics, per-statement results,
doc-relative error lines, transaction rollback reporting and cancellation all behave the way
TablePlus does on Postgres, MySQL and SQL Server; the ERD's arrows are anchored exactly, and
provably stay anchored through zoom, pan and card drags; and a non-trivial aggregation returns
numbers identical to `psql`.

The fourth — "table and column loading" — hid a genuine, total failure: **no cell in the
application could be edited**, because a single `<div id="portal">` that glide-data-grid
requires was never mounted. Everything around it was right (the optimistic-concurrency
`orig_value`, the staged-edit bar, the commit path, the NULL-vs-empty distinction), which is
exactly why it survived review — the bug was one missing line in `index.html`, invisible to
every test and to the browser mock. That is now fixed and verified end to end against live
Postgres.

The caveat is process, not code: this is the second QA round on this branch, and the first one
passed clean while a "you cannot edit anything" bug was live. The lesson is the one the brief
already drew — a mock that never errors is worse than no test. The `ipc_bridge` harness plus a
`fetch` recorder that shows the exact bytes on the wire is what caught all three bugs here, and
it should be the default for future rounds.

Remaining risk is low and well-bounded: the deferred items are two deliberate design choices,
one piece of dead code, and one dev-only StrictMode artifact. Nothing found points at data
corruption, silent write loss, or a retry storm — the two error paths I forced both stopped
after a single request and told the user what happened.
