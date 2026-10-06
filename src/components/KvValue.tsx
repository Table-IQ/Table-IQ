import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { KvBytes, KvItem } from "../lib/commands";
import { hexDump, plainBytes, prettyJsonText, toBase64, type StringFormat } from "../lib/kvFormat";
import {
  stageHdel,
  stageHset,
  stageJsonSet,
  stageLrem,
  stageLset,
  stagePush,
  stageSadd,
  stageSetString,
  stageSrem,
  stageXadd,
  stageXdel,
  stageZadd,
  stageZrem,
  type Staged,
} from "../lib/kvChanges";
import { highlightJson } from "../lib/docView";
import { SegmentedToggle } from "./ui/StatusBar";
import { Panel } from "./ui/Panel";
import { IconTrash } from "./ui/Icon";

/** What a type's view needs from the key pane. `readOnly` covers truncated values, a commit in flight, and unsupported types. */
export interface ValueProps {
  keyName: string;
  items: KvItem[];
  /** Staged edits by row (`Staged.target`). */
  staged: ReadonlyMap<string, Staged>;
  stage: (s: Staged) => void;
  unstage: (target: string) => void;
  /** A counter for rows that have no identity of their own (pushes, stream adds). */
  seq: () => number;
  readOnly: boolean;
}

const cellCls = "px-2 font-mono text-[12.5px]";
const input =
  "h-6 rounded-md border border-border bg-bg px-1.5 font-mono text-[11.5px] text-text placeholder:text-muted-2 focus:border-primary focus:outline-none";
const btn = "h-6 rounded-md border border-border px-2 text-[11px] text-text hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-40";

const rowKey = (b: KvBytes) => b.b64 ?? b.text;
const shown = (b: KvBytes | undefined) => b?.text ?? "";
const isBin = (b: KvBytes | undefined) => !!b?.binary;

// ── table + inline cell editing ─────────────────────────────────────────────

type RowState = "clean" | "edited" | "removed" | "new";
interface TRow {
  id: string;
  state: RowState;
  cells: ReactNode[];
  /** Absent for a row that can't be removed (a staged push, which only `undo` takes back). */
  remove?: () => void;
  undo?: () => void;
}

const TINT: Record<RowState, string> = {
  clean: "hover:bg-surface-2",
  edited: "bg-primary-soft",
  removed: "bg-danger/10 text-muted line-through",
  new: "bg-ok/10",
};

function KvTable({ head, rows, readOnly, empty }: { head: string[]; rows: TRow[]; readOnly: boolean; empty: string }) {
  return (
    <div className="min-h-0 flex-1 overflow-auto" data-testid="kv-table">
      <table className="w-full table-fixed border-separate border-spacing-0">
        <thead className="sticky top-0 z-10 bg-surface text-left text-[11px] text-muted">
          <tr>
            {head.map((h, i) => (
              <th key={h} className={`h-6 border-b border-border px-2 font-medium ${i === 0 ? "w-[34%]" : ""}`}>
                {h}
              </th>
            ))}
            <th className="h-6 w-[72px] border-b border-border" />
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.id} data-state={r.state} className={`group h-6 align-middle ${TINT[r.state]}`}>
              {r.cells.map((c, i) => (
                <td key={i} className={`${cellCls} border-b border-border/60`}>
                  {c}
                </td>
              ))}
              <td className="border-b border-border/60 pr-1 text-right">
                {!readOnly && (
                  <span className="inline-flex gap-0.5 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
                    {(r.state === "edited" || r.state === "removed" || r.state === "new") && r.undo && (
                      <button onClick={r.undo} className="rounded px-1 text-[10.5px] text-primary hover:bg-surface-2">
                        Undo
                      </button>
                    )}
                    {r.remove && r.state !== "removed" && (
                      <button onClick={r.remove} aria-label="Remove" title="Remove" className="inline-flex h-5 w-5 items-center justify-center rounded text-muted hover:bg-surface-2 hover:text-danger">
                        <IconTrash size={11} />
                      </button>
                    )}
                  </span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {rows.length === 0 && <div className="px-3 py-4 text-xs text-muted">{empty}</div>}
    </div>
  );
}

/** A cell that edits in place: double-click (or Enter on it), Enter stages, Esc cancels. */
function EditCell({ value, display, onCommit, disabled, title }: { value: string; display?: string; onCommit: (v: string) => void; disabled?: boolean; title?: string }) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(value);
  const done = useRef(false);
  const start = () => {
    if (disabled) return;
    done.current = false;
    setText(value);
    setEditing(true);
  };
  const finish = (save: boolean) => {
    if (done.current) return;
    done.current = true;
    setEditing(false);
    if (save && text !== value) onCommit(text);
  };
  if (editing)
    return (
      <input
        autoFocus
        spellCheck={false}
        aria-label="Edit value"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => finish(true)}
        onKeyDown={(e) => {
          if (e.key === "Enter") finish(true);
          else if (e.key === "Escape") finish(false);
        }}
        className={`${input} w-full`}
      />
    );
  return (
    <span
      tabIndex={disabled ? undefined : 0}
      onDoubleClick={start}
      onKeyDown={(e) => e.key === "Enter" && start()}
      title={disabled ? title : (title ?? "Double-click to edit")}
      className={`block truncate ${disabled ? "" : "cursor-text"}`}
    >
      {display ?? (value || <span className="text-muted-2">(empty)</span>)}
    </span>
  );
}

