import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { DataGrid, type DataGridHandle, type GridSource } from "./DataGrid";
import {
  applyEdit,
  cancelQuery,
  countRowsExact,
  deleteRow,
  describeTable,
  downloadText,
  estimateRows,
  exportTable,
  fetchTableWindow,
  insertRow,
  isNotImplementedError,
  openTable,
  type ColumnDetail,
  type ColumnFilter,
  type ExportFormat,
  type FilterOp,
  type ForeignKey,
  type ObjectInfo,
  type SortKey,
  type TableStructure,
} from "../lib/commands";
import { buildEdit, writeOutcomeMessage } from "../lib/edit";
import { confirmDialog } from "../lib/confirmDialog";
import { outgoingForeignKeys } from "../lib/fk";
import { decodeWindow, type DecodedWindow } from "../lib/arrowWindow";
import { pageCursor } from "../lib/keyset";
import { pagerLabel } from "../lib/pager";
import { mergeTotal, staleTotal, type RowTotal } from "../lib/rowTotal";
import { isFindShortcut, isMacPlatform } from "../lib/shortcut";
import { cycleSort } from "../lib/tableSort";
import { tabGuard } from "../state/tabGuard";
import { useStaged } from "../state/stagedStore";
import { normalizeEnv } from "../lib/connEnv";
import { productionCommitMessage } from "../lib/stagedEdits";
import {
  invalidateTableMeta,
  loadForeignKeys,
  loadTableMeta,
  useConnections,
} from "../state/connectionStore";
import { useSelectedRow } from "../state/selectedRowStore";
import * as colWidths from "../lib/colWidths";
import { isNumericType } from "../lib/autoFit";
import { ColumnProfilePanel } from "./ColumnProfilePanel";
import { AlterTablePanel } from "./AlterTablePanel";
import type { Engine } from "../lib/commands";
import { Toolbar, ToolbarButton, ToolbarSpacer } from "./ui/Toolbar";
import { SegmentedToggle, StatusBar, StatusBarButton, StatusDot } from "./ui/StatusBar";
import { Chip } from "./ui/Chip";
import { Elapsed } from "./ui/Elapsed";
import { ContextMenu, type ContextMenuItem } from "./ui/ContextMenu";
import { buildRowAs } from "../lib/rowFormat";
import { buildDeleteStatement, quoteIdent } from "../lib/tableScript";
import { buildInsertValues, insertedPkMatch, isDraftDirty } from "../lib/insertRow";
import {
  chipLabel,
  fetchErrorMessage,
  FILTER_OP_LABEL,
  isNullOp,
  isSuperseded,
  retrySuperseded,
  MANUAL_FILTER_OPS,
  searchTerm,
  ANY_COLUMN,
  RAW_SQL,
  ruleToFilter,
  rulesToFilters,
  type FilterRule,
} from "../lib/filterBuilder";
import {
  IconChevronLeft,
  IconChevronRight,
  IconClose,
  IconExport,
  IconFilter,
  IconLink,
  IconRefresh,
} from "./ui/Icon";

/** Rows-per-page choices; one more row than the page is fetched — the extra row
 *  is how we know a next page exists without a second query. */
const PAGE_SIZES = [100, 300, 1000, 5000] as const;
const DEFAULT_PAGE_SIZE = 300;
let ruleSeq = 0;
/** A blank filter-bar row. Defaults to Any column — the old search box's job. */
const newRule = (column = ANY_COLUMN, op: FilterOp = "Eq", value = ""): FilterRule => ({
  id: ++ruleSeq,
  enabled: true,
  column,
  op,
  value,
});

interface Props {
  connId: string;
  schema: string;
  object: ObjectInfo;
  /** Which sub-view the tab is on. Owned by the tab so it survives tab switches. */
  view: "data" | "structure";
}

