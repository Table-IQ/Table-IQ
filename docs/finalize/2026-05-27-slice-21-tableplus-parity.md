# Slice 21 — TablePlus Functional Parity (Finalize Report)

**Branch:** `slice-21-tableplus-parity`
**Spec:** [docs/superpowers/specs/2026-05-27-slice21-tableplus-parity.md](../superpowers/specs/2026-05-27-slice21-tableplus-parity.md)
**Plan:** [docs/superpowers/plans/2026-05-27-slice21-impl-plan.md](../superpowers/plans/2026-05-27-slice21-impl-plan.md)
**Workflow:** Rudy's full pipeline (brainstorm → spec → plan → TDD → frontend-design → subagent → review → QA → finalize)

---

## Task done

Closed the seven functional gaps the post-Slice-20 review identified — the "Big Six +1" features that
were visually present but stubbed:

1. **Per-column filters** — new `ColumnFilter`/`FilterOp` types, default-impl `build_where_clause`
   helper, `*_filtered` trait methods on all four SQL drivers (PG / MySQL / SQLite / MSSQL), threaded
   through `open_table` / `get_table_window` / `export_table` IPC, filter-chip rail + popover in
   [TableView.tsx](../../src/components/TableView.tsx).
2. **Inline row insert** — new `SqlDriver::insert_row` trait method, parameterised
   `build_insert_sql` helper, per-engine impls, new `insert_row` IPC command, draft-row rail with
   left-edge `--primary` accent + Commit/Cancel in TableView.
3. **Run selection (⇧⌘↵)** — pure `extractRunTarget` helper (explicit selection, statement-under-cursor,
   empty doc), `Mod-Shift-Enter` keymap in [SqlEditor.tsx](../../src/components/SqlEditor.tsx),
   `Run selection` toolbar button in [QueryView.tsx](../../src/components/QueryView.tsx).
4. **SQL autocomplete from schema** — new `schemaModelToCompletionMap` helper (bare + qualified
   names), passed into `sql({ schema })` in SqlEditor. Editor re-creates when schema identity
   changes.
5. **Right-click context menus** — new headless `ui/ContextMenu.tsx` primitive (outside-click /
   Escape / activation close + auto-flip near viewport edges). Wired into ObjectTree (Open / New
   Query Selecting / Show in Diagram / Pin / Copy name / Copy qualified), DataGrid (Copy value /
   Copy row JSON·CSV·MD / Set as filter), and the tab strip (Close / Close others / Close all /
   Duplicate). ErdView gains optional `focusTable` that centers + highlights one card.
6. **Tab persistence across relaunches** — new pure `tabsPersist.ts` (`hydrateForConnection` +
   `persistForConnection`, stale-table filter), wired into MainShell's connection lifecycle.
   Storage key `tiq:tabs:<connId>`.
7. **Parquet upload (Postgres + MySQL + SQLite + MSSQL)** — new `query/parquet.rs` (Arrow-backed
   parser, per-engine `arrow_type_to_sql`, `build_create_table_sql` with identifier validation,
   500 MB cap), new `parse_parquet_preview` + `apply_parquet_import` IPC commands, parquet flow
   in ImportDialog (file accept, target-schema/table inputs, inferred-types preview, confirmation
   modal for any CREATE TABLE or >100k-row append). Apply step streams INSERTs in 5k-row chunks
   so large files don't build one giant SQL string in memory.

## Test results

| Suite | Before | After | Δ |
|---|---:|---:|---:|
| Rust lib tests | 80 (`cargo test --lib`) | **87** (86 passed, 1 ignored ssh-live) | **+7** ✅ |
| Frontend tests | 57 | **84** | **+27** ✅ |
| `pnpm tsc -b` | 1 preexisting error in vite.config.ts | 1 preexisting error in vite.config.ts | unchanged ✅ |

New tests added:
- Rust: **10 driver helpers** (`build_where_clause` × 6 cases × 4 engines, `build_insert_sql` × 4
  engines) + **7 parquet helpers** (`arrow_type_to_sql` PG / MySQL / SQLite / struct fallback,
  `build_create_table_sql` PG / MySQL / SQLite / control-char rejection).
- Frontend: filterBuilder × 6, insertRow × 4, sqlSelection × 5, schemaCompletion × 3, rowFormat ×
  4, tabsPersist × 5 = **27**.

## QA evidence (browser-mock via `/browse`)

`pnpm dev` started; drove the app via the headless browse daemon. Captured at
[docs/finalize/assets/slice-21/](assets/slice-21/):

| # | What it shows | Asset |
|---|---|---|
| 01 | Connection manager (entry point) | [01-home.png](assets/slice-21/01-home.png) |
| 02 | Connected to Local Postgres mock; sidebar tree + workspace toolbar | [02-connected.png](assets/slice-21/02-connected.png) |
| 03 | `public.wide` table view, row-count chip, FK chip rail | [03-table-view.png](assets/slice-21/03-table-view.png) |
| 04 | **Filter** toolbar button active; FILTERS rail with column/op/value/Add row | [04-filter-builder.png](assets/slice-21/04-filter-builder.png) |
| 05 | **Insert row** active; NEW ROW rail with editable per-column inputs + Commit ↵ / Cancel | [05-draft-row.png](assets/slice-21/05-draft-row.png) |
| 06 | Query view toolbar — `Run`, `Run selection ⇧⌘↵`, `Run in tx`, `Explain` | [06-query-view.png](assets/slice-21/06-query-view.png) |
| 07 | Import dialog title "Import CSV / JSON / Parquet"; drop hint mentions `.parquet` | [07-import-dialog.png](assets/slice-21/07-import-dialog.png) |
| 08 | Right-click context menu on a sidebar table — Open / New Query Selecting / Show in Diagram / Pin / Copy name / Copy as public.wide | [08-context-menu.png](assets/slice-21/08-context-menu.png) |