const Bin = () => (
  <span className="mr-1 rounded bg-surface-2 px-1 text-[9.5px] text-muted" title="Binary (not UTF-8): shown lossy, read-only">
    bin
  </span>
);

/** One line of inputs above a table: stage a new field / member / item. */
function AddRow({ fields, submit, actions }: { fields: { label: string; placeholder?: string; wide?: boolean }[]; submit: (vals: string[], action: string) => string | null; actions: string[] }) {
  const [vals, setVals] = useState<string[]>(() => fields.map(() => ""));
  const [err, setErr] = useState<string | null>(null);
  const go = (action: string) => {
    const e = submit(vals, action);
    setErr(e);
    if (!e) setVals(fields.map(() => ""));
  };
  return (
    <form
      className="flex flex-wrap items-center gap-1.5 border-b border-border bg-bg px-3 py-1.5 text-[11px] text-muted"
      onSubmit={(e) => {
        e.preventDefault();
        go(actions[0]);
      }}
    >
      {fields.map((f, i) => (
        <label key={f.label} className={`flex items-center gap-1 ${f.wide ? "min-w-[160px] flex-1" : ""}`}>
          <span>{f.label}</span>
          <input
            aria-label={f.label}
            value={vals[i]}
            placeholder={f.placeholder}
            spellCheck={false}
            onChange={(e) => setVals(vals.map((v, j) => (j === i ? e.target.value : v)))}
            className={`${input} min-w-0 ${f.wide ? "flex-1" : "w-28"}`}
          />
        </label>
      ))}
      {actions.map((a, i) => (
        <button key={a} type={i === 0 ? "submit" : "button"} onClick={i === 0 ? undefined : () => go(a)} className={btn}>
          {a}
        </button>
      ))}
      {err && (
        <span role="alert" className="text-danger">
          {err}
        </span>
      )}
    </form>
  );
}

// ── hash ────────────────────────────────────────────────────────────────────

