import { useEffect, useRef, useState } from "react";
import { highlightCode } from "@lezer/highlight";
import { StandardSQL } from "@codemirror/lang-sql";
import { sqlLog, clearSqlLog, type SqlLogEntry } from "../lib/commands";
import { formatLogTs, mergeEntries, SQL_LOG_CAP } from "../lib/sqlLog";
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

function escapeHtml(s: string): string {
  return s.replace(/[&<>]/g, (c) => (c === "&" ? "&amp;" : c === "<" ? "&lt;" : "&gt;"));
}

/** Highlight one SQL statement to an HTML string using the editor's own
 * `sqlHighlight` classes/colours (dialect-agnostic `StandardSQL`, since a
 * log entry can come from any connected engine). */
function highlightSql(sqlText: string): string {
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
 * TablePlus-parity SQL log panel (⌘⇧C, Task 9): every statement the app ran
 * on `connId`, each preceded by a `-- <timestamp>` comment line. Polls
 * `sqlLog` every second while mounted (skipped while the tab is hidden),
 * auto-scrolls unless the user has scrolled up, and offers a footer trash
 * button (`clearSqlLog`) + an optional syntax-highlighting toggle.
 */
export function SqlLogPanel({ connId }: { connId: string }) {
  const [entries, setEntries] = useState<SqlLogEntry[]>([]);
  const lastSeqRef = useRef(0);
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
  useEffect(() => {
    lastSeqRef.current = 0;
    stickyBottomRef.current = true;
    setEntries([]);
  }, [connId]);

  // Poll only while mounted; skip a tick while the window/tab is hidden.
  useEffect(() => {
    let cancelled = false;
    const poll = () => {
      if (document.hidden) return;
      sqlLog(connId, lastSeqRef.current)
        .then((next) => {
          if (cancelled || next.length === 0) return;
          lastSeqRef.current = next[next.length - 1].seq;
          setEntries((prev) => mergeEntries(prev, next, SQL_LOG_CAP));
        })
        .catch((e) => logError("sql log: poll", e));
    };
    poll();
    const id = window.setInterval(poll, POLL_MS);
    return () => {
      cancelled = true;
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
    void clearSqlLog(connId).catch((e) => logError("sql log: clear", e));
    lastSeqRef.current = 0;
    setEntries([]);
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
          entries.map((e) => (
            <div key={e.seq} className="mb-2">
              <div className="text-muted">-- {formatLogTs(e.ts_us)}</div>
              {highlight ? (
                <div dangerouslySetInnerHTML={{ __html: highlightSql(e.sql) }} />
              ) : (
                e.sql
              )}
            </div>
          ))
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
