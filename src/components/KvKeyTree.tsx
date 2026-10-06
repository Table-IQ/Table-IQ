import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { kvDbsize, kvScanPage, type KeyInfo } from "../lib/commands";
import { buildKeyTree, flattenTree, loadedLabel, mergeKeys, type KeyNode } from "../lib/kvTree";
import { kindLabel, ttlChip } from "../lib/kvFormat";
import { IconChevronDown, IconChevronRight, IconClose, IconSearch } from "./ui/Icon";

const PAGE_SIZES = [100, 300, 1000] as const;
/** A sparse MATCH can come back empty page after page: keep scanning, but never more than this per click. */
const MAX_SCANS_PER_LOAD = 8;
const AUTO_OPEN_BELOW = 60;
const num = (n: number) => n.toLocaleString("en-US");

/** The key sidebar: SCAN in pages (never the whole keyspace), folded on `:` into a collapsible tree. */
export function KvKeyTree({
  connId,
  pattern,
  onPattern,
  selected,
  onSelect,
  refreshKey,
  onStatus,
}: {
  connId: string;
  pattern: string;
  onPattern: (p: string) => void;
  selected: string | null;
  onSelect: (key: string) => void;
  refreshKey: number;
  /** Counts for the status bar. */
  onStatus: (s: { loaded: number; more: boolean; dbsize: number | null }) => void;
}) {
  const [draft, setDraft] = useState(pattern);
  const [keys, setKeys] = useState<KeyInfo[]>([]);
  const [cursor, setCursor] = useState("0");
  const [started, setStarted] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dbsize, setDbsize] = useState<number | null>(null);
  const [count, setCount] = useState<number>(300);
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  const gen = useRef(0);
  const keysRef = useRef<KeyInfo[]>([]);
  const cursorRef = useRef("0");
  const countRef = useRef(count);
  countRef.current = count;

  /** One click's worth: scan from `from` until `count` new keys, the end, or the scan budget. */
  const load = useCallback(
    async (reset: boolean, p: string) => {
      const my = ++gen.current;
      setLoading(true);
      setError(null);
      let acc: KeyInfo[] = reset ? [] : keysRef.current;
      let cur = reset ? "0" : cursorRef.current;
      const before = acc.length;
      const n = countRef.current;
      try {
        for (let i = 0; i < MAX_SCANS_PER_LOAD; i++) {
          const page = await kvScanPage(connId, p, cur, n);
          if (my !== gen.current) return;
          acc = mergeKeys(acc, page.keys);
          cur = page.nextCursor;
          if (cur === "0" || acc.length - before >= n) break;
        }
        keysRef.current = acc;
        cursorRef.current = cur;
        setKeys(acc);
        setCursor(cur);
        setStarted(true);
        if (reset && p.trim() !== "*" && acc.length <= AUTO_OPEN_BELOW) {
          const all = new Set<string>();
          const walk = (ns: KeyNode[]) => ns.forEach((n) => n.folder && (all.add(n.path), walk(n.children)));
          walk(buildKeyTree(acc));
          setOpen(all);
        }
      } catch (e) {
        if (my === gen.current) setError(String(e).replace(/^Error:\s*/, ""));
      } finally {
        if (my === gen.current) setLoading(false);
      }
    },
    [connId],
  );

  // A new connection / database / refresh / filter starts over from the first page.
  useEffect(() => {
    void load(true, pattern);
    let live = true;
    kvDbsize(connId)
      .then((n) => live && setDbsize(n))
      .catch(() => live && setDbsize(null));
    return () => {
      live = false;
    };
  }, [load, pattern, refreshKey, connId]);

  const more = cursor !== "0";
  useEffect(() => onStatus({ loaded: keys.length, more, dbsize }), [keys.length, more, dbsize, onStatus]);

  const tree = useMemo(() => buildKeyTree(keys), [keys]);
  const rows = useMemo(() => flattenTree(tree, open), [tree, open]);
  const toggle = (path: string) =>
    setOpen((o) => {
      const n = new Set(o);
      if (!n.delete(path)) n.add(path);
      return n;
    });
  const apply = (p: string) => {
    const t = p.trim() || "*";
    setDraft(t);
    onPattern(t);
  };

  return (
    <>
      <div className="border-b border-border p-2">
        <label className="flex items-center gap-1.5 rounded-md border border-border bg-bg px-2 focus-within:border-primary">
          <IconSearch size={12} className="text-muted" />
          <input
            aria-label="Key pattern"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && apply(draft)}
            placeholder="MATCH pattern, e.g. user:*"
            spellCheck={false}
            className="h-6 min-w-0 flex-1 bg-transparent font-mono text-[11.5px] text-text placeholder:text-muted focus:outline-none"
          />
          {pattern !== "*" && (
            <button onClick={() => apply("*")} aria-label="Clear pattern" title="Show every key" className="text-muted hover:text-text">
              <IconClose size={10} />
            </button>
          )}
        </label>
      </div>

      <div className="relative min-h-0 flex-1 overflow-auto px-1.5 py-1" data-testid="kv-tree">
        {loading && <div className="tiq-progress absolute inset-x-0 top-0 z-10" role="progressbar" aria-label="Scanning keys" />}
        {rows.map(({ node, depth }) => {
          const pad = { paddingLeft: 6 + depth * 12 };
          if (node.folder)
            return (
              <button
                key={node.path}
                onClick={() => toggle(node.path)}
                aria-expanded={open.has(node.path)}
                style={pad}
                title={`${node.path}*: ${num(node.count)}${more ? "+" : ""} loaded`}
                className="flex h-[var(--row-h)] w-full items-center gap-1 rounded-md pr-2 text-left font-mono text-[12.5px] text-text hover:bg-surface-2"
              >
                {open.has(node.path) ? <IconChevronDown size={10} className="shrink-0 text-muted" /> : <IconChevronRight size={10} className="shrink-0 text-muted" />}
                <span className="min-w-0 flex-1 truncate">{node.name || "(empty)"}</span>
                <span className="shrink-0 rounded bg-bg px-1 text-[10px] tabular-nums text-muted">{loadedLabel(node.count, more)}</span>
              </button>
            );
          const info = node.info!;
          const ttl = ttlChip(info.ttl);
          return (
            <button
              key={node.path}
              onClick={() => !info.binary && onSelect(info.key)}
              aria-disabled={info.binary || undefined}
              style={{ paddingLeft: 6 + depth * 12 + 11 }}
              title={info.binary ? "Binary key (not UTF-8): it can't be opened or edited here" : info.key}
              className={`flex h-[var(--row-h)] w-full items-center gap-1.5 rounded-md pr-2 text-left font-mono text-[12.5px] ${
                info.binary ? "cursor-not-allowed text-muted-2" : selected === info.key ? "bg-selection-bg text-text" : "text-text hover:bg-surface-2"
              }`}
            >
              <span className="min-w-0 flex-1 truncate">{node.name || "(empty)"}</span>
              {ttl && (
                <span className="shrink-0 text-[10px] tabular-nums text-warn" title={`expires in ${info.ttl}s`}>
                  {ttl}
                </span>
              )}
              <span className="shrink-0 rounded bg-bg px-1 text-[10px] text-muted">{kindLabel(info.kind)}</span>
            </button>
          );
        })}
        {error && (
          <div role="alert" className="px-2 py-2 text-[11px] text-danger">
            {error}
          </div>
        )}
        {started && !error && keys.length === 0 && !more && <div className="px-3 py-4 text-xs text-muted">{pattern === "*" ? "This database has no keys." : `No keys match ${pattern}.`}</div>}
      </div>

      <div className="shrink-0 border-t border-border px-2 py-1.5 text-[11px] text-muted">
        <div className="truncate tabular-nums" title="Keys loaded so far; Redis can't count a pattern without scanning it all">
          {loadedLabel(keys.length, more)} loaded{dbsize !== null ? ` of ${num(dbsize)} in this database` : ""}
        </div>
        <div className="mt-1 flex items-center gap-1.5">
          <select
            aria-label="Keys per scan"
            title="Keys per scan"
            value={count}
            onChange={(e) => setCount(Number(e.target.value))}
            className="h-5 rounded-[var(--radius-sm)] border border-border bg-bg px-1 text-[10.5px] text-muted hover:text-text"
          >
            {PAGE_SIZES.map((n) => (
              <option key={n} value={n}>
                {n} per scan
              </option>
            ))}
          </select>
          <button
            onClick={() => void load(false, pattern)}
            disabled={!more || loading}
            className="h-5 flex-1 rounded-[var(--radius-sm)] border border-border px-2 text-[10.5px] text-text hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-40"
          >
            Load more
          </button>
        </div>
      </div>
    </>
  );
}