export function HashView({ keyName: k, items, staged, stage, unstage, readOnly }: ValueProps) {
  const loaded = useMemo(() => new Map(items.map((i) => [rowKey(i.field!), i])), [items]);
  const rows: TRow[] = items.map((it) => {
    const id = `f:${rowKey(it.field!)}`;
    const s = staged.get(id);
    const removed = s?.change.kind === "hdel";
    const value = s?.change.kind === "hset" ? s.change.value : it.value!;
    const locked = readOnly || isBin(it.value) || isBin(it.field);
    return {
      id,
      state: removed ? "removed" : s ? "edited" : "clean",
      cells: [
        <span key="f" className="block truncate" title={shown(it.field)}>
          {isBin(it.field) && <Bin />}
          {shown(it.field)}
        </span>,
        <EditCell key="v" value={shown(value)} disabled={locked} title={locked ? "Read-only" : undefined} onCommit={(v) => (v === shown(it.value) ? unstage(id) : stage(stageHset(k, it.field!, plainBytes(v), it.value)))} />,
      ],
      remove: () => stage(stageHdel(k, it.field!, it.value!)),
      undo: () => unstage(id),
    };
  });
  for (const s of staged.values())
    if (s.change.kind === "hset" && s.fresh)
      rows.push({
        id: s.target,
        state: "new",
        cells: [s.change.field.text, s.change.value.text],
        remove: () => unstage(s.target),
        undo: () => unstage(s.target),
      });
  return (
    <>
      {!readOnly && (
        <AddRow
          fields={[{ label: "Field" }, { label: "Value", wide: true }]}
          actions={["Stage field"]}
          submit={([f, v]) => {
            if (!f) return "Enter a field name";
            const ex = loaded.get(f);
            stage(stageHset(k, plainBytes(f), plainBytes(v), ex?.value));
            return null;
          }}
        />
      )}
      <KvTable head={["Field", "Value"]} rows={rows} readOnly={readOnly} empty="This hash has no fields." />
    </>
  );
}

// ── list ────────────────────────────────────────────────────────────────────

export function ListView({ keyName: k, items, staged, stage, unstage, seq, readOnly }: ValueProps) {
  const pushes = [...staged.values()].filter((s) => s.change.kind === "lpush" || s.change.kind === "rpush");
  const rowOf = (s: Staged): TRow => ({
    id: s.target,
    state: "new",
    cells: [s.change.kind === "lpush" ? "left" : "right", (s.change as { value: KvBytes }).value.text],
    undo: () => unstage(s.target),
  });
  const rows: TRow[] = [
    ...pushes.filter((s) => s.change.kind === "lpush").map(rowOf),
    ...items.map((it) => {
      const i = it.index!;
      const id = `i:${i}`;
      const s = staged.get(id);
      const removed = !!s?.removal;
      const value = s?.change.kind === "lset" && !s.removal ? s.change.value : it.value!;
      const locked = readOnly || isBin(it.value);
      return {
        id,
        state: (removed ? "removed" : s ? "edited" : "clean") as RowState,
        cells: [
          <span key="i" className="tabular-nums text-muted">
            {i}
          </span>,
          <EditCell key="v" value={shown(value)} disabled={locked} title={locked ? "Read-only" : undefined} onCommit={(v) => (v === shown(it.value) ? unstage(id) : stage(stageLset(k, i, plainBytes(v), it.value!)))} />,
        ],
        remove: () => stage(stageLrem(k, i, it.value!)),
        undo: () => unstage(id),
      };
    }),
    ...pushes.filter((s) => s.change.kind === "rpush").map(rowOf),
  ];
  return (
    <>
      {!readOnly && (
        <AddRow
          fields={[{ label: "Value", wide: true }]}
          actions={["Push right", "Push left"]}
          submit={([v], side) => {
            stage(stagePush(k, side === "Push left" ? "left" : "right", plainBytes(v), seq()));
            return null;
          }}
        />
      )}
      <KvTable head={["Index", "Value"]} rows={rows} readOnly={readOnly} empty="This list is empty." />
    </>
  );
}

// ── set ─────────────────────────────────────────────────────────────────────

