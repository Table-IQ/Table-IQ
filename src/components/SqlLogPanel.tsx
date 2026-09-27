import { memo, useEffect, useMemo, useRef, useState } from "react";
import { highlightCode } from "@lezer/highlight";
import { StandardSQL } from "@codemirror/lang-sql";
import { sqlLog, clearSqlLog, type SqlLogEntry } from "../lib/commands";
import { formatLogTs, mergeEntries, shouldApply, SQL_LOG_CAP } from "../lib/sqlLog";
import { sqlHighlight } from "../lib/sqlHighlight";
import { logError } from "../lib/report";
import { ResizeHandle, clampWidth } from "./ui/ResizeHandle";
import { IconButton } from "./ui/IconButton";
import { IconTrash } from "./ui/Icon";

const POLL_MS = 1000;
const HEIGHT_KEY = "tiq:sqllog:h";
const HL_KEY = "tiq:sqllog:hl";
const MIN_H = 80;
const MAX_H = 600;
const DEFAULT_H = 180;
// Sticky-bottom tolerance: still counts as "at the bottom" within this many
// px, so the row's own height doesn't kick the user out of auto-scroll.
const STICKY_PX = 24;

function readLS(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}
function writeLS(key: string, value: string): void {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // quota / private mode — ignore, it's just a UI pref
  }
}

let highlightCssInjected = false;
/**
 * `sqlHighlight` is a `HighlightStyle` built for a CodeMirror `EditorView`,
 * which injects its generated CSS into the document itself. We drive
 * `highlightCode` directly (no editor instance), so nothing does that for
 * us — mount the same CSS text once, by hand.
 */
function ensureHighlightCss(): void {
  if (highlightCssInjected || typeof document === "undefined") return;
  const css = sqlHighlight.module?.getRules();
  if (!css) return;
  highlightCssInjected = true;
  const style = document.createElement("style");
  style.setAttribute("data-tiq", "sql-log-highlight");
  style.textContent = css;
  document.head.appendChild(style);
}

/** Exported for unit testing (SqlLogPanel.test.ts) — no component harness needed, it's a
 *  pure string transform. */
export function escapeHtml(s: string): string {
  return s.replace(/[&<>]/g, (c) => (c === "&" ? "&amp;" : c === "<" ? "&lt;" : "&gt;"));
}

/** Highlight one SQL statement to an HTML string using the editor's own
 * `sqlHighlight` classes/colours (dialect-agnostic `StandardSQL`, since a
 * log entry can come from any connected engine).
 *
 * Exported for unit testing: `ensureHighlightCss` no-ops when `document` is
 * undefined (no jsdom in this project), so this whole function — lezer
 * parse + highlight, no DOM required otherwise — is safe to call from a
 * plain Node test. */
export function highlightSql(sqlText: string): string {
  ensureHighlightCss();
  const tree = StandardSQL.language.parser.parse(sqlText);
  let html = "";
  highlightCode(
    sqlText,
    tree,
    sqlHighlight,
    (text, classes) => {
      const escaped = escapeHtml(text);
      html += classes ? `<span class="${classes}">${escaped}</span>` : escaped;
    },
    () => {
      html += "\n";
    },
  );
  return html;
}

/**
 * One log entry (fix round 1): extracted + `memo`'d so an unrelated poll tick
 * that only appends new entries doesn't re-highlight the ones already on
 * screen — `entry` keeps its object identity across `mergeEntries` calls for
 * anything already present, so `memo`'s shallow prop comparison skips a
 * re-render (and therefore the `useMemo` below) for every entry except the
 * newly-appended ones.
 */
const LogEntryRow = memo(function LogEntryRow({
  entry,
  highlight,
}: {
  entry: SqlLogEntry;
  highlight: boolean;
}) {
  const html = useMemo(
    () => (highlight ? highlightSql(entry.sql) : null),
    [entry.sql, highlight],
  );
  return (
    <div className="mb-2">
      <div className="text-muted">-- {formatLogTs(entry.ts_us)}</div>
      {html !== null ? <div dangerouslySetInnerHTML={{ __html: html }} /> : entry.sql}
    </div>
  );
});

/**
 * TablePlus-parity SQL log panel (⌘⇧C, Task 9): every statement the app ran
 * on `connId`, each preceded by a `-- <timestamp>` comment line. Polls
 * `sqlLog` every second while mounted (skipped while the tab is hidden),
 * auto-scrolls unless the user has scrolled up, and offers a footer trash
 * button (`clearSqlLog`) + an optional syntax-highlighting toggle.
 */