## Checked thoroughly

- **Backend SQL correctness:** pure builders (`build_where_clause`, `build_insert_sql`,
  `arrow_type_to_sql`, `build_create_table_sql`) are exhaustively unit-tested per engine for both
  identifier quoting (PG/SQLite `"…"`, MySQL `` `…` ``, MSSQL `[…]`) and placeholder dialect (PG
  `$N`, SQLite `?N`, MySQL `?`, MSSQL `@PN`). All 17 new Rust tests green.
- **Backwards compat:** the IPC commands that gained `filters: Option<Vec<ColumnFilter>>` default
  to empty when unset, which routes to the original `open_window` / `row_estimate` paths via the
  trait's default `*_filtered` impls. Existing callers untouched.
- **Trust boundary:** parquet bytes are size-capped both client-side (pre-allocation check before
  `arrayBuffer()`) and server-side (500 MB ceiling). Target identifiers (schema / table /
  columns) validated for control chars before any engine call (`validate_target_idents`).
  Engine-specific `quote_ident` provides the second line of defence.
- **Concurrent state:** the tabs hydration / persistence race the reviewer flagged is fixed —
  the persist effect now gates on `hydrated === active.id` so it never overwrites a fresh
  per-connection slice with empty pre-mount state.
- **Parquet memory bound:** review flagged the original "load all rows → build one giant INSERT"
  path. Apply step now streams in 5,000-row chunks through `run_script`, keeping peak SQL string
  size bounded.
- **UI features verified live** via browse screenshots (above) — each of the 7 features renders
  in the browser-mock app, including the keyboard shortcut hint on `Run selection`, the
  CREATE-or-append toggle on parquet target, the filter chip rail, and the context menu items.
- **Code review** (`general-purpose` agent reviewing the staged diff): 1 BLOCKER + 1 MAJOR + 3
  MINOR + 1 NIT findings. BLOCKER and MAJOR both fixed in this branch. The remaining MINOR
  (`extractRunTarget` doesn't handle `;` inside string literals or block comments) is a documented
  TablePlus-style limitation; deferred. The MINOR about `validate_ident` permissiveness was
  addressed by always-on validation regardless of `create_table`.

## Per-stage cost

(generated via `~/.claude/skills/rudys-workflow/cost-table.py`)

| Stage | Model | Input | Output | Cache read | Cache write | Cost (USD) |
|---|---|---|---|---|---|---|
| 1. brainstorm | opus | 83 | 9,426 | 1,334,843 | 208,390 | $6.62 |
| 2. spec-saved | opus | 7 | 1,687 | 520,341 | 12,294 | $1.14 |
| 3. plan | opus | 10 | 15,240 | 881,736 | 52,620 | $3.45 |
| 4. tdd | opus | 171 | 129,739 | 29,869,553 | 247,294 | $59.17 |
| 5+6. frontend-design+subagent | opus | 13 | 4,446 | 3,583,415 | 9,369 | $5.88 |
| 7. review | opus | 20 | 12,826 | 5,682,213 | 22,471 | $9.91 |
| 8. qa | opus, sonnet | 78 | 16,473 | 17,341,385 | 75,589 | $28.67 |
| 9. finalize | opus | 1 | 154 | 332,380 | 842 | $0.53 |
| **Total** | | **383** | **189,991** | **59,545,866** | **628,869** | **$115.37** |

The "tdd" stage carries the bulk of the work because it covers backend wiring + helper builds +
test scaffolding for all 7 features (rudy's workflow merges the TDD + impl steps for pure
helpers).

## Credentials

No external services hit — browser-mock backend only, Docker containers not required for this
slice. `.claude/finalize-creds.json` not used.

## Out of scope (intentionally deferred to Slice 22)

- Free-text raw-WHERE input (use the SQL tab).
- Save-as-view / named filter sets.
- Bulk insert dialog (CSV/JSON/Parquet ImportDialog covers that path).
- Snippet completions in the SQL editor.
- Per-tab editor SQL persistence (v1 restores tab metadata only).
- `sqlSelection` SQL-comment / dollar-quote awareness.
- Postgres `COPY FROM STDIN` binary fast path for parquet imports.
- Chunked / partitioned parquet upload for files >500 MB.
- Parquet export (the reverse direction — table → `.parquet`).

## What changes for `main`

35 files changed: 6 modified Rust files (drivers, IPC, lib), 1 new Rust module (`query/parquet.rs`),
1 new ContextMenu primitive, 8 modified frontend components, 6 new pure frontend helpers (+ their
tests), 1 spec, 1 plan, 1 report, 8 screenshots.