export function TableView({ connId, schema, object, view }: Props) {
  /** Unfiltered/filtered row total. `count: null` means "the engine could not
   *  count it" — which must never be rendered as 0 rows. */
  const [total, setTotal] = useState<RowTotal | null>(null);
  const [pk, setPk] = useState<string[] | null>(null);
  /** Why the primary key is unknown: the `primary_key` call itself failed. A
   *  failure must never read as "this table has no primary key" — that tells the
   *  user their rows can never be edited, which may be false. */
  const [pkError, setPkError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [fks, setFks] = useState<ForeignKey[]>([]);
  const [showProfile, setShowProfile] = useState(false);
  const [showAlter, setShowAlter] = useState(false);
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState<number>(DEFAULT_PAGE_SIZE);
  const [sort, setSort] = useState<SortKey | null>(null);
  /** The fetched window for the current page. `rows` is what the server returned
   *  (up to pageSize + 1); `version` bumps on every re-read so the grid can drop
   *  its cache without the full reset a new source key does. `page`, `pageKey`
   *  and `filters` say which request it answers — a stale window must never
   *  seed a keyset cursor or count a filter it wasn't fetched under. */
  const [pageData, setPageData] = useState<{
    bytes: Uint8Array;
    win: DecodedWindow;
    rows: number;
    version: number;
    page: number;
    size: number;
    pageKey: string;
    filters: ColumnFilter[];
  } | null>(null);
  /** A page fetch is in flight — drives the progress bar and the stale-grid dim. */
  const [fetching, setFetching] = useState(false);
  /** Keyset cursors: `map[p]` is the key of the last row of page p−1. Only valid
   *  for one filter + sort + page size (`key`); any change starts a new map. */
  const cursors = useRef<{ key: string; map: Map<number, (string | null)[]> }>({
    key: "",
    map: new Map(),
  });
  /** The filter bar, for focusing a row's value input. */
  const barRef = useRef<HTMLDivElement>(null);
  /** An exact COUNT(*) the user asked for, cancellable by `token`. */
  const [counting, setCounting] = useState<{ token: string; started: number } | null>(null);
  const countingRef = useRef<string | null>(null);
  const [dirty, setDirty] = useState(0);
  const [refreshKey, setRefreshKey] = useState(0);
  /** The applied filters — what the grid is fetched with. */
  const [filters, setFilters] = useState<ColumnFilter[]>([]);
  /** The filter bar's rows (TablePlus parity); only Apply turns them into `filters`. */
  const [rules, setRules] = useState<FilterRule[]>([]);
  const [barOpen, setBarOpen] = useState(false);
  const [draft, setDraft] = useState<Record<string, string | null | undefined> | null>(null);
  const [cellMenu, setCellMenu] = useState<{
    x: number;
    y: number;
    col: string;
    row: number;
    value: string | null;
    /** Row as displayed — includes staged, uncommitted edits. Copy/display only. */
    values: Record<string, string | null> | null;
    /** Row as the database still has it. The ONLY source for a WHERE clause. */
    serverValues: Record<string, string | null> | null;
  } | null>(null);
  const [pendingCount, setPendingCount] = useState(0);
  // True while the grid is walking its staged edits, one round trip each. Sorting or
  // paging mid-commit would change what the remaining edits' row indices mean, so both
  // are locked for the duration (the grid also stops itself — see its generation guard).
  const [committing, setCommitting] = useState(false);
  // "Saved n changes" in the status bar for 2s after a commit. `id` re-keys the
  // element so a second save inside the window restarts its fade.
  const [saved, setSaved] = useState<{ n: number; id: number } | null>(null);
  // Staged edits are keyed to rows of the page on screen, so anything that swaps
  // that page (search, filter, sort, paging, refresh) would silently drop them.
  // Those actions are refused until the edits are committed or discarded, and the
  // unsaved-changes bar says so. Not while the fetch errored: the grid is gone.
  const locked = !error && (committing || pendingCount > 0);
  const lockedRef = useRef(locked);
  lockedRef.current = locked;
  // Published so leaving this connection (rail, ⌘O, Disconnect) can ask first.
  const setEditsPending = useConnections((s) => s.setEditsPending);
  useEffect(() => {
    setEditsPending(locked);
    return () => setEditsPending(false);
  }, [locked, setEditsPending]);
  const [lockHint, setLockHint] = useState(false);
  useEffect(() => {
    if (!locked) setLockHint(false);
  }, [locked]);
  /** True — and shows the hint — when staged edits would be lost by this action. */
  const blockedByEdits = useCallback(() => {
    if (!lockedRef.current) return false;
    setLockHint(true);
    return true;
  }, []);
  const savedTimer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(savedTimer.current), []);
  const gridRef = useRef<DataGridHandle>(null);
  const engine = useConnections((s) => s.active?.engine ?? "postgres") as Engine;
  const setSelectedRow = useSelectedRow((s) => s.setRow);
  const clearSelectedRowIfFrom = useSelectedRow((s) => s.clearIfFrom);
  // Per-(conn, schema, table) column widths, hydrated from localStorage so a
  // user's drag survives relaunches (TablePlus parity).
  const [columnWidths, setColumnWidths] = useState<Record<string, number>>(() =>
    colWidths.load(connId, schema, object.name),
  );
  // Re-hydrate when the user switches tables; otherwise we'd keep applying the
  // previous table's widths to the new table's columns.
  useEffect(() => {
    setColumnWidths(colWidths.load(connId, schema, object.name));
  }, [connId, schema, object.name]);
  // Clear any prior selection when the user navigates to a different table — the
  // Inspector should fall back to "table metadata" mode until the user clicks a
  // row in the new table.
  useEffect(() => {
    clearSelectedRowIfFrom(schema, object.name);
    useSelectedRow.setState({ row: null });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [schema, object.name]);

  /** Re-read this table: its cached count is stale after any write, and the
   *  visible page has to come back from the server. Deliberately leaves
   *  `pageData` in place so the grid keeps painting (and keeps its scroll and
   *  selection) while the new window is in flight. */
  const refresh = useCallback(() => {
    invalidateTableMeta(connId, schema, object.name);
    setRefreshKey((k) => k + 1);
  }, [connId, schema, object.name]);

  /** Changing the filter set always returns to the first page. */
  const applyFilters = useCallback(
    (next: ColumnFilter[]) => {
      if (blockedByEdits()) return;
      setFilters(next);
      setPage(0);
    },
    [blockedByEdits],
  );

  // The first page has painted. The PK + count (`loadTableMeta`) and FKs wait
  // for it, so the page never queues behind a COUNT(*).
  const painted = pageData !== null;

  // Primary key + FK edges. Both come from the connection-store caches, shared
  // with the Inspector.
  useEffect(() => {
    if (!painted) return;
    let cancelled = false;
    setNotice(null);
    setDirty(0);
    setPkError(null);
    loadTableMeta(connId, schema, object.name)
      .then((m) => {
        if (cancelled) return;
        setPk(m.pk);
        setPkError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        // Leave `pk` unknown (null) rather than claiming an empty key.
        setPk(null);
        setPkError(String(e));
        setNotice(`Couldn't read this table's primary key, so editing is off: ${e}`);
      });
    loadForeignKeys(connId)
      .then((all) => !cancelled && setFks(outgoingForeignKeys(all, schema, object.name)))
      .catch(() => !cancelled && setFks([]));
    return () => {
      cancelled = true;
    };
  }, [connId, schema, object.name, refreshKey, painted]);

  // A different table: its total starts from nothing.
  useEffect(() => setTotal(null), [connId, schema, object.name]);
  // Row total, step 1: the catalog estimate, instant (never a scan). Shown
  // until the real count below lands; a filter has no estimate. Until either
  // lands, the previous filters' total stays up, dimmed — not a blank label.
  useEffect(() => {
    let cancelled = false;
    setTotal(staleTotal);
    if (filters.length === 0) {
      estimateRows(connId, schema, object.name)
        .then((n) => {
          if (!cancelled && n !== null) setTotal((t) => mergeTotal(t, { count: n, exact: false }));
        })
        .catch(() => {});
    }
    return () => {
      cancelled = true;
    };
  }, [connId, schema, object.name, filters]);

  // Row total, step 2, once this filter's first page has painted: `open_table`
  // (exact, or the estimate again for a huge table — it never scans those). A
  // count that fails is NOT fatal: the page is the authority on what to render,
  // so the label keeps the estimate or drops the "of …" half.
  const pageReady = pageData?.filters === filters;
  useEffect(() => {
    if (!pageReady) return;
    let cancelled = false;
    const counted =
      filters.length === 0
        ? loadTableMeta(connId, schema, object.name).then((m) => ({
            count: m.count,
            exact: m.exact,
          }))
        : openTable(connId, schema, object.name, filters);
    // mergeTotal: a refresh after an edit re-reads the estimate, which must not
    // replace an exact count the user asked for.
    counted
      .then((t) => !cancelled && setTotal((prev) => mergeTotal(prev, t)))
      .catch(() => !cancelled && setTotal((prev) => mergeTotal(prev, { count: null, exact: false })));
    return () => {
      cancelled = true;
    };
  }, [connId, schema, object.name, filters, refreshKey, pageReady]);

  /** "Count exactly": an unbounded COUNT(*), cancellable server-side. */
  const cancelCount = useCallback(() => {
    const token = countingRef.current;
    if (!token) return;
    countingRef.current = null;
    setCounting(null);
    cancelQuery(connId, token).catch(() => {});
  }, [connId]);
  const startCount = useCallback(() => {
    const token = `count-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    countingRef.current = token;
    setCounting({ token, started: performance.now() });
    countRowsExact(connId, schema, object.name, token, filters)
      .then((t) => {
        if (countingRef.current !== token) return;
        countingRef.current = null;
        setCounting(null);
        setTotal(t);
      })
      .catch((e) => {
        if (countingRef.current !== token) return;
        countingRef.current = null;
        setCounting(null);
        if (!isSuperseded(String(e))) setNotice(`Count failed: ${e}`);
      });
  }, [connId, schema, object.name, filters]);
  // A running count answers for one table + filter set: cancel it on any change,
  // and on unmount (closing or switching the tab).
  useEffect(() => cancelCount, [cancelCount, filters, refreshKey]);

  const order = useMemo(() => (sort ? [sort] : undefined), [sort]);
  const orderKey = sort ? `${sort.column}:${sort.desc}` : "";
  const filterKey = filters.map((f) => `${f.column}${f.op}${f.value ?? ""}`).join("|");
  /** What a keyset cursor is valid for: same filters, sort and page size. */
  const pageKey = `${filterKey}.${orderKey}.${pageSize}`;
  /** Same key ⇒ the backend cancels this tab's previous in-flight page fetch. */
  const requestKey = `table:${connId}:${schema}.${object.name}`;

  // The visible page. pageSize + 1 rows are asked for: the extra row is how we
  // know a next page exists without counting. Deep pages seek by keyset cursor
  // where the backend can (offset is always sent too, so it stays correct).
  useEffect(() => {
    let cancelled = false;
    if (cursors.current.key !== pageKey) cursors.current = { key: pageKey, map: new Map() };
    const after = cursors.current.map.get(page);
    setFetching(true);
    retrySuperseded(
      () =>
        fetchTableWindow(
          connId,
          schema,
          object.name,
          page * pageSize,
          pageSize + 1,
          filters,
          order,
          after,
          requestKey,
        ),
      () => cancelled,
    )
      .then(({ bytes }) => {
        if (cancelled) return;
        setError(null);
        // ponytail: the window is decoded here (row count, cursor) and again in
        // the grid for its cells. Two decodes of one page are cheaper than a
        // second round trip; upgrade path is to hand the grid this window.
        const win = decodeWindow(bytes);
        setPageData((prev) => ({
          bytes,
          win,
          rows: win.numRows,
          version: (prev?.version ?? 0) + 1,
          page,
          size: pageSize,
          pageKey,
          filters,
        }));
      })
      .catch((e) => {
        // A newer fetch for this tab replaced this one; that one reports. Still
        // current here means a stale fetch kept superseding it: say so.
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setFetching(false);
      });
    return () => {
      cancelled = true;
    };
  }, [connId, schema, object.name, page, pageSize, pageKey, filters, order, requestKey, refreshKey]);

  /** Focus a filter-bar row's value input (the first row's by default), once
   *  the render that adds the row has landed. */
  const focusRule = useCallback((id?: number) => {
    requestAnimationFrame(() => {
      const row = id === undefined ? "" : `[data-rule="${id}"] `;
      barRef.current?.querySelector<HTMLInputElement>(`${row}input[type=text]`)?.select();
    });
  }, []);
  /** Show the filter bar with at least one row, and focus it. */
  const openBar = useCallback(() => {
    setBarOpen(true);
    setRules((r) => (r.length ? r : [newRule()]));
    focusRule();
  }, [focusRule]);
  /** Hide the filter bar, dropping its rows and whatever they applied. */
  const closeBar = useCallback(() => {
    if (filters.length) {
      if (blockedByEdits()) return;
      applyFilters([]);
    }
    setRules([]);
    setBarOpen(false);
  }, [filters.length, blockedByEdits, applyFilters]);
  // ⌘F (Ctrl+F off macOS) opens the filter bar — only the active tab's view is
  // mounted. Not while a modal / panel is open over it: that owns the keyboard.
  useEffect(() => {
    if (view !== "data") return;
    const mac = isMacPlatform();
    const onKey = (e: KeyboardEvent) => {
      if (isFindShortcut(e, mac) && !document.querySelector(".tiq-overlay")) {
        e.preventDefault();
        openBar();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [view, openBar]);

  // Re-sync the grid when a cell is edited from the Inspector panel (Slice 33),
  // so the data grid reflects the committed value.
  useEffect(() => {
    const onEdited = (e: Event) => {
      const target = (e as CustomEvent<string>).detail;
      if (target === `${schema}.${object.name}`) refresh();
    };
    window.addEventListener("tiq:row-edited", onEdited);
    return () => window.removeEventListener("tiq:row-edited", onEdited);
  }, [schema, object.name, refresh]);

  const editable = object.kind === "Table" && !!pk && pk.length > 0;

  const onEdit = useCallback(
    async (
      _row: number,
      col: string,
      newValue: string,
      getOriginal: (c: string) => string | null,
    ): Promise<boolean> => {
      if (!pk || pk.length === 0) return false;
      setNotice(null);
      setDirty((d) => d + 1);
      try {
        // Inside the try: a throw here would otherwise leave "1 saving…" forever.
        const edit = buildEdit(pk, getOriginal, col, newValue);
        const { rowsAffected } = await applyEdit(connId, schema, object.name, edit);
        setDirty((d) => Math.max(0, d - 1));
        // Only rows_affected === 1 is a clean write. 0 and >1 are both the user's
        // business and are reported verbatim — nothing is rolled back (the backend
        // UPDATE is a single non-transactional statement), so we never claim it was.
        const problem = writeOutcomeMessage("update", rowsAffected);
        if (!problem) return true;
        setNotice(`Update "${col}": ${problem}`);
        return false;
      } catch (e) {
        setNotice(`Update failed: ${e}`);
        setDirty((d) => Math.max(0, d - 1));
        return false;
      }
    },
    [connId, schema, object.name, pk],
  );

  // Build the FK lookup the grid uses to render the cell-level "go to referenced row" chevron.
  const fkByColumn = useMemo(() => {
    const m = new Map<string, ForeignKey>();
    for (const fk of fks) m.set(fk.from_column, fk);
    return m;
  }, [fks]);

  // Numeric columns render right-aligned in the grid (TablePlus parity). Type
  // classification lives in the tested `isNumericType` helper.
  const rightAlignColumns = useMemo(() => {
    const s = new Set<string>();
    for (const c of object.columns) if (isNumericType(c.data_type)) s.add(c.name);
    return s;
  }, [object.columns]);

  // Stable identity so a re-read (which rebuilds `source`) doesn't look like a
  // different column set to the grid, which would cost scroll + selection.
  const columnNames = useMemo(() => object.columns.map((c) => c.name), [object.columns]);

  // Everything the grid shows is read off `pageData` — the page on screen — not
  // off `page`/`pageSize`, which move the moment a new fetch starts. The stale
  // page stays put (dimmed) until the new one lands.
  /** Rows on the visible page — at most its page size, whatever the look-ahead row says. */
  const rowsOnPage = pageData ? Math.min(pageData.size, pageData.rows) : 0;
  const hasNext = !!pageData && pageData.rows > pageData.size;
  const firstRow = pageData ? pageData.page * pageData.size : 0;

  // The titlebar's Discard / Review / Commit (G2) act on this tab's staged edits.
  // `rowOffset` is `firstRow` (the page on screen): `PendingEdit.row` indexes that page.
  const publishStaged = useStaged((s) => s.publish);
  const unregisterStaged = useStaged((s) => s.unregister);
  useEffect(() => {
    publishStaged(requestKey, pendingCount, committing, {
      list: () => gridRef.current?.listPending() ?? [],
      commit: () => gridRef.current?.commitPending(),
      discard: () => gridRef.current?.discardPending(),
      rowOffset: firstRow,
    });
  }, [requestKey, pendingCount, committing, publishStaged, firstRow]);
  useEffect(() => () => unregisterStaged(requestKey), [requestKey, unregisterStaged]);

  // Production connections confirm every commit (⌘S, the in-grid and titlebar Commit).
  const env = useConnections((s) => normalizeEnv(s.saved.find((c) => c.id === connId)?.env));
  const confirmCommit = useMemo(
    () =>
      env === "production"
        ? async () => {
            const name =
              useConnections.getState().saved.find((c) => c.id === connId)?.name ?? "production";
            return confirmDialog(productionCommitMessage(pendingCount, name), "Production", {
              ok: "Commit to production",
              cancel: "Cancel",
            });
          }
        : undefined,
    [env, connId, pendingCount],
  );

  const source = useMemo<GridSource | null>(() => {
    if (!pageData) return null;
    return {
      // Filters, sort, page size and page identify the rows; a re-read of the
      // same rows moves `version` instead, which keeps scroll and selection.
      key: `${connId}.${schema}.${object.name}.${pageData.page}.${pageData.pageKey}`,
      totalRows: Math.min(pageData.size, pageData.rows),
      // One cache page holds the whole fetched window, so the grid never pages.
      pageSize: pageData.size + 1,
      version: pageData.version,
      columns: columnNames,
      // This view already fetched the window (it needs the look-ahead row), so
      // the grid reads the bytes we hold rather than asking again.
      fetch: () => Promise.resolve({ bytes: pageData.bytes, ms: 0 }),
    };
  }, [connId, schema, object.name, columnNames, pageData]);

  /** Go to page `p`. Moving forward records the keyset cursor for it — the key
   *  of the displayed page's last row — so the backend can seek instead of
   *  skipping `offset` rows; going back reuses the cursor stored on the way. */
  const goToPage = (p: number) => {
    if (blockedByEdits()) return;
    if (pageData && p === pageData.page + 1 && pageData.pageKey === cursors.current.key) {
      const c = pageCursor(pageData.win, pageData.size, pk);
      if (c) cursors.current.map.set(p, c);
    }
    setPage(p);
    gridRef.current?.scrollToRow(0);
  };

  const commitDraft = useCallback(async () => {
    if (!draft) return;
    setNotice(null);
    const values = buildInsertValues(
      object.columns.map((c) => c.name),
      draft,
    );
    try {
      await insertRow(connId, schema, object.name, values);
      setDraft(null);
      // Flash the new row once the refetch lands — only if the user typed its PK.
      const match = insertedPkMatch(pk ?? [], values);
      if (match) gridRef.current?.flashRowWhere(match);
      refresh();
    } catch (e) {
      setNotice(`Insert failed: ${e}`);
    }
  }, [connId, schema, object, draft, pk, refresh]);

  const cancelDraft = useCallback(() => {
    setDraft(null);
    setNotice(null);
  }, []);

  /** Filter bar (TablePlus parity): one row per rule. Enter / Apply applies that
   *  row alone; ⌘↵ / Apply All applies every ticked row. Declared before the early
   *  returns so the error view can show it too — a filter that the engine rejects
   *  has to stay visible and removable, or the tab is stuck. */
  const setRule = (id: number, patch: Partial<FilterRule>) =>
    setRules((rs) => rs.map((r) => (r.id === id ? { ...r, ...patch } : r)));
  const applyRule = (r: FilterRule) => {
    const f = ruleToFilter({ ...r, enabled: true });
    setRule(r.id, { enabled: true });
    applyFilters(f ? [f] : []);
  };
  const applyAll = () => applyFilters(rulesToFilters(rules));
  const ctl =
    "h-6 rounded-md border border-border bg-surface px-1.5 text-[11px] text-text focus:border-primary focus:outline-none";
  const btn = "h-6 rounded-md border border-border px-2 text-[11px] text-muted hover:text-text";
  const filterRail =
    barOpen || filters.length > 0 ? (
      <div
        ref={barRef}
        className="tiq-rail flex flex-col gap-1 border-b border-border bg-bg px-3 py-1.5"
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            e.stopPropagation();
            applyAll();
          }
        }}
      >
        {rules.map((r) => {
          const raw = r.column === RAW_SQL;
          return (
            <div key={r.id} data-rule={r.id} className="flex items-center gap-1">
              <input
                type="checkbox"
                checked={r.enabled}
                onChange={(e) => setRule(r.id, { enabled: e.target.checked })}
                aria-label="Include in Apply All"
                title="Include in Apply All"
                className="accent-[var(--primary)]"
              />
              <select
                value={r.column}
                onChange={(e) => setRule(r.id, { column: e.target.value })}
                aria-label="Filter column"
                className={`${ctl} max-w-44`}
              >
                {object.columns.map((c) => (
                  <option key={c.name} value={c.name}>
                    {c.name}
                  </option>
                ))}
                <option disabled>──────────</option>
                <option value={ANY_COLUMN}>Any column</option>
                <option value={RAW_SQL}>Raw SQL</option>
              </select>
              {r.column === ANY_COLUMN ? (
                <span className="w-16 text-center text-[11px] text-muted">contains</span>
              ) : (
                !raw && (
                  <select
                    value={r.op}
                    onChange={(e) => setRule(r.id, { op: e.target.value as FilterOp })}
                    aria-label="Filter operator"
                    className={ctl}
                  >
                    {MANUAL_FILTER_OPS.map((op) => (
                      <option key={op} value={op}>
                        {FILTER_OP_LABEL[op]}
                      </option>
                    ))}
                  </select>
                )
              )}
              {raw || !isNullOp(r.op) || r.column === ANY_COLUMN ? (
                <input
                  type="text"
                  value={r.value}
                  onChange={(e) => setRule(r.id, { value: e.target.value })}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.metaKey && !e.ctrlKey) {
                      e.preventDefault();
                      applyRule(r);
                    } else if (e.key === "Escape" && filters.length === 0) {
                      // Nothing applied: Esc cancels the bar, as it cancelled the old draft.
                      e.preventDefault();
                      e.stopPropagation();
                      closeBar();
                    }
                  }}
                  aria-label={raw ? "Raw SQL WHERE expression" : "Filter value"}
                  spellCheck={false}
                  placeholder={raw ? "WHERE … e.g. id > 10 AND name LIKE 'a%'" : r.column === ANY_COLUMN ? "Search rows" : "value…"}
                  className={`${ctl} min-w-0 flex-1 placeholder:text-muted ${raw ? "font-mono" : ""}`}
                />
              ) : (
                <span className="flex-1" />
              )}
              <button onClick={() => applyRule(r)} className={btn} title="Apply this filter only (↵)">
                Apply
              </button>
              <button
                onClick={() => (rules.length > 1 ? setRules((rs) => rs.filter((x) => x.id !== r.id)) : closeBar())}
                className={`${btn} w-6 px-0`}
                aria-label="Remove filter row"
                title="Remove filter row"
              >
                −
              </button>
              <button
                onClick={() => {
                  const next = newRule(r.column === RAW_SQL ? ANY_COLUMN : r.column, r.op);
                  setRules((rs) => rs.flatMap((x) => (x.id === r.id ? [x, next] : [x])));
                  focusRule(next.id);
                }}
                className={`${btn} w-6 px-0`}
                aria-label="Add filter row"
                title="Add filter row"
              >
                +
              </button>
            </div>
          );
        })}
        <div className="flex items-center gap-1.5">
          <IconFilter size={11} className="text-primary" />
          <span className="text-[10px] text-muted">
            {filters.length > 0 ? filters.map(chipLabel).join(" AND ") : "no filter applied"}
          </span>
          <span className="flex-1" />
          <button
            onClick={() => {
              if (filters.length) {
                if (blockedByEdits()) return;
                applyFilters([]);
              }
              setRules([newRule()]);
              focusRule();
            }}
            className={btn}
          >
            Clear
          </button>
          <button
            onClick={applyAll}
            className="h-6 rounded-md bg-primary px-2 text-[11px] font-medium text-white hover:opacity-90"
            title="Apply every ticked filter"
          >
            Apply All <span className="opacity-70">{isMacPlatform() ? "⌘↵" : "Ctrl ↵"}</span>
          </button>
          <button onClick={closeBar} className={`${btn} w-6 px-0`} aria-label="Close filter bar" title="Close filter bar">
            <IconClose size={9} className="mx-auto" />
          </button>
        </div>
      </div>
    ) : null;

  // Loading / error states still render the table chrome's bottom bar so the
  // window never flashes a bar-less state (MainShell suppresses its fallback bar
  // for table tabs, trusting this view to always own one — TablePlus parity).
  const statusObjectInfo = (
    <span className="font-mono text-text">
      <span className="text-muted">{schema}.</span>
      <span className="font-medium">{object.name}</span>
    </span>
  );
  if (error) {
    // A timed-out filter or search has a way forward; say that instead.
    const friendly = fetchErrorMessage(error, filters);
    return (
      <div className="flex h-full flex-col">
        {filterRail}
        <div className="flex-1 space-y-3 p-6">
          <div className={`text-sm ${friendly === error ? "font-mono text-danger" : "text-warn"}`}>
            {friendly === error ? `Failed to open ${object.name}: ${error}` : friendly}
          </div>
          {filters.length > 0 && friendly === error && (
            <p className="text-[11.5px] text-muted">
              {filters.length === 1 ? "A filter is" : "Filters are"} applied. If the engine
              rejected {filters.length === 1 ? "it" : "them"}, clear{" "}
              {filters.length === 1 ? "it" : "them"} to get back to the table.
            </p>
          )}
          <div className="flex items-center gap-2">
            {filters.length > 0 && (
              <button
                onClick={() => applyFilters([])}
                className="rounded-md bg-primary px-2.5 py-1 text-[11px] font-medium text-white hover:opacity-90"
              >
                Clear filters
              </button>
            )}
            <button
              onClick={refresh}
              className="rounded-md border border-border px-2.5 py-1 text-[11px] text-muted hover:text-text"
            >
              Try again
            </button>
          </div>
        </div>
        <StatusBar
          left={statusObjectInfo}
          right={
            <span className="inline-flex items-center gap-1 text-danger">
              <StatusDot tone="danger" />
              <span>error</span>
            </span>
          }
        />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* Compact action toolbar — Filter/Insert/Structure now live in the status bar
          (TablePlus parity). This row keeps the secondary actions: Refresh, Profile,
          Alter, and export. */}
      <Toolbar>
        <ToolbarButton
          icon={<IconRefresh size={12} className={fetching ? "animate-spin" : undefined} />}
          label="Refresh"
          onClick={() => !blockedByEdits() && refresh()}
        />
        <ToolbarButton label="Profile" onClick={() => setShowProfile(true)} />
        {object.kind === "Table" && (
          <ToolbarButton label="Alter" onClick={() => setShowAlter(true)} />
        )}
        <ToolbarSpacer />
        {(["csv", "json"] as ExportFormat[]).map((fmt) => (
          <ToolbarButton
            key={fmt}
            icon={<IconExport size={12} />}
            label={fmt.toUpperCase()}
            onClick={async () => {
              try {
                const text = await exportTable(connId, schema, object.name, fmt, filters, order);
                await downloadText(
                  `${object.name}.${fmt}`,
                  text,
                  fmt === "json" ? "application/json" : "text/csv",
                );
              } catch (e) {
                setNotice(`Export failed: ${e}`);
              }
            }}
          />
        ))}
      </Toolbar>

      {filterRail}

      {/* Draft-row rail (Slice 21 #2). Editable inputs for each column; Enter commits. */}
      {draft !== null && editable && (
        <div className="tiq-rail flex items-center gap-2 border-b border-border bg-bg px-3 py-1.5">
          <span className="text-[10px] uppercase tracking-wider text-primary">new row</span>
          <div className="flex flex-wrap gap-1.5">
            {object.columns.map((c) => (
              <input
                key={c.name}
                value={draft[c.name] ?? ""}
                onChange={(e) =>
                  setDraft({ ...draft, [c.name]: e.target.value === "" ? "" : e.target.value })
                }
                onKeyDown={(e) => {
                  // Same rule as the Commit button: an untouched draft inserts nothing.
                  if (e.key === "Enter" && isDraftDirty(draft)) void commitDraft();
                  if (e.key === "Escape") cancelDraft();
                }}
                placeholder={c.name}
                title={`${c.name} · ${c.data_type}${c.nullable ? " · null ok" : ""}`}
                className="w-32 rounded-md border-l-2 border-l-primary border-y border-r border-border bg-surface px-2 py-0.5 text-[11px] text-text placeholder:text-muted"
              />
            ))}
          </div>
          <button
            disabled={!isDraftDirty(draft)}
            onClick={() => void commitDraft()}
            className="rounded-md bg-primary px-2.5 py-0.5 text-[11px] font-medium text-white hover:opacity-90 disabled:opacity-40"
          >
            Commit ↵
          </button>
          <button
            onClick={cancelDraft}
            className="rounded-md border border-border px-2 py-0.5 text-[11px] text-muted hover:text-text"
          >
            Cancel
          </button>
        </div>
      )}

      {/* FK indicator — subtler than the prior chip rail. Only renders when this
          table has outgoing FKs; cells with an FK column now render a chevron in the
          grid (DataGrid handles that), so the rail just lists targets at a glance. */}
      {fks.length > 0 && (
        <div className="flex items-center gap-1.5 border-b border-border bg-bg px-3 py-1 text-[10.5px] text-muted">
          <IconLink size={10} className="text-muted-2" />
          <span>references</span>
          {fks.map((fk, i) => (
            <button
              key={`${fk.from_column}->${fk.to_table}`}
              onClick={() => void tabGuard.openTable(fk.to_schema ?? schema, fk.to_table)}
              title={`${fk.from_column} → ${fk.to_schema ?? schema}.${fk.to_table}.${fk.to_column}`}
              className="text-text/80 transition-colors hover:text-primary"
            >
              {fk.to_table}
              {i < fks.length - 1 ? <span className="ml-1 text-muted-2">·</span> : null}
            </button>
          ))}
        </div>
      )}

      {/* Staged-edits Commit/Discard bar — TablePlus batch model. Appears whenever
          there are uncommitted cell edits; Commit (⌘S) writes them all, Discard
          reverts the display. */}
      {pendingCount > 0 && (
        <div className="tiq-rail flex items-center gap-2 border-b border-l-2 border-l-primary border-border bg-primary-soft px-3 py-1.5">
          <span className="text-[11px] font-medium text-primary">
            {pendingCount} unsaved {pendingCount === 1 ? "change" : "changes"}
          </span>
          {lockHint && (
            <span className="tiq-slide-in text-[11px] text-warn" role="status">
              Commit or discard changes first
            </span>
          )}
          <ToolbarSpacer />
          <button
            onClick={() => gridRef.current?.discardPending()}
            disabled={committing}
            className="rounded-md border border-border px-2 py-0.5 text-[11px] text-muted hover:text-text disabled:opacity-40"
          >
            Discard
          </button>
          <button
            onClick={() => gridRef.current?.commitPending()}
            disabled={committing}
            className="inline-flex items-center gap-1 rounded-md bg-primary px-2.5 py-0.5 text-[11px] font-medium text-white hover:opacity-90 disabled:opacity-70"
          >
            {committing ? (
              <>
                <IconRefresh size={10} className="animate-spin" /> Saving…
              </>
            ) : (
              "Commit ⌘S"
            )}
          </button>
        </div>
      )}

      {/* Read-only hint — explains WHY cells can't be edited, at the point of
          interaction (the bottom-bar dot alone left "editing does nothing"
          feeling like a bug). Only in data view, once the PK has loaded. */}
      {view === "data" && !editable && pk !== null && (
        <div className="flex items-center gap-1.5 border-b border-border bg-bg px-3 py-1 text-[10.5px] text-muted">
          <StatusDot tone="muted" />
          {object.kind !== "Table" ? (
            <span>Read-only — a {object.kind.toLowerCase()} can't be edited.</span>
          ) : (
            <span>Read-only — this table has no primary key, so cells can't be edited.</span>
          )}
        </div>
      )}

      {/* Body. While a page is in flight: a progress bar on the top edge and the
          stale page dimmed (both only after 150ms, so fast pages don't flicker) but
          still live — scrolling, selecting and copying never freeze on a fetch.
          Before the first page: an empty grid shell under the same bar. */}
      <div className="relative min-h-0 flex-1">
        {view === "data" && fetching && (
          <div className="tiq-progress absolute inset-x-0 top-0 z-10" role="progressbar" aria-label="Loading rows" />
        )}
        {view === "data" && !source ? (
          <div className="h-[38px] border-b border-border bg-surface" aria-hidden />
        ) : view === "data" && source ? (
          <div
            className={`tiq-dimmable h-full ${fetching ? "is-busy" : ""}`}
          >
          <DataGrid
            ref={gridRef}
            source={source}
            rowMarkerStart={firstRow + 1}
            highlight={searchTerm(pageData?.filters ?? [])}
            pk={pk}
            confirmCommit={confirmCommit}
            // Read-only while a page is in flight: the grid stays live for
            // scrolling and copying, but an edit staged now would be wiped when
            // the new page lands.
            onEdit={editable && !fetching ? onEdit : undefined}
            fkByColumn={fkByColumn}
            onFkClick={(toTable) => {
              const fk = fks.find((f) => f.to_table === toTable);
              void tabGuard.openTable(fk?.to_schema ?? schema, toTable);
            }}
            onCellContext={(c) => setCellMenu(c)}
            onRowSelected={(row, values) => {
              if (!values) return; // page not loaded yet — ignore the click
              setSelectedRow({ schema, table: object.name, row: firstRow + row, values });
            }}
            columnWidths={columnWidths}
            onColumnResize={(col, w) => {
              const next = colWidths.update(connId, schema, object.name, col, w);
              setColumnWidths(next);
            }}
            rightAlignColumns={rightAlignColumns}
            onSave={(n) => {
              window.clearTimeout(savedTimer.current);
              setSaved((s) => ({ n, id: (s?.id ?? 0) + 1 }));
              savedTimer.current = window.setTimeout(() => setSaved(null), 2000);
              refresh();
            }}
            onPendingCountChange={setPendingCount}
            onCommittingChange={setCommitting}
            onFetchError={setError}
            sort={sort}
            onSort={(column) => {
              // A re-sort would re-point the staged rows (and, mid-commit, the writes).
              if (blockedByEdits()) return;
              setSort((cur) => cycleSort(cur, column));
              setPage(0);
            }}
          />
          </div>
        ) : (
          <StructureView connId={connId} schema={schema} object={object} fks={fks} />
        )}
      </div>

      {/* TablePlus-style status bar — decluttered (Slice 33). The object identity
          (schema.table · engine · db) already lives in the top connection capsule,
          so the bottom bar carries ONLY controls:
          Left  : Data/Structure toggle · + Row  (primary actions, grouped)
          Center: row-window pager + any transient edit notice
          Right : Filters · Columns · editable dot */}
      <StatusBar
        left={
          <>
            <SegmentedToggle
              options={[
                { value: "data", label: "Data" },
                { value: "structure", label: "Structure" },
              ]}
              value={view}
              onChange={(v) => {
                if (v !== view && !blockedByEdits()) void tabGuard.openTable(schema, object.name, v);
              }}
            />
            <StatusBarButton
              onClick={() => setDraft(draft ? null : {})}
              disabled={!editable}
              active={draft !== null}
              title="Insert a new row"
            >
              + Row
            </StatusBarButton>
          </>
        }
        center={
          notice ? (
            // The status bar truncates; keep the whole message reachable on hover.
            <span className="truncate text-warn" title={notice}>
              {notice}
            </span>
          ) : !pageData ? (
            <span className="text-muted">Opening…</span>
          ) : (
            <>
              {/* Steps from the page on screen, not `page` (already moved while a
                  fetch is in flight) — `hasNext` only speaks for the page shown. */}
              <RowPager
                label={pagerLabel(firstRow, rowsOnPage, total?.count ?? null, total?.exact ?? false)}
                stale={total?.stale}
                hasPrev={pageData.page > 0 && !committing}
                hasNext={hasNext && !committing}
                onPrev={() => goToPage(Math.max(0, pageData.page - 1))}
                onNext={() => goToPage(pageData.page + 1)}
              />
              {counting ? (
                <span className="inline-flex items-center gap-1 text-muted" role="status">
                  Counting…
                  {/* Fixed width, so the ticking time doesn't nudge the × out from under the pointer;
                      aria-hidden, so the live region announces "Counting…" once, not every tick. */}
                  <Elapsed since={counting.started} aria-hidden className="inline-block min-w-[6ch] tabular-nums" />
                  <StatusBarButton onClick={cancelCount} title="Stop counting">
                    <IconClose size={9} />
                    <span className="sr-only">Stop counting</span>
                  </StatusBarButton>
                </span>
              ) : (
                total &&
                !total.exact && (
                  <StatusBarButton
                    onClick={startCount}
                    title="The total is an estimate. Run an exact COUNT(*) — it can take a while on a big table."
                  >
                    Count exactly
                  </StatusBarButton>
                )
              )}
              <select
                aria-label="Rows per page"
                title="Rows per page"
                value={pageSize}
                disabled={committing}
                onChange={(e) => {
                  if (blockedByEdits()) return;
                  setPageSize(Number(e.target.value));
                  setPage(0);
                  gridRef.current?.scrollToRow(0);
                }}
                className="h-[18px] rounded-[var(--radius-sm)] border border-border bg-bg px-1 text-[11px] text-muted hover:text-text disabled:opacity-40"
              >
                {PAGE_SIZES.map((n) => (
                  <option key={n} value={n}>
                    {n.toLocaleString("en-US")} / page
                  </option>
                ))}
              </select>
            </>
          )
        }
        right={
          <>
            {dirty > 0 ? (
              <span className="text-warn">
                {dirty} saving…
              </span>
            ) : (
              saved && (
                <span key={saved.id} className="tiq-flash inline-flex items-center gap-1 text-ok" role="status">
                  <StatusDot tone="ok" />
                  Saved {saved.n} {saved.n === 1 ? "change" : "changes"}
                </span>
              )
            )}
            <StatusBarButton
              onClick={() => (barOpen && filters.length === 0 ? closeBar() : openBar())}
              active={barOpen || filters.length > 0}
              title={`Filter rows (${isMacPlatform() ? "⌘F" : "Ctrl F"})`}
            >
              Filters{filters.length > 0 ? ` · ${filters.length}` : ""}
            </StatusBarButton>
            <StatusBarButton title="Choose columns to display (coming soon)" disabled>
              Columns
            </StatusBarButton>
            {editable ? (
              <span className="inline-flex items-center gap-1 text-ok" title="This table has a primary key — cells are editable">
                <StatusDot tone="ok" />
                <span>editable</span>
              </span>
            ) : (
              <span
                className={`inline-flex items-center gap-1 ${pkError ? "text-warn" : "text-muted"}`}
                title={
                  pkError
                    ? `Couldn't read the primary key, so editing is disabled — this table may well have one. ${pkError}`
                    : pk && pk.length === 0
                      ? "No primary key — editing disabled"
                      : "Read-only"
                }
              >
                <StatusDot tone={pkError ? "warn" : "muted"} />
                <span>
                  read-only
                  {pkError ? " · PK unknown" : pk && pk.length === 0 ? " · no PK" : ""}
                </span>
              </span>
            )}
          </>
        }
      />

      {showProfile && (
        <ColumnProfilePanel
          connId={connId}
          engine={engine}
          schema={schema}
          table={object.name}
          columns={object.columns.map((c) => c.name)}
          onClose={() => setShowProfile(false)}
        />
      )}
      {showAlter && (
        <AlterTablePanel
          connId={connId}
          engine={engine}
          schema={schema}
          table={object.name}
          columns={object.columns}
          onClose={() => {
            setShowAlter(false);
            refresh();
          }}
        />
      )}

      {/* Right-click row menu — TablePlus parity (Slice 31). Built lazily. */}
      {cellMenu &&
        (() => {
          const colNames = object.columns.map((c) => c.name);
          const vals = cellMenu.values;
          // Identity (PK → WHERE) never comes from `vals`: a staged edit to a PK
          // column would aim the statement at whichever row already holds the new key.
          const srv = cellMenu.serverValues;
          // Full row in column order; falls back to the single clicked cell when
          // the row's page hasn't loaded yet.
          const rowValues: (string | null)[] = colNames.map((c) =>
            vals ? (vals[c] ?? null) : c === cellMenu.col ? cellMenu.value : null,
          );
          // Engine-quoted: `"users"` is a string literal on MySQL, not an identifier.
          const qualified = `${quoteIdent(schema, engine)}.${quoteIdent(object.name, engine)}`;
          const copy = (s: string) => navigator.clipboard?.writeText(s);
          const copyRowAs = (fmt: Parameters<typeof buildRowAs>[0]) =>
            copy(buildRowAs(fmt, rowValues, colNames, { table: qualified, engine }));
          const applyCellValue = async (newValue: string | null) => {
            if (!pk || pk.length === 0 || !srv) return;
            const edit = buildEdit(pk, (c) => srv[c] ?? null, cellMenu.col, newValue);
            setNotice(null);
            try {
              const { rowsAffected } = await applyEdit(connId, schema, object.name, edit);
              const problem = writeOutcomeMessage("update", rowsAffected);
              if (problem) setNotice(`Set "${cellMenu.col}": ${problem}`);
              refresh();
            } catch (e) {
              setNotice(`Update failed: ${e}`);
            }
          };
          const deleteStmt = () => {
            if (!pk || pk.length === 0 || !srv) return `-- no primary key on ${object.name}`;
            return buildDeleteStatement(
              schema,
              object.name,
              pk.map((c) => [c, srv[c] ?? null] as [string, string | null]),
              engine,
            );
          };
          const deleteRowAction = async () => {
            if (!pk || pk.length === 0 || !srv) return;
            const pkPairs = pk.map((c) => [c, srv[c] ?? null] as [string, string | null]);
            const ok = await confirmDialog(
              `Delete this row from ${qualified}? This cannot be undone.`,
              "Delete row",
            );
            if (!ok) return;
            setNotice(null);
            try {
              const { rowsAffected } = await deleteRow(connId, schema, object.name, pkPairs);
              // The confirm said "this row" — say so plainly when it was not one row.
              const problem = writeOutcomeMessage("delete", rowsAffected);
              if (problem) setNotice(`Delete row: ${problem}`);
              // The row's tint fades out first, so the refetch doesn't just yank it.
              else await gridRef.current?.fadeRow(cellMenu.row);
              refresh();
            } catch (e) {
              setNotice(`Delete failed: ${e}`);
            }
          };

          const items: (ContextMenuItem | "separator")[] = [
            { id: "refresh", label: "Refresh", onSelect: () => !blockedByEdits() && refresh() },
            {
              id: "add-row",
              label: "Add row",
              disabled: !editable,
              onSelect: () => setDraft({}),
            },
            {
              id: "duplicate",
              label: "Duplicate row",
              disabled: !editable || !vals,
              onSelect: () => {
                if (!vals) return;
                const d: Record<string, string> = {};
                for (const c of object.columns) {
                  if (!pk?.includes(c.name)) d[c.name] = vals[c.name] ?? "";
                }
                setDraft(d);
              },
            },
            "separator",
            {
              id: "set-value",
              label: "Set value",
              disabled: !editable || !srv,
              children: [
                { id: "set-empty", label: "Empty ''", onSelect: () => void applyCellValue("") },
                { id: "set-null", label: "NULL", onSelect: () => void applyCellValue(null) },
                { id: "set-default", label: "DEFAULT", disabled: true, onSelect: () => {} },
              ],
            },
            "separator",
            {
              id: "copy-cell",
              label: "Copy cell value",
              onSelect: () => copy(cellMenu.value ?? ""),
            },
            {
              id: "copy-all-cols",
              label: "Copy all column values",
              onSelect: () => copy(buildRowAs("plain", rowValues, colNames)),
            },
            {
              id: "copy-row-as",
              label: "Copy row as",
              children: [
                { id: "cra-plain", label: "Plain text", onSelect: () => copyRowAs("plain") },
                { id: "cra-json", label: "JSON", onSelect: () => copyRowAs("json") },
                { id: "cra-html", label: "HTML", onSelect: () => copyRowAs("html") },
                { id: "cra-md", label: "Markdown table", onSelect: () => copyRowAs("md") },
                { id: "cra-csv", label: "CSV", onSelect: () => copyRowAs("csv") },
                { id: "cra-csvh", label: "CSV with header", onSelect: () => copyRowAs("csv-header") },
                { id: "cra-insert", label: "INSERT statement", onSelect: () => copyRowAs("insert") },
              ],
            },
            "separator",
            {
              id: "quick-filter",
              label: `Filter "${cellMenu.col}" = ${cellMenu.value ?? "NULL"}`,
              onSelect: () => {
                // `= NULL` matches nothing; a NULL cell filters as IS NULL.
                const r = newRule(cellMenu.col, cellMenu.value === null ? "IsNull" : "Eq", cellMenu.value ?? "");
                const f = ruleToFilter(r);
                if (!f || blockedByEdits()) return;
                setRules((rs) => [...rs, r]);
                setBarOpen(true);
                applyFilters([...filters, f]);
              },
            },
            {
              id: "copy-delete",
              label: "Copy DELETE statement",
              tone: "danger",
              disabled: !pk || pk.length === 0 || !srv,
              onSelect: () => copy(deleteStmt()),
            },
            {
              id: "delete-row",
              label: "Delete row",
              tone: "danger",
              disabled: !editable || !srv,
              onSelect: () => void deleteRowAction(),
            },
          ];
          return (
            <ContextMenu
              x={cellMenu.x}
              y={cellMenu.y}
              items={items}
              onClose={() => setCellMenu(null)}
            />
          );
        })()}
    </div>
  );
}

