import {
  DataEditor,
  GridCellKind,
  type EditableGridCell,
  type GridCell,
  type GridColumn,
  type GridSelection,
  type Item,
  type DataEditorRef,
} from "@glideapps/glide-data-grid";
import "@glideapps/glide-data-grid/dist/index.css";
import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { decodeWindow, searchHits, WindowCache, type DecodedWindow } from "../lib/arrowWindow";
import { visibleColumns } from "../lib/keyset";
import { gridTheme, gridRowTints, editTints, nullCellStyle, searchHitBg } from "../lib/theme";
import { fadeOutAlpha, mixHex, prefersReducedMotion, pulseAlpha } from "../lib/motion";
import { useTheme } from "../theme/ThemeProvider";
import type { ForeignKey, SortKey } from "../lib/commands";
import {
  discardAll,
  exclusive,
  gatedCommit,
  isNullLeftEmpty,
  listPending,
  pendingKey,
  removePending,
  stageEdit,
  movedTints,
  type PendingMap,
} from "../lib/stagedEdits";

import { DEFAULT_COL_WIDTH } from "../lib/colWidths";
import { autoFitWidths } from "../lib/autoFit";
import {
  insertCellDisplay,
  stagedCounts,
  totalStaged,
  type StagedCounts,
  type StagedInsert,
  type StagedSnapshot,
} from "../lib/stagedChanges";

/** Same column list, same order? */
function sameNames(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((n, i) => n === b[i]);
}

const PAGE = 200; // default rows per fetched window (a source may override)
const MAX_PAGES = 12; // LRU bound → JS heap stays bounded regardless of total rows
const AUTO_FIT_SAMPLE = 60; // rows sampled from the first page to size columns to content
const ROW_HEIGHT = 24; // TablePlus: 12.5px mono in a 24px row (measured)
const HEADER_HEIGHT = 28;
// Tint motion (DESIGN.md → Motion). The canvas can't use CSS transitions, so a
// fade is a per-frame blend toward the row's own background.
const SAVED_HOLD = 800; // green holds this long, then fades over TINT_FADE
const SAVED_HOLD_REDUCED = 1100; // reduced motion: solid green, then off (no fade)
const TINT_FADE = 300;
const PULSE_PERIOD = 1000; // amber "saving" breath

/** A tint fading out on the canvas. Keyed like `editState` (`row:col`). */
interface TintFx {
  tint: "saved" | "error";
  start: number;
  hold: number;
  dur: number;
}

/** A windowed data source the grid pages over. `key` identity resets the cache. */
export interface GridSource {
  key: string;
  totalRows: number;
  columns?: string[];
  /** Rows per fetched window. Defaults to {@link PAGE}; a source that already
   *  pages upstream (TableView) sets it to its own page size so one fetch
   *  covers the whole visible window. */
  pageSize?: number;
  /** Bumped when the same rows must be re-read from the server (Refresh /
   *  commit / insert / delete). Drops the cache *without* the full reset a new
   *  `key` does, so scroll, selection and fitted widths survive. */
  version?: number;
  fetch: (offset: number, len: number) => Promise<{ bytes: Uint8Array; ms: number }>;
}

export interface DataGridHandle {
  scrollToRow: (row: number) => void;
  /** Commit every staged change to the DB in one call (TablePlus ⌘S). */
  commitPending: () => void;
  /** Drop every staged change and revert the display. */
  discardPending: () => void;
  /** Everything staged, for the commit batch and the Review SQL. */
  snapshot: () => StagedSnapshot;
  /** Stage these page rows for delete. Rows past the page, already staged, or whose page
   *  isn't loaded are skipped; a no-op unless the grid is editable and has a PK. */
  stageDeleteRows: (rows: number[]) => void;
  /** Unstage a staged delete (its staged cell edits come back into the batch). */
  undoDelete: (row: number) => void;
  /** Append a staged new row after the page's rows. False when the grid has no
   *  `onCommitChanges` (read-only, or the parent withholds it while a page is in flight)
   *  or a commit is in flight, so the caller can keep the draft. */
  stageInsert: (values: [string, string | null][]) => boolean;
  /** Remove a staged new row, by its grid row. */
  removeInsertedRow: (row: number) => void;
}

