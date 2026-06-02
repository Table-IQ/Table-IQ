# Spec — Slice 31: TablePlus interaction parity (context menus, sidebar, chrome)

Date: 2026-06-02 · Branch: `slice-31-tableplus-interactions`

Now that we connect to real data (`cartiq`), bring the interaction layer up to
TablePlus: rich right-click menus (table + row), a sidebar `+` menu, and more
prominent chrome. Replicate TablePlus, adapted to our theme.

## Foundational
- **F0 ContextMenu submenus.** Add nested submenu support to
  `ui/ContextMenu.tsx` (an item may have `children`; hovering opens a flyout).

## Pure logic (TDD)
- **L1 `rowFormat`** — extend `buildRowAs` formats to: `plain` (tab-separated),
  `json`, `html` (table), `md`, `csv`, `csv-header`, `insert` (INSERT statement).
- **L2 `tableScript`** (new) — `buildTableScript(kind, schema, table, columns)`
  for `select` / `drop` / `truncate` / `create` (CREATE TABLE from column specs).
  Identifiers quoted. Pure + unit-tested.

## Row context menu (data grid → TableView)
TablePlus order, using submenus:
- Refresh · Add row · Duplicate row
- Set value → (empty `''` / NULL / DEFAULT) via existing `applyEdit`
- Copy cell value · Copy all column values
- Copy row as → (Plain / JSON / HTML / Markdown / CSV / CSV with header / INSERT)
- Quick filter by this cell (existing `upsertFilter`)
- Copy DELETE statement (safe; row hard-delete execution **deferred** — no backend)
- Export current page (CSV / JSON of the loaded window)

Needs the **full row** at right-click: extend `DataGrid.onCellContext` to include
`values` (via the existing `readRow`).

## Table context menu (sidebar tree → MainShell)
- Open · Open structure · Item overview (focus Inspector) · Copy name · Copy as `schema.table`
- Pin to top
- Copy script as → (CREATE / DROP / TRUNCATE / SELECT) — **copies SQL** (TablePlus
  behavior; no execution)
- Export → (CSV / JSON) via existing `exportTable`
- New → (Table → StructureEditor; View / Materialized view / Function / Procedure /
  Schema / Group → open a New Query pre-filled with a template)
- Truncate / Drop → **open a New Query** pre-filled with the statement (safe review
  before running on real data)

## Sidebar bottom `+` menu (MainShell)
A `+` button at the sidebar foot → New Table / View / Materialized view / Function /
Procedure / Schema / Group (same routing as the table "New →" submenu). Keep the
schema label.

## Chrome
- **Header**: ensure every top-right icon has a hover tooltip (`title`); surface
  ERD / Import / Diff / Command palette in the strip.
- **Bottom bar / Structure**: Structure view gains sub-tabs **Columns / Indexes /
  Triggers** (Columns live; Indexes/Triggers show a "needs introspection" note —
  backend **deferred**).

## Deferred (documented in report)
Export-with-column-selection, Import-from-SQL execution, Materialized-view /
Procedure DDL creation, Structure Indexes/Triggers introspection (backend), Quick
Look preview, row hard-delete execution. Menu entries route to safe SQL-in-a-query
or are clearly marked.

## Verification
- vitest green incl. new `rowFormat` + `tableScript` + ContextMenu submenu tests.
- `tsc` clean. `/browse` screenshots of both context menus + sidebar `+` menu.
