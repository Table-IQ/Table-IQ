import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { kvApplyChanges, kvRead, type KvChange, type KvItem, type KvRead } from "../lib/commands";
import { useConnections } from "../state/connectionStore";
import { normalizeEnv } from "../lib/connEnv";
import { confirmDialog } from "../lib/confirmDialog";
import { gatedCommit, productionCommitMessage } from "../lib/stagedEdits";
import { escapeHtml } from "../lib/sqlHighlightHtml";
import { bytesLabel, kindLabel, ttlLabel, unwrapSingle, type StringFormat } from "../lib/kvFormat";
import { kvCommitError, kvStagedCounts, orderChanges, previewScript, unstage as dropStaged, upsert, type Staged } from "../lib/kvChanges";
import { readableError } from "../lib/docJson";
import { mergeItems } from "../lib/kvTree";
import { HashView, ListView, SetView, StreamView, TextValue, ZsetView, type ValueProps } from "./KvValue";
import { StagedReview } from "./StagedReview";
import { StagedCountChips } from "./StagedCountChips";
import { StatusBar, StatusBarButton, StatusDot } from "./ui/StatusBar";
import { IconRefresh, IconTrash } from "./ui/Icon";

const PAGE = 100;
const SUPPORTED = ["string", "list", "set", "hash", "zset", "stream", "ReJSON-RL"];
const num = (n: number) => n.toLocaleString("en-US");
const input =
  "h-6 rounded-md border border-border bg-bg px-1.5 font-mono text-[11.5px] text-text placeholder:text-muted-2 focus:border-primary focus:outline-none";
const sizeUnit: Record<string, string> = { list: "items", set: "members", hash: "fields", zset: "members", stream: "entries" };

type Notice = { tone: "ok" | "warn"; text: string };

/** One key: type / size / TTL header, the value by type with paging, and staged editing with
 *  Review → Commit (one MULTI/EXEC, every edit checked against the value it was loaded with). */
