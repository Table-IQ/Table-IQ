import { useEffect, useRef, useState } from "react";
import { kvCommand } from "../lib/commands";
import { confirmDialog } from "../lib/confirmDialog";
import { formatReply, needsConfirm } from "../lib/kvFormat";
import { loadHistory, pushHistory, saveHistory, shouldRemember, stepHistory } from "../lib/kvView";
import { readableError } from "../lib/docJson";

type Tone = "ok" | "error" | "refused";
interface Entry {
  id: number;
  line: string;
  text: string;
  tone: Tone;
}

const MAX_ENTRIES = 300;
const MAX_LINES = 1000;

const TONE: Record<Tone, string> = { ok: "text-text", error: "text-danger", refused: "text-warn" };

/** A redis-cli in a pane: type a command, see the reply as redis-cli prints it. Refusals (admin, scripting,
 *  blocking commands…) show the backend's reason; DEL / UNLINK / GETDEL / FLUSHDB ask first, then re-run confirmed. */
export function KvConsole({ connId, database, visible }: { connId: string; database: string; visible: boolean }) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [text, setText] = useState("");
  const [history, setHistory] = useState(() => loadHistory(connId));
  const [pos, setPos] = useState(() => loadHistory(connId).length);
  const [draft, setDraft] = useState("");
  const [running, setRunning] = useState(false);
  const seq = useRef(0);
  const bottom = useRef<HTMLDivElement>(null);
  const field = useRef<HTMLInputElement>(null);

  useEffect(() => {
    bottom.current?.scrollIntoView?.({ block: "end" });
  }, [entries]);
  useEffect(() => {
    if (visible) field.current?.focus();
  }, [visible]);

  const add = (line: string, out: string, tone: Tone) =>
    setEntries((es) => [...es, { id: ++seq.current, line, text: out, tone }].slice(-MAX_ENTRIES));

  const run = async () => {
    const line = text.trim();
    if (!line || running) return;
    setDraft("");
    setText("");
    setRunning(true);
    /** Only a command the server accepted is remembered, and never one that carries a credential. */
    let accepted = false;
    try {
      let confirmed = false;
      for (;;) {
        try {
          const reply = await kvCommand(connId, line, confirmed);
          add(line, formatReply(reply), reply.type === "error" ? "error" : "ok");
          accepted = true;
          break;
        } catch (e) {
          const cmd = needsConfirm(e);
          if (cmd && !confirmed && (await confirmDialog(`${cmd} deletes data. Run "${line}"?`, "Confirm command", { ok: "Run command", cancel: "Cancel" }))) {
            confirmed = true;
            continue;
          }
          add(line, readableError(e), "refused");
          break;
        }
      }
    } finally {
      const h = accepted && shouldRemember(line) ? pushHistory(history, line) : history;
      if (h !== history) {
        setHistory(h);
        saveHistory(connId, h);
      }
      setPos(h.length);
      setRunning(false);
      field.current?.focus();
    }
  };

  const walk = (dir: -1 | 1) => {
    const d = pos === history.length ? text : draft;
    if (pos === history.length) setDraft(text);
    const r = stepHistory(history, pos, dir, d);
    setPos(r.pos);
    setText(r.text);
  };

  return (
    <div className={`${visible ? "flex" : "hidden"} h-full min-h-0 flex-col bg-bg`} data-testid="kv-console">
      <div className="min-h-0 flex-1 overflow-auto px-3 py-2 font-mono text-[12.5px] leading-[1.5]" aria-live="polite">
        {entries.length === 0 && (
          <div className="font-sans text-xs text-muted">
            Type a Redis command, for example <code className="font-mono text-text">GET demo:1</code> or <code className="font-mono text-text">HGETALL hash:a</code>. ↑ and ↓ walk through earlier commands.
          </div>
        )}
        {entries.map((e) => {
          const lines = e.text.split("\n");
          return (
            <div key={e.id} className="mb-2" data-tone={e.tone}>
              <div className="text-muted">
                <span className="select-none text-primary">{`db${database}> `}</span>
                <span className="text-text">{e.line}</span>
              </div>
              <pre className={`whitespace-pre-wrap break-all ${TONE[e.tone]}`}>
                {lines.length > MAX_LINES ? `${lines.slice(0, MAX_LINES).join("\n")}\n… ${lines.length - MAX_LINES} more lines` : e.text}
              </pre>
            </div>
          );
        })}
        <div ref={bottom} />
      </div>
      <form
        className="flex shrink-0 items-center gap-2 border-t border-border bg-surface px-3 py-1.5"
        onSubmit={(e) => {
          e.preventDefault();
          void run();
        }}
      >
        <span className="select-none font-mono text-[12.5px] text-primary">{`db${database}>`}</span>
        <input
          ref={field}
          aria-label="Redis command"
          value={text}
          autoComplete="off"
          spellCheck={false}
          placeholder="PING"
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowUp") {
              e.preventDefault();
              walk(-1);
            } else if (e.key === "ArrowDown") {
              e.preventDefault();
              walk(1);
            } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              void run();
            } else if (e.key.toLowerCase() === "l" && e.ctrlKey) {
              e.preventDefault();
              setEntries([]);
            }
          }}
          className="h-6 min-w-0 flex-1 bg-transparent font-mono text-[12.5px] text-text placeholder:text-muted-2 focus:outline-none"
        />
        <button type="submit" disabled={running || !text.trim()} className="h-6 rounded-md bg-primary px-2.5 text-[11px] font-medium text-white hover:opacity-90 disabled:opacity-50">
          Run ↵
        </button>
        <button type="button" onClick={() => setEntries([])} disabled={entries.length === 0} className="h-6 rounded-md border border-border px-2 text-[11px] text-muted hover:text-text disabled:opacity-40">
          Clear
        </button>
      </form>
    </div>
  );
}