function StructureView({
  connId,
  schema,
  object,
  fks,
}: {
  connId: string;
  schema: string;
  object: ObjectInfo;
  fks: ForeignKey[];
}) {
  // Structure sub-tabs (TablePlus parity), all three served by one `describe_table`.
  const [tab, setTab] = useState<"columns" | "indexes" | "triggers">("columns");
  const [structure, setStructure] = useState<TableStructure | null>(null);
  /** Why there is nothing to show: the engine can't answer, or the call failed. */
  const [note, setNote] = useState<string | null>(null);

  const columnsKey = object.columns.map((c) => `${c.name}:${c.data_type}:${c.nullable}`).join("|");
  useEffect(() => {
    let live = true;
    setStructure(null);
    setNote(null);
    describeTable(connId, schema, object.name)
      .then((s) => {
        if (live) setStructure(s);
      })
      .catch((e: unknown) => {
        if (!live) return;
        setNote(
          isNotImplementedError(e)
            ? "This connection doesn't report indexes or triggers."
            : String((e as { message?: string })?.message ?? e),
        );
      });
    return () => {
      live = false;
    };
    // Re-read when the column list changes (an Alter applied from this tab).
  }, [connId, schema, object.name, columnsKey]);

  const subTabs: { id: typeof tab; label: string; count?: number }[] = [
    { id: "columns", label: "Columns", count: object.columns.length },
    { id: "indexes", label: "Indexes", count: structure?.indexes.length },
    { id: "triggers", label: "Triggers", count: structure?.triggers.length },
  ];
  // The catalog list is richer (default, comment), but the browse model already has the
  // names and types — so Columns renders at once and fills in, and still works for an
  // engine whose `describe_table` says NotImplemented.
  const columns: ColumnDetail[] =
    structure?.columns ??
    object.columns.map((c) => ({
      name: c.name,
      data_type: c.data_type,
      nullable: c.nullable,
      default: null,
      is_pk: c.pk ?? false,
      comment: null,
    }));

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-1 border-b border-border px-3 py-1.5">
        {subTabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={`rounded-[var(--radius-sm)] px-2.5 py-1 text-[11px] transition-colors ${
              tab === t.id
                ? "bg-primary-soft font-medium text-primary"
                : "text-muted hover:text-text"
            }`}
          >
            {t.label}
            {t.count !== undefined && (
              <span className="ml-1 tabular-nums opacity-60">{t.count}</span>
            )}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-4">
        {tab === "columns" && (
          <StructTable head={["Name", "Type", "Nullable", "Default", "References", "Comment"]}>
            {columns.map((c) => {
              const fk = fks.find((f) => f.from_column === c.name);
              return (
                <tr key={c.name}>
                  <td className="whitespace-nowrap px-3 py-1.5 font-mono text-text">
                    {c.name}
                    {c.is_pk && (
                      <span className="ml-1.5 align-middle">
                        <Chip tone="muted" title="Primary key">
                          PK
                        </Chip>
                      </span>
                    )}
                  </td>
                  <td className="whitespace-nowrap px-3 py-1.5 text-muted">{c.data_type}</td>
                  <td className="px-3 py-1.5 text-muted">{c.nullable ? "yes" : "no"}</td>
                  <td className="px-3 py-1.5 font-mono text-[11px] text-muted">
                    <Ellipsis text={c.default} />
                  </td>
                  <td className="whitespace-nowrap px-3 py-1.5 font-mono text-[11px] text-primary">
                    {fk ? `${fk.to_table}.${fk.to_column}` : ""}
                  </td>
                  <td className="px-3 py-1.5 text-muted">
                    <Ellipsis text={c.comment} />
                  </td>
                </tr>
              );
            })}
          </StructTable>
        )}

        {tab === "indexes" &&
          (note ? (
            <StructNote>{note}</StructNote>
          ) : !structure ? (
            <StructNote>Loading…</StructNote>
          ) : structure.indexes.length === 0 ? (
            <StructNote>No indexes on this {object.kind.toLowerCase()}.</StructNote>
          ) : (
            <StructTable head={["Name", "Columns", "Type", "Definition"]}>
              {structure.indexes.map((i) => (
                <tr key={i.name}>
                  <td className="whitespace-nowrap px-3 py-1.5 font-mono text-text">{i.name}</td>
                  <td className="px-3 py-1.5 font-mono text-[11px] text-muted">
                    {i.columns.join(", ")}
                  </td>
                  <td className="whitespace-nowrap px-3 py-1.5 text-muted">
                    {i.primary ? "PRIMARY" : i.unique ? "UNIQUE" : "INDEX"}
                  </td>
                  <td className="px-3 py-1.5 font-mono text-[11px] text-muted">
                    <Ellipsis text={i.definition} />
                  </td>
                </tr>
              ))}
            </StructTable>
          ))}

        {tab === "triggers" &&
          (note ? (
            <StructNote>{note}</StructNote>
          ) : !structure ? (
            <StructNote>Loading…</StructNote>
          ) : structure.triggers.length === 0 ? (
            <StructNote>No triggers on this {object.kind.toLowerCase()}.</StructNote>
          ) : (
            <StructTable head={["Name", "When", "Definition"]}>
              {structure.triggers.map((t) => (
                <tr key={t.name}>
                  <td className="whitespace-nowrap px-3 py-1.5 font-mono text-text">{t.name}</td>
                  <td className="whitespace-nowrap px-3 py-1.5 text-muted">{t.timing_event}</td>
                  <td className="px-3 py-1.5 font-mono text-[11px] text-muted">
                    <Ellipsis text={t.definition} />
                  </td>
                </tr>
              ))}
            </StructTable>
          ))}
      </div>
    </div>
  );
}