export function SetView({ keyName: k, items, staged, stage, unstage, readOnly }: ValueProps) {
  const have = useMemo(() => new Set(items.map((i) => rowKey(i.value!))), [items]);
  const rows: TRow[] = items.map((it) => {
    const id = `m:${rowKey(it.value!)}`;
    return {
      id,
      state: staged.has(id) ? "removed" : "clean",
      cells: [
        <span key="m" className="block truncate" title={shown(it.value)}>
          {isBin(it.value) && <Bin />}
          {shown(it.value)}
        </span>,
      ],
      remove: () => stage(stageSrem(k, it.value!)),
      undo: () => unstage(id),
    };
  });
  for (const s of staged.values())
    if (s.change.kind === "sadd") rows.push({ id: s.target, state: "new", cells: [s.change.member.text], undo: () => unstage(s.target) });
  return (
    <>
      {!readOnly && (
        <AddRow
          fields={[{ label: "Member", wide: true }]}
          actions={["Stage member"]}
          submit={([m]) => (have.has(m) ? "Already a member" : (stage(stageSadd(k, plainBytes(m))), null))}
        />
      )}
      <KvTable head={["Member"]} rows={rows} readOnly={readOnly} empty="This set is empty." />
    </>
  );
}

// ── sorted set ──────────────────────────────────────────────────────────────

export function ZsetView({ keyName: k, items, staged, stage, unstage, readOnly }: ValueProps) {
  const loaded = useMemo(() => new Map(items.map((i) => [rowKey(i.value!), i])), [items]);
  const setScore = (member: KvBytes, text: string, was?: number): string | null => {
    const n = Number(text);
    if (text.trim() === "" || !Number.isFinite(n)) return "The score must be a number";
    if (was === n) unstage(`m:${rowKey(member)}`);
    else stage(stageZadd(k, member, n, was));
    return null;
  };
  const rows: TRow[] = items.map((it) => {
    const id = `m:${rowKey(it.value!)}`;
    const s = staged.get(id);
    const removed = s?.change.kind === "zrem";
    const score = s?.change.kind === "zadd" ? s.change.score : it.score!;
    return {
      id,
      state: removed ? "removed" : s ? "edited" : "clean",
      cells: [
        <span key="m" className="block truncate" title={shown(it.value)}>
          {isBin(it.value) && <Bin />}
          {shown(it.value)}
        </span>,
        <EditCell key="s" value={String(score)} disabled={readOnly || isBin(it.value)} onCommit={(v) => void setScore(it.value!, v, it.score)} />,
      ],
      remove: () => stage(stageZrem(k, it.value!)),
      undo: () => unstage(id),
    };
  });
  for (const s of staged.values())
    if (s.change.kind === "zadd" && s.fresh)
      rows.push({ id: s.target, state: "new", cells: [s.change.member.text, String(s.change.score)], undo: () => unstage(s.target) });
  return (
    <>
      {!readOnly && (
        <AddRow
          fields={[{ label: "Member", wide: true }, { label: "Score", placeholder: "0" }]}
          actions={["Stage member"]}
          submit={([m, sc]) => (m === "" ? "Enter a member" : setScore(plainBytes(m), sc, loaded.get(m)?.score))}
        />
      )}
      <KvTable head={["Member", "Score"]} rows={rows} readOnly={readOnly} empty="This sorted set is empty." />
    </>
  );
}

// ── stream ──────────────────────────────────────────────────────────────────

const fieldsText = (f: { field: KvBytes; value: KvBytes }[]) => f.map((x) => `${x.field.text}=${x.value.text}`).join("  ");

export function StreamView({ keyName: k, items, staged, stage, unstage, seq, readOnly }: ValueProps) {
  const [adding, setAdding] = useState(false);
  const rows: TRow[] = items.map((it) => {
    const id = `d:${it.id}`;
    return {
      id,
      state: staged.has(id) ? "removed" : "clean",
      cells: [it.id, <span key="f" className="block truncate" title={fieldsText(it.fields ?? [])}>{fieldsText(it.fields ?? [])}</span>],
      remove: () => stage(stageXdel(k, it.id!)),
      undo: () => unstage(id),
    };
  });
  for (const s of staged.values())
    if (s.change.kind === "xadd")
      rows.push({ id: s.target, state: "new", cells: [s.change.id ?? "*", fieldsText(s.change.fields)], undo: () => unstage(s.target) });
  return (
    <>
      {!readOnly && (
        <div className="flex items-center border-b border-border bg-bg px-3 py-1.5">
          <button onClick={() => setAdding(true)} className={btn}>
            Add entry…
          </button>
        </div>
      )}
      <KvTable head={["Entry ID", "Fields"]} rows={rows} readOnly={readOnly} empty="This stream has no entries." />
      {adding && (
        <StreamAddDialog
          onClose={() => setAdding(false)}
          onStage={(id, fields) => {
            stage(stageXadd(k, id, fields, seq()));
            setAdding(false);
          }}
        />
      )}
    </>
  );
}