export function SqlLogPanel({ connId }: { connId: string }) {
  const [entries, setEntries] = useState<SqlLogEntry[]>([]);
  const lastSeqRef = useRef(0);
  // Bumped on clear and on connId change; each poll captures it when it
  // *starts* and checks it again when it resolves (`shouldApply`) — see
  // sqlLog.ts. Fixes a clear racing an in-flight poll (the poll would
  // otherwise reapply the pre-clear entries and `lastSeq` right after).
  const genRef = useRef(0);
  const inFlightRef = useRef(false);
  const [height, setHeight] = useState<number>(() => {
    const n = Number(readLS(HEIGHT_KEY));
    return Number.isFinite(n) && n > 0 ? clampWidth(n, MIN_H, MAX_H) : DEFAULT_H;
  });
  const [highlight, setHighlight] = useState<boolean>(() => readLS(HL_KEY) === "1");
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const stickyBottomRef = useRef(true);

  useEffect(() => writeLS(HEIGHT_KEY, String(height)), [height]);
  useEffect(() => writeLS(HL_KEY, highlight ? "1" : "0"), [highlight]);

  // `seq` is global across every connection (design-notes.md), so on a
  // connection switch the old entries no longer belong here — start over.
  // Declared (and thus committed) before the poll effect below, so its
  // generation bump is visible to the poll effect's own setup that runs
  // right after — see the ordering note in `shouldApply`'s doc comment.
  useEffect(() => {
    genRef.current += 1;
    lastSeqRef.current = 0;
    stickyBottomRef.current = true;
    setEntries([]);
  }, [connId]);

  // Poll only while mounted; skip a tick while the window/tab is hidden or a
  // previous poll is still in flight (a slow response shouldn't pile up
  // concurrent requests).
  useEffect(() => {
    let unmounted = false; // true unmount only — a connId change is already
    // covered by the generation bump above, since this whole effect re-runs.
    const poll = () => {
      if (document.hidden || inFlightRef.current) return;
      const gen = genRef.current;
      inFlightRef.current = true;
      sqlLog(connId, lastSeqRef.current)
        .then((next) => {
          if (unmounted || !shouldApply(gen, genRef.current) || next.length === 0) return;
          lastSeqRef.current = next[next.length - 1].seq;
          setEntries((prev) => mergeEntries(prev, next, SQL_LOG_CAP));
        })
        .catch((e) => logError("sql log: poll", e))
        .finally(() => {
          inFlightRef.current = false;
        });
    };
    poll();
    const id = window.setInterval(poll, POLL_MS);
    return () => {
      unmounted = true;
      window.clearInterval(id);
    };
  }, [connId]);

  useEffect(() => {
    const el = bodyRef.current;
    if (el && stickyBottomRef.current) el.scrollTop = el.scrollHeight;
  }, [entries]);

  const onScroll = () => {
    const el = bodyRef.current;
    if (!el) return;
    stickyBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < STICKY_PX;
  };

  const onClear = () => {
    // Bump the generation and reset local state synchronously, *before*
    // kicking off `clearSqlLog` — any poll already in flight resolves after
    // this and its generation check now fails, so it can't reapply the
    // pre-clear entries. `lastSeq` is kept, not reset: seq is globally
    // monotonic, so a poll tick that fires before `clearSqlLog` lands still
    // asks only for entries newer than the ones just cleared.
    genRef.current += 1;
    stickyBottomRef.current = true;
    setEntries([]);
    void clearSqlLog(connId).catch((e) => logError("sql log: clear", e));
  };

  return (
    <div
      role="region"
      aria-label="SQL log"
      className="relative flex shrink-0 flex-col border-t border-border bg-bg"
      style={{ height }}
    >
      <ResizeHandle
        direction="vertical"
        side="top"
        onResize={(d) => setHeight((h) => clampWidth(h - d, MIN_H, MAX_H))}
      />
      <div
        ref={bodyRef}
        onScroll={onScroll}
        className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap px-3 py-2 font-mono text-[11px] leading-relaxed text-text"
      >
        {entries.length === 0 ? (
          <div className="text-muted">No statements logged yet.</div>
        ) : (
          entries.map((e) => <LogEntryRow key={e.seq} entry={e} highlight={highlight} />)
        )}
      </div>
      <div className="flex shrink-0 items-center justify-between gap-2 border-t border-border bg-surface px-3 py-1.5">
        <IconButton
          icon={<IconTrash size={14} />}
          label="Clear log"
          variant="danger"
          size="sm"
          onClick={onClear}
        />
        <label className="flex items-center gap-2 text-xs text-muted">
          <input
            type="checkbox"
            checked={highlight}
            onChange={(e) => setHighlight(e.target.checked)}
          />
          Enable syntax highlighting
        </label>
      </div>
    </div>
  );
}
