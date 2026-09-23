import { useMemo, type ReactNode } from "react";
import { DataGrid, type GridSource } from "./DataGrid";
import { Tabs } from "./ui/Panel";
import { IconRefresh } from "./ui/Icon";
import { fetchResultWindow, type StatementResultMeta } from "../lib/commands";
import {
  errorJumpOffset,
  resultTabs,
  runMessages,
  type RunItem,
} from "../lib/queryResults";
import { lineColAt } from "../lib/sqlSplit";
import { oneLine } from "../lib/history";

/** Tab key of the always-present Messages tab. */
export const MESSAGES_KEY = "__messages__";

interface Props {
  /** One meta per statement the backend actually executed. */
  metas: StatementResultMeta[];
  /** The statements sent, aligned with `meta.index` (BEGIN/COMMIT carry no `stmt`). */
  items: RunItem[];
  /** The editor document the run came from — error line/position rebase onto it. */
  doc: string;
  running: boolean;
  /** Transport-level failure (the run never produced metas). */
  runError: string | null;
  /** `resultId` of the visible result, or {@link MESSAGES_KEY}. */
  selectedKey: string;
  onSelect: (key: string) => void;
  /** Move the editor caret to a doc offset (inline-error click). */
  onJump: (offset: number) => void;
  /** Replaces the result body when set (the chart drawer). */
  children?: ReactNode;
}

/**
 * The Query tab's results pane (TablePlus parity): one tab per statement that
 * returned rows, a Messages tab listing every statement's outcome, and the
 * failing statement's error inline with a clickable `Line N`.
 */
export function QueryResults({
  metas,
  items,
  doc,
  running,
  runError,
  selectedKey,
  onSelect,
  onJump,
  children,
}: Props) {
  const tabs = useMemo(() => resultTabs(metas), [metas]);
  const messages = useMemo(() => runMessages(metas), [metas]);
  const failed = useMemo(() => metas.find((m) => m.error) ?? null, [metas]);
  const selected = tabs.find((t) => t.resultId === selectedKey) ?? null;
  const selectedMeta = selected ? metas.find((m) => m.index === selected.metaIndex) ?? null : null;

  /** Doc offset the failing statement at `metaIndex` should jump to. */
  const offsetFor = (metaIndex: number): number | null => {
    const m = metas.find((x) => x.index === metaIndex);
    if (!m?.error) return null;
    return errorJumpOffset(doc, items[metaIndex]?.stmt, m.error);
  };
  const jumpTo = (metaIndex: number) => {
    const offset = offsetFor(metaIndex);
    if (offset !== null) onJump(offset);
  };
  /** 1-based line *in the document* — what the editor's gutter shows. */
  const docLineFor = (metaIndex: number): number | null => {
    const offset = offsetFor(metaIndex);
    return offset === null ? null : lineColAt(doc, offset).line;
  };

  const source = useMemo<GridSource | null>(() => {
    if (!selectedMeta?.resultId || selectedMeta.columns.length === 0) return null;
    const resultId = selectedMeta.resultId;
    return {
      key: resultId,
      totalRows: selectedMeta.rowCount,
      columns: selectedMeta.columns.map((c) => c.name),
      fetch: (o, l) => fetchResultWindow(resultId, o, l),
    };
  }, [selectedMeta]);

  const body = () => {
    if (children) return children;
    if (runError) {
      return (
        <div className="h-full overflow-auto p-4 font-mono text-[11.5px] text-danger">
          {runError}
        </div>
      );
    }
    if (metas.length === 0) {
      return (
        <div className="flex h-full items-center justify-center text-xs text-muted">
          {running ? (
            <span className="inline-flex items-center gap-2">
              <IconRefresh size={12} className="animate-spin" /> Running…
            </span>
          ) : (
            <span>
              Write SQL, then <span className="tiq-kbd">⌘↵</span> to run the statement under the
              cursor, <span className="tiq-kbd">⇧⌘↵</span> to run all.
            </span>
          )}
        </div>
      );
    }
    if (selectedMeta && !source) {
      // A result set with zero columns (e.g. `SET`, or a driver that returns an
      // empty shape) — show its message rather than the grid's "Loading…".
      return (
        <div className="flex h-full items-center justify-center text-xs text-muted">
          {messages.find((m) => m.index === selectedMeta.index)?.text ?? "No columns"}
        </div>
      );
    }
    if (source) return <DataGrid source={source} />;
    return (
      <MessageList messages={messages} items={items} onJump={jumpTo} docLineFor={docLineFor} />
    );
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <Tabs
        tabs={[
          ...tabs.map((t) => ({ key: t.resultId, label: t.label })),
          {
            key: MESSAGES_KEY,
            label: "Messages",
            badge: failed ? (
              <span aria-hidden className="h-1.5 w-1.5 rounded-full bg-danger" />
            ) : undefined,
          },
        ]}
        active={selected ? selected.resultId : MESSAGES_KEY}
        onSelect={onSelect}
      />

      {failed?.error && (
        <button
          onClick={() => jumpTo(failed.index)}
          title="Jump to the error in the editor"
          className="flex shrink-0 items-start gap-2 border-b border-danger/30 bg-danger/10 px-3 py-1.5 text-left text-[11.5px] text-danger transition-colors hover:bg-danger/15"
        >
          <span className="shrink-0 font-mono text-[10px] uppercase tracking-wide opacity-80">
            Error
          </span>
          <span className="min-w-0 flex-1 whitespace-pre-wrap break-words font-mono">
            {failed.error.message}
          </span>
          <span className="shrink-0 font-mono text-[10.5px] underline underline-offset-2">
            Line {docLineFor(failed.index)}
          </span>
        </button>
      )}

      <div className="min-h-0 flex-1">{body()}</div>
    </div>
  );
}

function MessageList({
  messages,
  items,
  onJump,
  docLineFor,
}: {
  messages: ReturnType<typeof runMessages>;
  items: RunItem[];
  onJump: (metaIndex: number) => void;
  docLineFor: (metaIndex: number) => number | null;
}) {
  return (
    <div className="h-full overflow-auto">
      <table className="w-full border-collapse text-[11.5px]">
        <tbody>
          {messages.map((m) => (
            <tr
              key={m.index}
              onClick={() => m.tone === "error" && onJump(m.index)}
              className={`border-b border-border align-top ${
                m.tone === "error" ? "cursor-pointer hover:bg-surface-2" : ""
              }`}
            >
              <td className="w-8 px-3 py-1 font-mono text-[10.5px] text-muted-2">{m.index + 1}</td>
              <td className="px-2 py-1 font-mono text-muted">
                {oneLine(items[m.index]?.text ?? "", 60)}
              </td>
              <td
                className={`px-2 py-1 font-mono ${m.tone === "error" ? "text-danger" : "text-text"}`}
              >
                {m.text}
                {m.tone === "error" && docLineFor(m.index) != null && (
                  <span className="ml-2 text-[10.5px] underline underline-offset-2">
                    Line {docLineFor(m.index)}
                  </span>
                )}
                {m.notice && <div className="text-[10.5px] text-warn">{m.notice}</div>}
              </td>
              <td className="w-16 px-3 py-1 text-right font-mono text-[10.5px] text-muted-2">
                {m.execMs} ms
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