function StreamAddDialog({ onClose, onStage }: { onClose: () => void; onStage: (id: string | undefined, fields: { field: KvBytes; value: KvBytes }[]) => void }) {
  const [id, setId] = useState("");
  const [pairs, setPairs] = useState<[string, string][]>([["", ""]]);
  const [err, setErr] = useState<string | null>(null);
  const submit = () => {
    const f = pairs.filter(([a]) => a !== "");
    if (f.length === 0) return setErr("Add at least one field");
    if (id.trim() && !/^\d+(-\d+)?$/.test(id.trim())) return setErr("An entry ID looks like 1700000000000-0; leave it empty to let Redis pick one");
    onStage(id.trim() || undefined, f.map(([a, b]) => ({ field: plainBytes(a), value: plainBytes(b) })));
  };
  return (
    <Panel
      title="Add stream entry"
      onClose={onClose}
      width="sm"
      footer={
        <>
          <span className="flex-1 text-[11px] text-muted">Staged until you commit</span>
          <button onClick={onClose} className="rounded-md px-3 py-1 text-xs text-muted hover:bg-surface-2 hover:text-text">
            Cancel
          </button>
          <button onClick={submit} className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover">
            Stage
          </button>
        </>
      }
    >
      <div className="flex flex-col gap-2 p-3 text-[11px] text-muted">
        <label className="flex items-center gap-2">
          <span className="w-16">Entry ID</span>
          <input aria-label="Entry ID" value={id} onChange={(e) => setId(e.target.value)} placeholder="* (auto)" spellCheck={false} className={`${input} flex-1`} autoFocus />
        </label>
        {pairs.map(([a, b], i) => (
          <div key={i} className="flex items-center gap-2">
            <input aria-label={`Field ${i + 1}`} value={a} placeholder="field" spellCheck={false} onChange={(e) => setPairs(pairs.map((p, j) => (j === i ? [e.target.value, p[1]] : p)))} className={`${input} w-32`} />
            <input aria-label={`Value ${i + 1}`} value={b} placeholder="value" spellCheck={false} onChange={(e) => setPairs(pairs.map((p, j) => (j === i ? [p[0], e.target.value] : p)))} className={`${input} flex-1`} />
          </div>
        ))}
        <button onClick={() => setPairs([...pairs, ["", ""]])} className={`${btn} self-start`}>
          Add field
        </button>
        {err && (
          <div role="alert" className="rounded-md border border-danger/30 bg-danger/10 px-2 py-1 text-danger">
            {err}
          </div>
        )}
      </div>
    </Panel>
  );
}

// ── string + JSON: one text editor ──────────────────────────────────────────

/** The string value (Text / JSON / Hex) or the JSON document (`$`) as a text area. The JSON format is a DISPLAY:
 *  whitespace only, values as written. An edit is staged as the exact text typed (JSON is only syntax-checked,
 *  never re-serialised); typing the original back unstages it. `jsonLoaded` is `JSON.GET key $` as the server
 *  answered (the optimistic check). A value holding a CR can't be edited here: a text area turns CRLF into LF. */