/** The Structure tab's one table chrome — dense rows, quiet header (DESIGN.md). */
function StructTable({ head, children }: { head: string[]; children: ReactNode }) {
  return (
    <div className="overflow-hidden rounded-md border border-border bg-bg">
      <table className="w-full text-xs">
        <thead className="border-b border-border bg-surface text-left text-[11px] text-muted">
          <tr>
            {head.map((h) => (
              <th key={h} className="px-3 py-1.5 font-medium">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-border">{children}</tbody>
      </table>
    </div>
  );
}

/** Loading / empty / "this engine can't answer" — never a blank panel. */
function StructNote({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-md border border-dashed border-border p-6 text-center text-[12px] text-muted">
      {children}
    </div>
  );
}

/** A one-line cell: an em-dash for nothing, the full text on hover for something long. */
function Ellipsis({ text }: { text: string | null }) {
  if (!text) return <span className="text-muted-2">—</span>;
  return (
    <span className="block max-w-[38ch] truncate" title={text}>
      {text}
    </span>
  );
}


/** TablePlus-style row pager — `‹ 1–300 of 1,234 ›`. The label is built by the
 *  tested {@link pagerLabel}; this only draws it. */
function RowPager({
  label,
  stale,
  hasPrev,
  hasNext,
  onPrev,
  onNext,
}: {
  label: string;
  /** The label's total answers the previous filters — drawn dimmed. */
  stale?: boolean;
  hasPrev: boolean;
  hasNext: boolean;
  onPrev: () => void;
  onNext: () => void;
}) {
  return (
    <span className="inline-flex items-center gap-1 text-[11px] text-muted">
      <StatusBarButton onClick={onPrev} disabled={!hasPrev} title="Previous page">
        <IconChevronLeft size={10} />
      </StatusBarButton>
      <span className={`tabular-nums transition-colors ${stale ? "text-muted" : "text-text"}`}>{label}</span>
      <StatusBarButton onClick={onNext} disabled={!hasNext} title="Next page">
        <IconChevronRight size={10} />
      </StatusBarButton>
    </span>
  );
}