interface Props {
  source: GridSource;
  onWindowFetched?: (ms: number) => void;
  /** When set, the grid is editable: cells stage edits, rows stage deletes and inserts,
   *  and every commit path hands everything staged to this in ONE call. Resolve true once
   *  it is all written (the grid clears and calls `onSave`), false to keep it all staged. */
  onCommitChanges?: (snap: StagedSnapshot) => Promise<boolean>;
  /** Map of column-name → foreign-key edge; cells whose column is a FK show a chevron affordance. */
  fkByColumn?: Map<string, ForeignKey>;
  /** Fires when the user clicks a FK chevron / cell. */
  onFkClick?: (toTable: string, toColumn: string, fromValue: string | null) => void;
  /** Right-click on a cell (Slice 21 #5). `value` is the visible cell text (or null);
   *  `values` is the full {col → value} map for the row (null if its page hasn't
   *  loaded), used by the row context menu's copy/duplicate actions. */
  onCellContext?: (args: {
    col: string;
    row: number;
    value: string | null;
    values: Record<string, string | null> | null;
    /** The same row read WITHOUT staged edits — what the database still holds.
     *  Use this (never `values`) for anything that identifies the row: a PK in a
     *  DELETE/UPDATE WHERE clause. Null when the page hasn't loaded. */
    serverValues: Record<string, string | null> | null;
    /** A staged row: its menu offers Undo delete / Remove row instead. */
    staged?: "deleted" | "inserted";
    x: number;
    y: number;
  }) => void;
  /** Fires when the grid's visible row range changes — used by the status-bar pager. */
  onVisibleRangeChange?: (firstRow: number, lastRow: number) => void;
  /** Fires on any cell click with the whole row's values map. Used by the
   *  Inspector's row-detail mode (Slice 27). Receives `null` when the row's
   *  page hasn't loaded yet (the caller should ignore null). */
  onRowSelected?: (row: number, values: Record<string, string | null> | null) => void;
  /** Per-column width overrides, keyed by column name. Missing entries fall
   *  back to {@link DEFAULT_COL_WIDTH}. (Slice 28 — TablePlus parity). */
  columnWidths?: Record<string, number>;
  /** Fires when the user finishes dragging a column edge. Parent persists the
   *  new width and (typically) updates `columnWidths` on the next render. */
  onColumnResize?: (column: string, newWidth: number) => void;
  /** Columns whose values are numeric — rendered right-aligned (TablePlus
   *  parity, Slice 29). */
  rightAlignColumns?: ReadonlySet<string>;
  /** Called after a successful commit (with how many changes it wrote) so the
   *  parent can re-sync the page. */
  onSave?: (saved: number) => void;
  /** Fires whenever the staged counts change. */
  onStagedCountsChange?: (counts: StagedCounts) => void;
  /** True while {@link DataGridHandle.commitPending} is writing the staged batch.
   *  The parent MUST use it to lock anything that would change what a row index
   *  means (sort, paging, refresh) — see the generation guard in `commitPending`. */
  onCommittingChange?: (busy: boolean) => void;
  /** A page fetch failed. The page is remembered as failed and never
   *  re-requested on repaint, so one broken window can't flood the database. */
  onFetchError?: (msg: string) => void;
  /** Active sort — drives the ▲/▼ glyph in the header. */
  sort?: SortKey | null;
  /** Header click. The parent owns the asc → desc → none cycle. */
  onSort?: (column: string) => void;
  /** Number shown on the first row's marker — a paged source passes offset + 1
   *  so numbering continues across pages. Defaults to 1. */
  rowMarkerStart?: number;
  /** Search term whose matching cells get the accent tint. */
  highlight?: string;
  /** Primary-key columns. A saved tint remembers its row by them, so a re-read
   *  that moves another row onto that index doesn't inherit the green. */
  pk?: string[] | null;
  /** Asked before any commit (⌘S, the in-grid Commit, the titlebar's Commit — all
   *  route through `commitPending`); resolving false leaves every edit staged. Set
   *  for production connections. */
  confirmCommit?: () => Promise<boolean>;
}

/** Row-marker column width for the widest number it will show (~8px a digit). */
function markerWidth(lastNumber: number): number {
  return Math.max(32, 8 + 8 * String(lastNumber).length);
}

/**
 * glide-data-grid bound to a windowed Arrow source. Cells read from an LRU page
 * cache; misses trigger a background fetch + re-render. Only ~MAX_PAGES pages are
 * ever resident, so memory does not grow with total row count.
 */