export function TextValue({
  keyName: k,
  mode,
  value,
  jsonLoaded,
  format,
  onFormat,
  readOnly,
  truncated,
  staged,
  stage,
  unstage,
}: {
  keyName: string;
  mode: "string" | "json";
  /** string: the loaded bytes; json: the document text as the server wrote it. */
  value: KvBytes;
  jsonLoaded?: string;
  format: StringFormat;
  onFormat: (f: StringFormat) => void;
  readOnly: boolean;
  truncated: boolean;
  staged: ReadonlyMap<string, Staged>;
  stage: (s: Staged) => void;
  unstage: (target: string) => void;
}) {
  const target = mode === "json" ? "json" : "value";
  const binary = mode === "string" && !!value.binary;
  const hasCr = value.text.includes("\r");
  /** How the loaded text is shown in `f`. Display only. */
  const shownIn = (f: StringFormat) => (mode === "json" || f === "json" ? (prettyJsonText(value.text) ?? value.text) : value.text);
  const [text, setText] = useState(() => {
    const s = staged.get(target)?.change;
    return s?.kind === "setString" ? s.value.text : s?.kind === "jsonSet" ? s.value : shownIn(format);
  });
  const [parseErr, setParseErr] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  // Changing the format re-presents the loaded text; text the user has already changed is left exactly as typed.
  useEffect(() => {
    if (!staged.has(target)) setText(shownIn(format));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [format]);
  const locked = readOnly || binary || truncated || hasCr || format === "hex";

  const edit = (t: string) => {
    setText(t);
    const unchanged = t === value.text || t === shownIn(format);
    if (mode === "json") {
      try {
        JSON.parse(t);
        setParseErr(null);
      } catch (e) {
        setParseErr((e as Error).message);
        unstage(target);
        return;
      }
      if (unchanged) unstage(target);
      else stage(stageJsonSet(k, "$", t, jsonLoaded ?? ""));
    } else if (unchanged) unstage(target);
    else stage(stageSetString(k, plainBytes(t), value));
  };

  const copyB64 = () => {
    void navigator.clipboard?.writeText(toBase64(value))?.catch(() => {});
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 border-b border-border bg-bg px-3 py-1.5 text-[11px] text-muted">
        {mode === "string" ? (
          <SegmentedToggle<StringFormat>
            options={[
              { value: "text", label: "Text" },
              { value: "json", label: "JSON" },
              { value: "hex", label: "Hex" },
            ]}
            value={format}
            onChange={onFormat}
          />
        ) : (
          <span>JSON document at $</span>
        )}
        {binary && (
          <span className="rounded bg-surface-2 px-1.5 py-0.5 text-[10.5px]" title="The bytes are not valid UTF-8, so the text view is lossy and editing is off">
            binary · read-only
          </span>
        )}
        {hasCr && !binary && <span className="text-warn">Contains CR characters, so it is read-only here: edit it from the Console tab.</span>}
        {truncated && <span className="text-warn">Shown cut at 1 MiB — read-only so it can't be written back cut</span>}
        <span className="flex-1" />
        {mode === "string" && (
          <button onClick={copyB64} className={btn}>
            {copied ? "Copied" : "Copy base64"}
          </button>
        )}
      </div>
      {format === "hex" && mode === "string" ? (
        <pre aria-label="Hex dump" className="min-h-0 flex-1 overflow-auto p-3 font-mono text-[12px] leading-[1.5] text-text">
          {hexDump(value)}
        </pre>
      ) : locked ? (
        mode === "json" ? (
          <pre className="min-h-0 flex-1 overflow-auto p-3 font-mono text-[12px] leading-[1.5]" dangerouslySetInnerHTML={{ __html: highlightJson(text) }} />
        ) : (
          <pre aria-label="Value" className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap break-all p-3 font-mono text-[12px] leading-[1.5] text-text">
            {text}
          </pre>
        )
      ) : (
        <textarea
          aria-label={mode === "json" ? "JSON document" : "Value"}
          spellCheck={false}
          value={text}
          onChange={(e) => edit(e.target.value)}
          className="min-h-0 flex-1 resize-none bg-bg p-3 font-mono text-[12px] leading-[1.5] text-text focus:outline-none"
        />
      )}
      {parseErr && (
        <div role="alert" className="border-t border-danger/30 bg-danger/10 px-3 py-1 text-[11px] text-danger">
          Not valid JSON, so it is not staged: {parseErr}
        </div>
      )}
    </div>
  );
}