export function KvKey({
  connId,
  keyName,
  visible,
  format,
  onFormat,
  onGone,
}: {
  connId: string;
  keyName: string;
  visible: boolean;
  format: StringFormat;
  onFormat: (f: StringFormat) => void;
  /** The key was renamed (`to`) or deleted / expired (undefined): the tree should re-scan. */
  onGone: (to?: string) => void;
}) {
  const [data, setData] = useState<KvRead | null>(null);
  const [items, setItems] = useState<KvItem[]>([]);
  const [next, setNext] = useState<string | undefined>();
  const [version, setVersion] = useState(0);
  const [loading, setLoading] = useState(true);
  const [more, setMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [staged, setStaged] = useState<Staged[]>([]);
  const [review, setReview] = useState(false);
  const [committing, setCommitting] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [lockHint, setLockHint] = useState(false);
  const [tool, setTool] = useState<"ttl" | "rename" | null>(null);
  const [toolText, setToolText] = useState("");
  const [busy, setBusy] = useState(false);
  const seq = useRef(0);
  const kindRef = useRef("");

  const env = useConnections((s) => normalizeEnv(s.saved.find((c) => c.id === connId)?.env));
  const connName = useConnections((s) => s.saved.find((c) => c.id === connId)?.name ?? "production");
  const setEditsPending = useConnections((s) => s.setEditsPending);
  const production = env === "production";

  const locked = staged.length > 0 || committing;
  const lockedRef = useRef(locked);
  lockedRef.current = locked;
  useEffect(() => {
    setEditsPending(locked);
    return () => setEditsPending(false);
  }, [locked, setEditsPending]);
  useEffect(() => {
    if (staged.length === 0) setReview(false);
    if (!locked) setLockHint(false);
  }, [staged.length, locked]);
  const blocked = useCallback(() => {
    if (!lockedRef.current) return false;
    setLockHint(true);
    return true;
  }, []);

  // ── load ──────────────────────────────────────────────────────────────────
  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      // path "$": a document is read the way `jsonSet`'s optimistic check reads it.
      const r = await kvRead(connId, keyName, { count: PAGE, path: "$" });
      setData(r);
      kindRef.current = r.kind;
      setItems(r.items);
      setNext(r.next);
      setVersion((v) => v + 1);
      return r;
    } catch (e) {
      setError(readableError(e));
      return null;
    } finally {
      setLoading(false);
    }
  }, [connId, keyName]);
  useEffect(() => {
    void load();
  }, [load]);

  const loadMore = async () => {
    if (!next || more) return;
    setMore(true);
    try {
      const r = await kvRead(connId, keyName, { cursor: next, count: PAGE });
      setItems((cur) => mergeItems(kindRef.current, cur, r.items));
      setNext(r.next);
    } catch (e) {
      setNotice({ tone: "warn", text: readableError(e) });
    } finally {
      setMore(false);
    }
  };

  const refresh = async () => {
    if (blocked()) return;
    setNotice(null);
    const r = await load();
    if (r?.kind === "none") onGone();
  };

  // ── staging ───────────────────────────────────────────────────────────────
  const stagedMap = useMemo(() => new Map(staged.map((s) => [s.target, s])), [staged]);
  const stage = useCallback((s: Staged) => {
    setStaged((l) => upsert(l, s));
    setNotice(null);
  }, []);
  const unstage = useCallback((t: string) => setStaged((l) => dropStaged(l, t)), []);
  const counts = kvStagedCounts(staged);
  const changes = useMemo(() => orderChanges(staged), [staged]);

  const confirmCommit = useMemo(
    () => (production ? () => confirmDialog(productionCommitMessage(staged.length, connName), "Production", { ok: "Commit to production", cancel: "Cancel" }) : undefined),
    [production, staged.length, connName],
  );

  const commit = useCallback(async () => {
    if (committing || staged.length === 0) return;
    await gatedCommit(confirmCommit, async () => {
      setCommitting(true);
      const sent: KvChange[] = orderChanges(staged);
      try {
        const res = await kvApplyChanges(connId, sent);
        setStaged([]);
        setNotice({ tone: "ok", text: `Committed ${res.applied} ${res.applied === 1 ? "change" : "changes"} in one transaction` });
        const r = await load();
        if (r?.kind === "none") onGone();
      } catch (e) {
        const ce = kvCommitError(e, sent);
        setNotice({ tone: "warn", text: ce.text });
        if (ce.partial) {
          // Redis ran (part of) the batch and can't undo it: a retry would repeat it. Drop the edits, show what is there now.
          setStaged([]);
          const r = await load();
          if (r?.kind === "none") onGone();
        }
      } finally {
        setCommitting(false);
      }
    });
  }, [committing, staged, confirmCommit, connId, load, onGone]);

  useEffect(() => {
    if (!visible || staged.length === 0) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void commit();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [visible, staged.length, commit]);

  const discard = () => {
    setStaged([]);
    setNotice(null);
    // The staged text lives in the editors: remount them from the loaded value.
    setVersion((v) => v + 1);
  };

  // ── key-level actions: applied at once (they have no value to check), so never alongside staged edits ──
  const apply = async (change: KvChange, done: string, after?: () => void) => {
    if (blocked() || busy) return;
    if (production && !(await confirmDialog(productionCommitMessage(1, connName), "Production", { ok: "Apply to production", cancel: "Cancel" }))) return;
    setBusy(true);
    try {
      await kvApplyChanges(connId, [change]);
      setNotice({ tone: "ok", text: done });
      setTool(null);
      if (after) after();
      else await load();
    } catch (e) {
      setNotice({ tone: "warn", text: kvCommitError(e, [change]).text });
    } finally {
      setBusy(false);
    }
  };
  const setTtl = () => {
    const s = Number(toolText.trim());
    if (!Number.isInteger(s) || s < 1) return setNotice({ tone: "warn", text: "A TTL is a whole number of seconds, 1 or more. To remove the expiry use Persist." });
    void apply({ kind: "expire", key: keyName, seconds: s }, `Expires in ${s}s`);
  };
  const rename = () => {
    const to = toolText;
    if (!to || to === keyName) return setNotice({ tone: "warn", text: "Enter a different name" });
    void apply({ kind: "rename", key: keyName, newKey: to }, `Renamed to ${to}`, () => onGone(to));
  };
  const del = async () => {
    if (blocked() || busy) return;
    const ok = await confirmDialog(`Delete the key "${keyName}" and its ${data?.size ? `${num(data.size)} ${sizeUnit[data.kind] ?? "bytes"}` : "value"}? This can't be undone.${production ? ` It is on production (${connName}).` : ""}`, "Delete key", { ok: "Delete key", cancel: "Cancel" });
    if (ok) await apply({ kind: "del", key: keyName }, `Deleted ${keyName}`, () => onGone());
  };
  const openTool = (t: "ttl" | "rename") => {
    if (blocked()) return;
    setTool(tool === t ? null : t);
    setToolText(t === "rename" ? keyName : data && data.ttl > 0 ? String(data.ttl) : "");
  };

  // ── render ────────────────────────────────────────────────────────────────
  const kind = data?.kind ?? "";
  const supported = SUPPORTED.includes(kind);
  const binaryValue = kind === "string" && !!items[0]?.value?.binary;
  const readOnly = committing || !!data?.truncated || !supported || binaryValue;
  const vp: ValueProps = { keyName, items, staged: stagedMap, stage, unstage, seq: () => seq.current++, readOnly };
  const first = items[0];
  const script = review ? previewScript(changes) : "";

  const body = () => {
    if (error)
      return (
        <div className="m-3 rounded-md border border-danger/30 bg-danger/10 p-3 text-xs text-danger" role="alert">
          {error}
        </div>
      );
    if (!data) return <div className="p-3 text-xs text-muted">Loading…</div>;
    if (kind === "none")
      return <div className="flex flex-1 items-center justify-center text-sm text-muted">This key no longer exists. It may have expired or been deleted.</div>;
    if (!supported)
      return (
        <div className="flex flex-1 items-center justify-center px-6 text-center text-sm text-muted">
          Unsupported type "{kind}". Browse or edit it from the Console tab.
        </div>
      );
    switch (kind) {
      case "string":
        return <TextValue key={version} keyName={keyName} mode="string" value={first?.value ?? { text: "" }} format={format} onFormat={onFormat} readOnly={readOnly} truncated={data.truncated} staged={stagedMap} stage={stage} unstage={unstage} />;
      case "ReJSON-RL": {
        // `JSON.GET key $` answers `[doc]`; show doc as the server wrote it (display only, never re-serialised).
        const raw = first?.value?.text ?? "";
        return <TextValue key={version} keyName={keyName} mode="json" value={{ text: unwrapSingle(raw) ?? raw }} jsonLoaded={raw} format="text" onFormat={() => {}} readOnly={readOnly} truncated={data.truncated} staged={stagedMap} stage={stage} unstage={unstage} />;
      }
      case "hash":
        return <HashView {...vp} />;
      case "list":
        return <ListView {...vp} />;
      case "set":
        return <SetView {...vp} />;
      case "zset":
        return <ZsetView {...vp} />;
      default:
        return <StreamView {...vp} />;
    }
  };

  const paged = !!data && ["list", "set", "hash", "zset", "stream"].includes(kind);

  return (
    <div className={`${visible ? "flex" : "hidden"} h-full min-h-0 flex-col`}>
      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border bg-bg px-3 py-1.5 text-xs">
        <span className="min-w-0 max-w-[40%] truncate font-mono font-semibold text-text" title={keyName}>
          {keyName}
        </span>
        {data && kind !== "none" && <span className="rounded bg-surface px-1.5 py-0.5 text-[10.5px] text-muted">{kindLabel(kind)}</span>}
        {data && supported && (
          <span className="text-[11px] text-muted tabular-nums">
            {kind === "string" || kind === "ReJSON-RL" ? bytesLabel(data.size) : `${num(data.size)} ${sizeUnit[kind]}`}
          </span>
        )}
        {data && kind !== "none" && (
          <button
            onClick={() => openTool("ttl")}
            aria-expanded={tool === "ttl"}
            title="Set or remove the expiry"
            className={`rounded border px-1.5 py-0.5 text-[10.5px] tabular-nums hover:text-text ${data.ttl >= 0 ? "border-warn/30 bg-warn/10 text-warn" : "border-border text-muted"}`}
          >
            TTL · {ttlLabel(data.ttl)}
          </button>
        )}
        <span className="flex-1" />
        {data && kind !== "none" && (
          <>
            <button onClick={() => openTool("rename")} className="rounded-md border border-border px-2 py-0.5 text-[11px] text-muted hover:text-text">
              Rename
            </button>
            <button onClick={() => void del()} aria-label="Delete key" title="Delete key" className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-danger/10 hover:text-danger">
              <IconTrash size={12} />
            </button>
          </>
        )}
        <button onClick={() => void refresh()} aria-label="Reload key" title="Reload key" className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-surface-2 hover:text-text">
          <IconRefresh size={12} className={loading ? "animate-spin" : undefined} />
        </button>
      </div>

      {tool && (
        <form
          className="tiq-slide-in flex flex-wrap items-center gap-1.5 border-b border-border bg-surface px-3 py-1.5 text-[11px] text-muted"
          onSubmit={(e) => {
            e.preventDefault();
            if (tool === "ttl") setTtl();
            else rename();
          }}
        >
          {tool === "ttl" ? (
            <>
              <label className="flex items-center gap-1">
                <span>Expire in</span>
                <input aria-label="Seconds" autoFocus inputMode="numeric" value={toolText} onChange={(e) => setToolText(e.target.value)} placeholder="seconds" className={`${input} w-24`} />
                <span>seconds</span>
              </label>
              <button type="submit" disabled={busy} className="h-6 rounded-md bg-primary px-2.5 text-[11px] font-medium text-white hover:opacity-90 disabled:opacity-60">
                Set TTL
              </button>
              <button type="button" disabled={busy || (data?.ttl ?? -1) < 0} onClick={() => void apply({ kind: "persist", key: keyName }, "Expiry removed")} className="h-6 rounded-md border border-border px-2 text-[11px] text-text hover:bg-surface-2 disabled:opacity-40">
                Persist
              </button>
            </>
          ) : (
            <label className="flex flex-1 items-center gap-1">
              <span>New name</span>
              <input aria-label="New key name" autoFocus value={toolText} onChange={(e) => setToolText(e.target.value)} spellCheck={false} className={`${input} min-w-0 flex-1`} />
              <button type="submit" disabled={busy} className="h-6 rounded-md bg-primary px-2.5 text-[11px] font-medium text-white hover:opacity-90 disabled:opacity-60">
                Rename
              </button>
            </label>
          )}
          <button type="button" onClick={() => setTool(null)} className="h-6 rounded-md px-2 text-[11px] text-muted hover:text-text">
            Cancel
          </button>
        </form>
      )}

      {staged.length > 0 && (
        <div className="relative z-20 flex items-center gap-2 border-b border-l-2 border-l-primary border-border bg-primary-soft px-3 py-1.5">
          <span className="text-[11px] font-medium text-primary">
            {staged.length} unsaved {staged.length === 1 ? "change" : "changes"}
          </span>
          {lockHint && (
            <span className="tiq-slide-in text-[11px] text-warn" role="status">
              Commit or discard changes first
            </span>
          )}
          <span className="flex-1" />
          <span className="relative inline-flex">
            <button onClick={() => setReview((v) => !v)} className="rounded-md border border-border px-2 py-0.5 text-[11px] text-muted hover:text-text">
              Review
            </button>
            {review && (
              <StagedReview
                sql={script}
                html={escapeHtml(script)}
                total={staged.length}
                align="right"
                summary="redis commands"
                copyLabel="Copy commands"
                footnote="Applied as one MULTI/EXEC with WATCH: if any change fails, or a value changed since it was loaded, nothing is written."
                committing={committing}
                onCommit={() => void commit()}
                onDiscard={discard}
                onClose={() => setReview(false)}
              />
            )}
          </span>
          <button onClick={discard} disabled={committing} className="rounded-md border border-border px-2 py-0.5 text-[11px] text-muted hover:text-text disabled:opacity-40">
            Discard
          </button>
          <button onClick={() => void commit()} disabled={committing} className="inline-flex items-center gap-1 rounded-md bg-primary px-2.5 py-0.5 text-[11px] font-medium text-white hover:opacity-90 disabled:opacity-70">
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

      {notice && (
        <div role="status" className={`flex items-start gap-2 border-b border-border px-3 py-1.5 text-[11px] ${notice.tone === "ok" ? "bg-ok/10 text-ok" : "bg-warn/10 text-warn"}`}>
          <span className="max-h-24 min-w-0 flex-1 overflow-y-auto break-words">{notice.text}</span>
          <button onClick={() => setNotice(null)} aria-label="Dismiss" className="text-muted hover:text-text">
            ×
          </button>
        </div>
      )}

      <div className="relative flex min-h-0 flex-1 flex-col">
        {loading && <div className="tiq-progress absolute inset-x-0 top-0 z-10" role="progressbar" aria-label="Loading key" />}
        {body()}
      </div>

      <StatusBar
        left={<StagedCountChips counts={counts} />}
        center={
          paged ? (
            <span className="inline-flex items-center gap-1.5 text-[11px] text-muted">
              <span className="tabular-nums text-text">
                {num(items.length)} of {num(data!.size)} loaded
              </span>
              <StatusBarButton onClick={() => void loadMore()} disabled={!next || more} title={next ? `Load ${PAGE} more` : "Everything is loaded"}>
                {more ? "Loading…" : `Load ${PAGE} more`}
              </StatusBarButton>
            </span>
          ) : null
        }
        right={
          !data || kind === "none" ? null : readOnly ? (
            <span className="inline-flex items-center gap-1 text-muted" title={data.truncated ? "Over 1 MiB: shown cut, so it can't be written back" : binaryValue ? "Not valid UTF-8: shown lossy, so it can't be edited here" : "This type can't be edited here"}>
              <StatusDot tone="muted" />
              <span>read-only{data.truncated ? " · over 1 MiB" : binaryValue ? " · binary" : ""}</span>
            </span>
          ) : (
            <span className="inline-flex items-center gap-1 text-ok">
              <StatusDot tone="ok" />
              <span>editable</span>
            </span>
          )
        }
      />
    </div>
  );
}