export const DataGrid = forwardRef<DataGridHandle, Props>(function DataGrid(
  {
    source,
    onWindowFetched,
    onCommitChanges,
    fkByColumn,
    onFkClick,
    onCellContext,
    onVisibleRangeChange,
    onRowSelected,
    columnWidths,
    onColumnResize,
    rightAlignColumns,
    onSave,
    onStagedCountsChange,
    onCommittingChange,
    onFetchError,
    sort,
    onSort,
    rowMarkerStart = 1,
    highlight = "",
    pk,
    confirmCommit,
  },
  ref,
) {
  const { resolved } = useTheme();
  // The canvas can't resolve CSS variables, so FK text gets the theme's hex.
  const fkText = useMemo(() => {
    const c = gridTheme(resolved).accentColor;
    return { textDark: c, textLight: c };
  }, [resolved]);
  const editable = !!onCommitChanges;
  const gridRef = useRef<DataEditorRef>(null);
  const cacheRef = useRef(new WindowCache(source.pageSize ?? PAGE, MAX_PAGES));
  // Pages whose fetch failed. They are NOT retried on repaint — glide re-asks for
  // every visible cell on each paint (a mouse move is enough), so without this a
  // failing page hammers the database until the user navigates away.
  const failed = useRef(new Set<number>());
  // Bumped on every source/version change. A fetch that resolves against an older
  // generation is dropped, so the previous table's rows can never land in this
  // table's cache.
  const generation = useRef(0);
  const overlay = useRef(new Map<string, string>()); // optimistic edits: `${row}:${col}` → value
  // Staged (uncommitted) edits in the TablePlus batch model — committed/discarded
  // as a set. `overlay` mirrors their values for display; `pending` drives the bar.
  const pending = useRef<PendingMap>(new Map());
  // Staged row deletes (page-relative rows) and new rows, committed with the edits.
  const deletes = useRef(new Set<number>());
  const inserts = useRef<StagedInsert[]>([]);
  const notifyPending = useCallback(() => {
    onStagedCountsChange?.(stagedCounts(listPending(pending.current), deletes.current, inserts.current.length));
  }, [onStagedCountsChange]);
  // Per-cell tint: blue while staged, amber while committing, green on success
  // (auto-clears), red on conflict/error (TablePlus parity).
  const editState = useRef(new Map<string, "staged" | "saving" | "saved" | "error">());
  // Undo stack of applied edits ({row,col,prev}) for ⌘Z.
  const undoStack = useRef<Array<{ row: number; col: string; prev: string | null }>>([]);
  // Pending save-state clear timers, keyed by cell — so we can cancel them on
  // unmount / source switch (else setVersion fires on an unmounted grid).
  const editTimers = useRef(new Map<string, number>());
  // Row key (PK column → value) of each saved tint, checked when a re-read lands.
  const savedIds = useRef(new Map<string, Record<string, string | null>>());
  const pkRef = useRef(pk);
  pkRef.current = pk;
  const confirmCommitRef = useRef(confirmCommit);
  confirmCommitRef.current = confirmCommit;
  // A commit (its confirm included) is in flight: a second ⌘S is dropped, not re-sent,
  // and staging is frozen — a successful commit clears everything staged, so anything
  // staged or unstaged meanwhile would be lost silently.
  const committingRef = useRef(false);
  // Monotonic per-cell edit token — a resolving write that's been superseded by
  // a newer edit on the same cell bails instead of clobbering it.
  const editSeq = useRef(new Map<string, number>());
  const inflight = useRef(new Set<number>());
  // Fading tints + the rAF loop that repaints them (and pulses "saving" cells).
  // The loop only runs while something is animating, never under reduced motion.
  const fx = useRef(new Map<string, TintFx>());
  const rafId = useRef<number | null>(null);
  const pulseStart = useRef(0);
  const reducedMotion = useRef(prefersReducedMotion());
  const [columns, setColumns] = useState<GridColumn[]>([]);
  const colNames = useRef<string[]>([]);
  // Search hits per decoded page, computed once per (page, term, columns) — a
  // re-read or new page is a new window object, so stale sets fall away.
  const hitsRef = useRef(
    new WeakMap<DecodedWindow, { term: string; cols: string[]; hits: Set<number> }>(),
  );
  const [, setVersion] = useState(0);
  // Content-fitted widths computed once from the first loaded page (TablePlus
  // parity). Persisted user drags always win over these (see `widthFor`).
  const autoFitRef = useRef<Record<string, number>>({});
  const autoFitDoneRef = useRef(false);
  // Hovered row index drives a faint row highlight (see `getRowThemeOverride`).
  const [hoverRow, setHoverRow] = useState<number | undefined>(undefined);
  // Clicked row gets a stronger "selected" band (TablePlus parity) — the whole
  // row highlights, on top of glide's native single-cell focus ring.
  const [selRow, setSelRow] = useState<number | undefined>(undefined);
  // Read by the stable `getCellContent`: a fading tint blends toward the row's
  // own band (selected / hover / zebra), so it ends without a colour jump.
  const selRowRef = useRef(selRow);
  const hoverRowRef = useRef(hoverRow);
  useEffect(() => {
    selRowRef.current = selRow;
    hoverRowRef.current = hoverRow;
  }, [selRow, hoverRow]);
  useEffect(() => {
    const mq = window.matchMedia?.("(prefers-reduced-motion: reduce)");
    if (!mq) return;
    const on = () => {
      reducedMotion.current = mq.matches;
    };
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);

  const onFetchedRef = useRef(onWindowFetched);
  useEffect(() => {
    onFetchedRef.current = onWindowFetched;
  }, [onWindowFetched]);
  const onFetchErrorRef = useRef(onFetchError);
  useEffect(() => {
    onFetchErrorRef.current = onFetchError;
  }, [onFetchError]);
  // Read by the stable `applyColumns` so a sort change re-labels the header
  // without re-creating `ensurePage`.
  const sortRef = useRef(sort);
  useEffect(() => {
    sortRef.current = sort;
  }, [sort]);

  // Mirror columnWidths into a ref so the stable `maybeAutoFit` callback can read
  // the latest persisted widths without re-creating (and churning `ensurePage`).
  const columnWidthsRef = useRef(columnWidths);
  useEffect(() => {
    columnWidthsRef.current = columnWidths;
  }, [columnWidths]);

  // A user's persisted drag wins; otherwise use the content-fitted width; only
  // then fall back to the legacy fixed default.
  const widthFor = (name: string) =>
    columnWidthsRef.current?.[name] ?? autoFitRef.current[name] ?? DEFAULT_COL_WIDTH;

  /** Rebuild the glide columns from `names`. The sort glyph lives in the *title*
   *  only — `id` stays the bare column name, so width persistence and the resize
   *  handler are unaffected by it. */
  const applyColumns = useCallback((names: string[]) => {
    const s = sortRef.current;
    setColumns(
      names.map((c) => ({
        id: c,
        title: s?.column === c ? `${c} ${s.desc ? "▼" : "▲"}` : c,
        width: widthFor(c),
      })),
    );
    // widthFor reads refs only; re-creating this callback would churn every
    // effect that rebuilds the header.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Size each not-yet-resized column to its content using a sample of the first
  // loaded page. Runs once per source (guarded by `autoFitDoneRef`); retries on
  // the next fetch if the page hasn't decoded yet. Persisted widths are skipped.
  const maybeAutoFit = useCallback(() => {
    if (autoFitDoneRef.current) return;
    const cols = colNames.current;
    if (cols.length === 0) return;
    const persisted = columnWidthsRef.current ?? {};
    const rows: Record<string, string>[] = [];
    for (let r = 0; r < AUTO_FIT_SAMPLE; r++) {
      const row: Record<string, string> = {};
      let complete = true;
      for (const c of cols) {
        const v = cacheRef.current.getCell(r, c);
        if (v === undefined) {
          complete = false;
          break;
        }
        // Width-fitting only cares about rendered width; the `NULL` placeholder
        // is narrower than the default column width, so measuring it as empty is
        // close enough and keeps autoFit's Record<string, string> shape.
        row[c] = v ?? "";
      }
      if (!complete) break;
      rows.push(row);
    }
    if (rows.length === 0) return; // first page not decoded yet — retry next fetch
    autoFitRef.current = autoFitWidths(cols, rows, persisted);
    autoFitDoneRef.current = true;
    applyColumns(cols);
  }, [applyColumns]);
  // Reset cache + columns whenever the source changes.
  useEffect(() => {
    cacheRef.current = new WindowCache(source.pageSize ?? PAGE, MAX_PAGES);
    generation.current += 1;
    failed.current.clear();
    overlay.current.clear();
    inflight.current.clear();
    // Bare `row:col` edit keys carry no source identity, so they MUST be wiped
    // on every source switch — otherwise a stale tint repaints an unrelated cell
    // and a leftover undo entry would write into a freshly-refetched row.
    editState.current.clear();
    pending.current = new Map();
    deletes.current = new Set();
    inserts.current = [];
    // notifyPending() is intentionally left out of this effect's deps — the parent
    // passes a stable setState, and re-running the reset on its identity change
    // would wipe staged edits unexpectedly.
    notifyPending();
    undoStack.current = [];
    editSeq.current.clear();
    editTimers.current.forEach((id) => window.clearTimeout(id));
    editTimers.current.clear();
    savedIds.current.clear();
    fx.current.clear();
    autoFitRef.current = {};
    autoFitDoneRef.current = false;
    setSelRow(undefined); // stale selection must not linger across table/result switches
    if (source.columns && source.columns.length > 0) {
      colNames.current = source.columns;
      applyColumns(source.columns);
    } else {
      colNames.current = [];
      setColumns([]);
    }
    setVersion((v) => v + 1);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source.key, source.columns, source.pageSize, applyColumns]);

  // Same rows, re-read from the server (Refresh / commit / insert / delete):
  // drop the cached pages and let the repaint re-fetch. Deliberately does NOT
  // touch selection, scroll or fitted widths — that is the whole point.
  useEffect(() => {
    if (source.version === undefined) return;
    cacheRef.current = new WindowCache(source.pageSize ?? PAGE, MAX_PAGES);
    generation.current += 1;
    failed.current.clear();
    inflight.current.clear();
    // Server values win over anything still staged — the display would otherwise
    // show an overlay value the "unsaved changes" bar no longer tracks.
    overlay.current.clear();
    // Staged / error tints go with the staged values. A "saved" tint stays: a
    // commit ends in exactly this re-read, and its green has to finish on the
    // refreshed row (its clear timer still owns it).
    for (const [k, st] of editState.current) if (st !== "saved") editState.current.delete(k);
    pending.current = new Map();
    deletes.current = new Set();
    inserts.current = [];
    notifyPending();
    undoStack.current = [];
    setVersion((v) => v + 1);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source.version]);

  // Apply width updates from the parent (e.g. after persistence rehydrates) and
  // re-label the header when the sort changes.
  useEffect(() => {
    if (colNames.current.length === 0) return;
    applyColumns(colNames.current);
  }, [columnWidths, sort, applyColumns]);

  /** One animation frame: repaint only the tinted cells, retire finished fades,
   *  and keep going while any fade is live or any cell is still saving. */
  const tick = useCallback(() => {
    rafId.current = null;
    const now = performance.now();
    const cols = colNames.current;
    const damage: { cell: Item }[] = [];
    const add = (key: string) => {
      const i = key.indexOf(":");
      const row = Number(key.slice(0, i));
      const name = key.slice(i + 1);
      if (cols.includes(name)) damage.push({ cell: [cols.indexOf(name), row] });
    };
    for (const [key, f] of fx.current) {
      add(key);
      if (now - f.start >= f.hold + f.dur) {
        fx.current.delete(key); // this frame repaints it without the tint
      }
    }
    let saving = false;
    for (const [key, st] of editState.current) {
      if (st !== "saving") continue;
      saving = true;
      add(key);
    }
    if (damage.length > 0) gridRef.current?.updateCells(damage);
    if (fx.current.size > 0 || saving) rafId.current = requestAnimationFrame(tick);
  }, []);

  /** Start the loop if it isn't running. A no-op under reduced motion. */
  const animate = useCallback(() => {
    if (rafId.current !== null || reducedMotion.current) return;
    rafId.current = requestAnimationFrame(tick);
  }, [tick]);

  // Unmount: stop the tint loop and every pending saved-tint timer, so neither
  // touches a grid that is gone.
  useEffect(() => {
    const timers = editTimers.current;
    return () => {
      if (rafId.current !== null) cancelAnimationFrame(rafId.current);
      timers.forEach((id) => window.clearTimeout(id));
      timers.clear();
    };
  }, []);

  const ensurePage = useCallback(
    (page: number) => {
      const size = cacheRef.current.pageSize;
      // Never ask for a page that starts at or past the last row — a wrong total
      // used to send the grid fetching windows the table doesn't have.
      if (page < 0 || page * size >= source.totalRows) return;
      if (cacheRef.current.has(page) || inflight.current.has(page)) return;
      if (failed.current.has(page)) return;
      const gen = generation.current;
      inflight.current.add(page);
      source
        .fetch(page * size, size)
        .then(({ bytes, ms }) => {
          if (gen !== generation.current) return; // source changed mid-flight
          const win = decodeWindow(bytes);
          cacheRef.current.put(page, win);
          // A saved tint whose row index now holds a different row goes.
          for (const key of movedTints(savedIds.current, (r, c) => cacheRef.current.getCell(r, c))) {
            savedIds.current.delete(key);
            if (editState.current.get(key) === "saved") editState.current.delete(key);
          }
          // The first page is the authority on the column list: the sidebar
          // schema can be stale (or absent for a view), the decoded Arrow window
          // never is. The hidden paging cursor column is never shown.
          const names = visibleColumns(win.columns);
          if (
            names.length > 0 &&
            (page === 0 || colNames.current.length === 0) &&
            !sameNames(colNames.current, names)
          ) {
            colNames.current = names;
            applyColumns(names);
          }
          maybeAutoFit();
          onFetchedRef.current?.(ms);
          setVersion((v) => v + 1);
        })
        .catch((e) => {
          if (gen !== generation.current) return;
          failed.current.add(page);
          onFetchErrorRef.current?.(String(e));
          setVersion((v) => v + 1);
        })
        .finally(() => {
          // Generation-scoped like the handlers above: a stale fetch must not
          // delete the in-flight marker the NEW source just put down for the same
          // page, which would let a duplicate concurrent fetch through. The reset
          // that bumped the generation already cleared the set.
          if (gen === generation.current) inflight.current.delete(page);
        });
    },
    [source, maybeAutoFit, applyColumns],
  );

  useEffect(() => {
    ensurePage(0);
  }, [source.totalRows, ensurePage]);

  /** The background `getRowThemeOverride` gives `row` — what a fade ends on. */
  const rowBg = useCallback(
    (row: number) => {
      const t = gridRowTints(resolved);
      if (row === selRowRef.current) return t.selected;
      if (row === hoverRowRef.current) return t.hover;
      return row % 2 === 1 ? t.zebra : gridTheme(resolved).bgCell;
    },
    [resolved],
  );

  const getCellContent = useCallback(
    ([col, row]: Item): GridCell => {
      const name = colNames.current[col];
      // A staged new row: read-only (v1), green, DEFAULT / NULL muted italic.
      if (row >= source.totalRows) {
        const ins = inserts.current[row - source.totalRows];
        if (!ins || !name) return { kind: GridCellKind.Loading, allowOverlay: false };
        const cell = insertCellDisplay(ins.values, name);
        return {
          kind: GridCellKind.Text,
          data: cell.kind === "value" ? cell.text : "",
          displayData: cell.text,
          allowOverlay: false,
          readonly: true,
          contentAlign: cell.kind === "value" && rightAlignColumns?.has(name) ? "right" : undefined,
          themeOverride: {
            bgCell: editTints(resolved).inserted,
            ...(cell.kind === "value" ? {} : nullCellStyle(resolved)),
          },
        };
      }
      // A row staged for delete paints red over any cell tint, and can't be edited.
      const deleted = deletes.current.has(row);
      const ov = name ? overlay.current.get(`${row}:${name}`) : undefined;
      const value = ov ?? (name ? cacheRef.current.getCell(row, name) : undefined);
      if (value === undefined) {
        const page = cacheRef.current.pageOf(row);
        ensurePage(page);
        ensurePage(page + 1);
        return { kind: GridCellKind.Loading, allowOverlay: false };
      }
      const isNull = value === null;
      const fk = name ? fkByColumn?.get(name) : undefined;
      const display = fk && !isNull && value !== "" ? `→ ${value}` : value;
      // Numeric columns right-align (TablePlus parity) — but never an FK cell,
      // whose `→ value` affordance reads left-to-right.
      const numeric = !fk && name ? rightAlignColumns?.has(name) : false;
      // Save-state tint: amber while writing (breathing, unless reduced motion),
      // green on save, red on error. A fading saved tint blends toward the row's
      // own background.
      const save = name ? editState.current.get(`${row}:${name}`) : undefined;
      const fade = save ? undefined : fx.current.get(`${row}:${name}`);
      let saveBg: string | undefined;
      if (deleted) {
        saveBg = editTints(resolved).deleted;
      } else if (save || fade) {
        const tints = editTints(resolved);
        const now = performance.now();
        if (save === "saving" && !reducedMotion.current) {
          saveBg = mixHex(rowBg(row), tints.saving, pulseAlpha(now - pulseStart.current, PULSE_PERIOD));
        } else if (save) {
          saveBg = tints[save];
        } else if (fade) {
          saveBg = mixHex(rowBg(row), tints[fade.tint], fadeOutAlpha(now - fade.start, fade.dur, fade.hold));
        }
      }
      // A search hit gets the accent tint, looked up in the page's precomputed
      // hit set (see `searchHits`) — O(1) per cell, whatever the cell's length.
      if (saveBg === undefined && highlight !== "" && !isNull && ov === undefined) {
        const win = cacheRef.current.windowOf(row);
        if (win) {
          const cols = colNames.current;
          let h = hitsRef.current.get(win);
          if (!h || h.term !== highlight || h.cols !== cols) {
            h = { term: highlight, cols, hits: searchHits(win, cols, highlight) };
            hitsRef.current.set(win, h);
          }
          const local = row - cacheRef.current.pageOf(row) * cacheRef.current.pageSize;
          if (h.hits.has(local * cols.length + col)) saveBg = searchHitBg(resolved);
        }
      }
      // A SQL NULL prints a dimmed italic `NULL`; an empty string stays blank.
      const themeOverride =
        saveBg !== undefined
          ? { bgCell: saveBg, ...(isNull ? nullCellStyle(resolved) : {}) }
          : isNull
            ? nullCellStyle(resolved)
            : fk
              ? fkText
              : undefined;
      return {
        kind: GridCellKind.Text,
        // `data` is what the overlay editor opens with, so a NULL edits from empty.
        data: value ?? "",
        displayData: isNull ? "NULL" : (display ?? ""),
        allowOverlay: editable && !deleted && !committingRef.current,
        readonly: !editable || deleted,
        contentAlign: numeric ? "right" : undefined,
        themeOverride,
      };
    },
    [ensurePage, editable, fkByColumn, rightAlignColumns, resolved, rowBg, highlight, fkText, source.totalRows],
  );

  /** Build the full {col → value} map for `row` by walking columns through the
   *  cache + overlay. A SQL NULL stays `null` (never ""), so the Inspector and
   *  the copy/INSERT helpers can tell it apart from an empty string. Returns
   *  null if the row's page hasn't loaded yet.
   *
   *  `fromServer` skips the staged overlay: anything that identifies the row to
   *  the database (a PK in a WHERE clause) must use last-known-server values, or
   *  an uncommitted edit to a PK column would aim the write at another row. */
  const readRow = useCallback(
    (row: number, fromServer = false): Record<string, string | null> | null => {
      const cols = colNames.current;
      if (cols.length === 0) return null;
      const out: Record<string, string | null> = {};
      for (const name of cols) {
        const ov = fromServer ? undefined : overlay.current.get(`${row}:${name}`);
        const v = ov ?? cacheRef.current.getCell(row, name);
        if (v === undefined) return null; // page miss — caller should retry later
        out[name] = v;
      }
      return out;
    },
    [],
  );

  // Cell click: lift the row to the Inspector and (if applicable) fire FK navigation.
  const onCellClicked = useCallback(
    ([col, row]: Item) => {
      setSelRow(row);
      if (row >= source.totalRows) return; // a staged new row has no server row
      if (onRowSelected) {
        onRowSelected(row, readRow(row));
      }
      if (!onFkClick) return;
      const name = colNames.current[col];
      const fk = name ? fkByColumn?.get(name) : undefined;
      if (!fk) return;
      const value = name ? (overlay.current.get(`${row}:${name}`) ?? cacheRef.current.getCell(row, name)) : null;
      onFkClick(fk.to_table, fk.to_column, value ?? null);
    },
    [fkByColumn, onFkClick, onRowSelected, readRow, source.totalRows],
  );

  // Schedule a saved-tint clear, tracking the timer so a source switch / unmount
  // can cancel it. With motion on, the green then fades out on the canvas
  // instead of snapping off.
  const clearTintLater = useCallback(
    (key: string) => {
      const prev = editTimers.current.get(key);
      if (prev !== undefined) window.clearTimeout(prev);
      const id = window.setTimeout(
        () => {
          editTimers.current.delete(key);
          savedIds.current.delete(key);
          if (editState.current.get(key) === "saved") {
            editState.current.delete(key);
            if (!reducedMotion.current) {
              fx.current.set(key, { tint: "saved", start: performance.now(), hold: 0, dur: TINT_FADE });
              animate();
            }
            setVersion((v) => v + 1);
          }
        },
        reducedMotion.current ? SAVED_HOLD_REDUCED : SAVED_HOLD,
      );
      editTimers.current.set(key, id);
    },
    [animate],
  );

  /** Stage one cell edit (TablePlus batch model): mirror the new value into the
   *  overlay for display, record it in `pending`, and tint it blue. The DB write
   *  is deferred to {@link commitPending}. `before` (the last-saved value) is
   *  preserved across re-edits by the staging model so the optimistic match at
   *  commit still targets the DB value. */
  const applyCellEdit = useCallback(
    (row: number, name: string, newValue: string, pushUndo: boolean) => {
      if (!onCommitChanges || committingRef.current || deletes.current.has(row) || row >= source.totalRows) return;
      const key = pendingKey(row, name);
      const existing = pending.current.get(key);
      const priorOverlay = overlay.current.get(key);
      const before = priorOverlay ?? cacheRef.current.getCell(row, name) ?? null;
      // A NULL cell's editor opens empty, so leaving it empty means "no change" —
      // not "write ''". If the cell was already staged from NULL, clearing it back
      // to empty un-stages that edit instead of queueing a '' write.
      if (isNullLeftEmpty(existing ? existing.before : before, newValue)) {
        if (existing) {
          overlay.current.delete(key);
          editState.current.delete(key);
          pending.current = removePending(pending.current, row, name);
          notifyPending();
          setVersion((v) => v + 1);
        }
        return;
      }
      if (before === newValue && !pending.current.has(key)) return; // no-op
      overlay.current.set(key, newValue);
      pending.current = stageEdit(pending.current, row, name, newValue, before);
      editState.current.set(key, "staged");
      if (pushUndo) undoStack.current.push({ row, col: name, prev: before });
      notifyPending();
      setVersion((v) => v + 1);
    },
    [onCommitChanges, source.totalRows, notifyPending],
  );

  /** Repaint whole rows now: a staged tint lives in refs, which glide can't see change. */
  const repaintRows = useCallback((rows: number[]) => {
    const cols = colNames.current;
    gridRef.current?.updateCells(rows.flatMap((r) => cols.map((_, c) => ({ cell: [c, r] as Item }))));
  }, []);

  const stageDeleteRows = useCallback(
    (rows: number[]) => {
      const keyCols = pkRef.current ?? [];
      if (!onCommitChanges || committingRef.current || keyCols.length === 0) return;
      const added = rows.filter(
        (r) =>
          r >= 0 &&
          r < source.totalRows &&
          !deletes.current.has(r) &&
          cacheRef.current.getCell(r, keyCols[0]) !== undefined,
      );
      if (added.length === 0) return;
      for (const r of added) deletes.current.add(r);
      notifyPending();
      repaintRows(added);
      setVersion((v) => v + 1);
    },
    [onCommitChanges, source.totalRows, notifyPending, repaintRows],
  );

  const undoDelete = useCallback(
    (row: number) => {
      if (committingRef.current || !deletes.current.delete(row)) return;
      notifyPending();
      repaintRows([row]);
      setVersion((v) => v + 1);
    },
    [notifyPending, repaintRows],
  );

  const stageInsert = useCallback(
    (values: [string, string | null][]) => {
      if (!onCommitChanges || committingRef.current) return false;
      inserts.current = [...inserts.current, { values }];
      notifyPending();
      setVersion((v) => v + 1); // `rows` grows, so glide redraws
      return true;
    },
    [onCommitChanges, notifyPending],
  );

  const removeInsertedRow = useCallback(
    (row: number) => {
      const i = row - source.totalRows;
      if (committingRef.current || i < 0 || i >= inserts.current.length) return;
      inserts.current = inserts.current.filter((_, j) => j !== i);
      notifyPending();
      setVersion((v) => v + 1);
    },
    [source.totalRows, notifyPending],
  );

  /** ⌫ with rows selected (row-number clicks) stages them all for delete. Without a row
   *  selection glide's own delete (clear the selected cells) runs, as before. */
  const onGridDelete = useCallback(
    (sel: GridSelection) => {
      if (sel.rows.length === 0) return true;
      stageDeleteRows(sel.rows.toArray());
      return false;
    },
    [stageDeleteRows],
  );

  /** What's staged now. `serverValue` closes over THIS page's cache, so the batch keeps
   *  reading the page it was staged against. */
  const snapshotNow = useCallback((): StagedSnapshot => {
    const cache = cacheRef.current;
    return {
      edits: listPending(pending.current),
      deletes: [...deletes.current].sort((a, b) => a - b),
      inserts: [...inserts.current],
      serverValue: (r, c) => cache.getCell(r, c),
    };
  }, []);

  /** Commit everything staged in ONE `onCommitChanges` call (one transaction). Success:
   *  every staged change clears and `onSave` re-syncs the page. Failure: nothing was
   *  written, so everything stays staged for the user to fix or discard. */
  const doCommit = useCallback(async () => {
    if (!onCommitChanges) return;
    const snap = snapshotNow();
    const dead = new Set(snap.deletes);
    const n = totalStaged(stagedCounts(snap.edits, dead, snap.inserts.length));
    if (n === 0) return;
    // `row` indexes the CURRENT page. The parent locks sort / paging / refresh while
    // committing; the pinned generation is the backstop, exactly as in `ensurePage`.
    const gen = generation.current;
    const live = snap.edits.filter((e) => !dead.has(e.row));
    for (const e of live) editState.current.set(pendingKey(e.row, e.col), "saving");
    onCommittingChange?.(true);
    pulseStart.current = performance.now();
    animate();
    setVersion((v) => v + 1);
    let ok = false;
    try {
      ok = await onCommitChanges(snap);
    } catch {
      ok = false;
    } finally {
      onCommittingChange?.(false);
    }
    if (gen !== generation.current) return; // the page changed under the call
    if (!ok) {
      for (const e of live) editState.current.set(pendingKey(e.row, e.col), "staged");
      setVersion((v) => v + 1);
      return;
    }
    const keyCols = pkRef.current ?? [];
    for (const e of live) {
      const key = pendingKey(e.row, e.col);
      editState.current.set(key, "saved");
      const shown = readRow(e.row);
      if (shown && keyCols.length > 0) {
        savedIds.current.set(key, Object.fromEntries(keyCols.map((c) => [c, shown[c] ?? null])));
      }
      clearTintLater(key);
    }
    for (const e of snap.edits) if (dead.has(e.row)) editState.current.delete(pendingKey(e.row, e.col));
    pending.current = new Map();
    deletes.current = new Set();
    inserts.current = [];
    undoStack.current = [];
    notifyPending();
    setVersion((v) => v + 1);
    onSave?.(n);
  }, [onCommitChanges, snapshotNow, notifyPending, clearTintLater, onSave, onCommittingChange, animate, readRow]);

  /** {@link doCommit}, behind `confirmCommit` when one is set. Nothing staged means
   *  nothing to confirm, so an empty ⌘S never prompts; one already in flight (confirm
   *  open or `apply_changes` running) drops this call. */
  const commitPending = useCallback(async () => {
    if (!onCommitChanges) return;
    if (pending.current.size + deletes.current.size + inserts.current.length === 0) return;
    await exclusive(committingRef, () => gatedCommit(confirmCommitRef.current, doCommit));
  }, [onCommitChanges, doCommit]);

  /** Drop every staged change and revert the display to each cell's saved value. */
  const discardPending = useCallback(() => {
    for (const e of discardAll(pending.current)) {
      overlay.current.delete(pendingKey(e.row, e.col));
      editState.current.delete(pendingKey(e.row, e.col));
    }
    const undeleted = [...deletes.current];
    pending.current = new Map();
    deletes.current = new Set();
    inserts.current = [];
    undoStack.current = [];
    notifyPending();
    repaintRows(undeleted);
    setVersion((v) => v + 1);
  }, [notifyPending, repaintRows]);

  const onCellEdited = useCallback(
    ([col, row]: Item, newCell: EditableGridCell) => {
      if (!onCommitChanges || newCell.kind !== GridCellKind.Text) return;
      const name = colNames.current[col];
      if (!name) return;
      applyCellEdit(row, name, String(newCell.data), true);
    },
    [onCommitChanges, applyCellEdit],
  );

  /** ⌘Z — revert the most recent edit by re-applying its previous value. */
  const undoLastEdit = useCallback(() => {
    if (committingRef.current) return; // before the pop, or the entry is lost
    const last = undoStack.current.pop();
    if (!last || last.prev === null) return; // null restore can't go through the string edit path
    applyCellEdit(last.row, last.col, last.prev, false);
  }, [applyCellEdit]);

  // ⌘Z undo last edit · ⌘S re-sync the page from the DB (TablePlus idioms).
  useEffect(() => {
    if (!editable) return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      // Don't hijack ⌘Z/⌘S while the user is typing in the cell overlay editor or
      // any other input — let native text undo / form handling win there.
      const t = e.target as HTMLElement | null;
      if (
        t &&
        (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)
      )
        return;
      const k = e.key.toLowerCase();
      if (k === "z") {
        e.preventDefault();
        undoLastEdit();
      } else if (k === "s") {
        e.preventDefault();
        void commitPending();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [editable, undoLastEdit, commitPending]);

  useImperativeHandle(ref, () => ({
    scrollToRow: (row: number) => gridRef.current?.scrollTo(0, row, "vertical"),
    commitPending: () => void commitPending(),
    discardPending,
    snapshot: snapshotNow,
    stageDeleteRows,
    undoDelete,
    stageInsert,
    removeInsertedRow,
  }));

  // Update only the dragged column's width in local state so the resize feels
  // instant; the parent's persistence handler picks up the final value via
  // `onColumnResize`.
  const onLocalColumnResize = useCallback(
    (_col: GridColumn, newWidth: number, colIndex: number) => {
      // Key on the column *name*, not the title — the title carries the sort glyph.
      const name = colNames.current[colIndex];
      setColumns((cs) => cs.map((c, i) => (i === colIndex ? { ...c, width: newWidth } : c)));
      if (onColumnResize && name) onColumnResize(name, newWidth);
    },
    [onColumnResize],
  );

  // Zebra stripe on odd rows + a neutral highlight on the hovered row (TablePlus
  // parity). Both layer over `bgCell` via glide's per-row theme override. Defined
  // before the early return below so hook order stays stable across renders.
  const getRowThemeOverride = useCallback(
    (row: number) => {
      // Red wins over glide's row-selection fill too, so ⌫ on selected rows shows at once.
      if (deletes.current.has(row)) {
        const red = editTints(resolved).deleted;
        return { bgCell: red, accentLight: red };
      }
      // Only real staged-insert rows: glide also asks for the blank filler rows past the data.
      if (row >= source.totalRows) {
        return row < source.totalRows + inserts.current.length ? { bgCell: editTints(resolved).inserted } : undefined;
      }
      const t = gridRowTints(resolved);
      if (row === selRow) return { bgCell: t.selected };
      if (row === hoverRow) return { bgCell: t.hover };
      if (row % 2 === 1) return { bgCell: t.zebra };
      return undefined;
    },
    [resolved, hoverRow, selRow, source.totalRows],
  );

  if (columns.length === 0) {
    return <div className="p-6 text-sm text-muted">Loading…</div>;
  }

  return (
    <DataEditor
      ref={gridRef}
      columns={columns}
      rows={source.totalRows + inserts.current.length}
      rowHeight={ROW_HEIGHT}
      headerHeight={HEADER_HEIGHT}
      getRowThemeOverride={getRowThemeOverride}
      onItemHovered={(args) => {
        const r = args.kind === "cell" ? args.location[1] : undefined;
        setHoverRow((prev) => (prev === r ? prev : r));
      }}
      getCellContent={getCellContent}
      onCellEdited={editable ? onCellEdited : undefined}
      onCellClicked={onFkClick || onRowSelected ? onCellClicked : undefined}
      onVisibleRegionChanged={
        onVisibleRangeChange
          ? (r) => onVisibleRangeChange(r.y, r.y + r.height)
          : undefined
      }
      onCellContextMenu={
        onCellContext
          ? ([col, row], event) => {
              event.preventDefault();
              const name = colNames.current[col];
              if (!name) return;
              const value =
                overlay.current.get(`${row}:${name}`) ??
                cacheRef.current.getCell(row, name) ??
                null;
              onCellContext({
                col: name,
                row,
                value,
                values: readRow(row),
                serverValues: readRow(row, true),
                staged: row >= source.totalRows ? "inserted" : deletes.current.has(row) ? "deleted" : undefined,
                x: event.bounds.x + event.localEventX,
                y: event.bounds.y + event.localEventY,
              });
            }
          : undefined
      }
      onColumnResize={onLocalColumnResize}
      onHeaderClicked={
        onSort
          ? (colIndex) => {
              const name = colNames.current[colIndex];
              if (name) onSort(name);
            }
          : undefined
      }
      // Clicking a row number selects the row: the row selection ⌫ stages for delete.
      rowMarkers={editable ? "clickable-number" : "number"}
      rowMarkerStartIndex={rowMarkerStart}
      rowMarkerWidth={markerWidth(rowMarkerStart + source.totalRows + inserts.current.length - 1)}
      onDelete={editable ? onGridDelete : undefined}
      smoothScrollX
      smoothScrollY
      verticalBorder
      width="100%"
      height="100%"
      theme={gridTheme(resolved)}
    />
  );
});
