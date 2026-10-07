import { useCallback, useEffect, useState } from "react";
import { ConnectionRail } from "./ConnectionRail";
import { Wordmark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { docCollections, docCount } from "../lib/commands";
import { canLeaveSession, closeConnection, TAB_GUARD_LABELS, useConnections } from "../state/connectionStore";
import { StatusBar, StatusDot } from "./ui/StatusBar";
import { ColorDot } from "./ui/Chip";
import { Section } from "./ui/Section";
import { IconButton } from "./ui/IconButton";
import { Tabs } from "./ui/Panel";
import { IconRefresh, IconSearch, IconView } from "./ui/Icon";
import { DocCollection } from "./DocCollection";
import { DocIndexes } from "./DocIndexes";
import { DocAggregate } from "./DocAggregate";
import { DocMapping } from "./DocMapping";

const COMPACT = new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 });
type Pane = "documents" | "indexes" | "mapping" | "aggregate";

/** Mongo / DocumentDB shell — same chrome treatment as MainShell / KvShell: a collection sidebar
 *  (filter, doc counts), and for the open collection Documents / Indexes / Aggregate panes.
 *  Elasticsearch / OpenSearch use it too, read-only (the session's `read_only`): indices are the collections,
 *  and Mapping replaces Indexes. */
export function DocShell() {
  const { active, saved } = useConnections();
  const cfg = active ? saved.find((c) => c.id === active.id) : undefined;
  const activeColor = cfg?.color ?? null;
  const [collections, setCollections] = useState<string[]>([]);
  const [counts, setCounts] = useState<Record<string, number>>({});
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [pane, setPane] = useState<Pane>("documents");
  const [listError, setListError] = useState<string | null>(null);

  const id = active?.id;
  const elastic = active?.engine === "elasticsearch";
  const readOnly = !!active?.read_only;
  const kind = elastic ? "indices" : "collections";
  // The product the server reported at connect (OpenSearch speaks the same API).
  const engineName = active?.profile === "opensearch" ? "opensearch" : elastic ? "elasticsearch" : "mongo";
  // A URL connection (mongodb+srv://, several hosts) may report no database; the saved config has it.
  const database = active?.database || cfg?.database || "";

  const load = useCallback(() => {
    if (!id) return;
    docCollections(id)
      .then((names) => {
        const sorted = [...names].sort((a, b) => a.localeCompare(b));
        setCollections(sorted);
        setListError(null);
        // Counts come from collection metadata (cheap, estimated); fetched a few at a time.
        const queue = [...sorted];
        const worker = async () => {
          for (let c = queue.shift(); c !== undefined; c = queue.shift()) {
            const name = c;
            await docCount(id, name)
              .then((n) => setCounts((m) => ({ ...m, [name]: n })))
              .catch(() => {});
          }
        };
        void Promise.all([worker(), worker(), worker(), worker()]);
      })
      .catch((e) => {
        setCollections([]);
        setListError(String(e));
      });
  }, [id]);
  useEffect(load, [load]);

  const open = async (c: string) => {
    if (c === selected) return;
    // Leaving the collection drops its staged changes; ask first (same prompt as the SQL tab guard).
    if (!(await canLeaveSession("This collection has unsaved changes. Discard them and switch collections?", TAB_GUARD_LABELS))) return;
    setSelected(c);
    setPane("documents");
  };

  if (!active || !id) return null;

  const filtered = filter ? collections.filter((c) => c.toLowerCase().includes(filter.toLowerCase())) : collections;

  return (
    <div className="flex h-full flex-col">
      <header
        className="tiq-titlebar-drag flex shrink-0 items-center gap-3 border-b border-border bg-surface pl-[88px] pr-3"
        style={{ height: "var(--header-h)" }}
      >
        <Wordmark className="text-sm" />
        <span className="inline-flex items-center gap-1.5 rounded-md bg-bg/60 px-1.5 py-0.5">
          {activeColor && <ColorDot color={activeColor} />}
          <span className="text-[11px] font-medium text-text">{active.name}</span>
        </span>
        <span className="font-mono text-[11px] text-muted">
          {engineName}
          {!elastic && database ? ` · ${database}` : ""}
        </span>
        <div className="ml-auto flex items-center gap-1">
          <IconButton icon={<IconRefresh />} label={`Reload ${kind}`} onClick={load} />
          <ThemeToggle />
          <button
            onClick={async () => {
              await closeConnection(active.id);
            }}
            className="rounded-md border border-border px-2 py-1 text-[11px] text-muted hover:text-text"
          >
            Disconnect
          </button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <ConnectionRail />
        <aside className="flex shrink-0 flex-col border-r border-border bg-surface" style={{ width: "var(--sidebar-w)" }}>
          <div className="border-b border-border p-2">
            <label className="flex items-center gap-1.5 rounded-md border border-border bg-bg px-2 focus-within:border-primary">
              <IconSearch size={12} className="text-muted" />
              <input
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder={`Filter ${kind}…`}
                className="h-6 flex-1 bg-transparent text-xs text-text placeholder:text-muted focus:outline-none"
              />
            </label>
          </div>
          <div className="min-h-0 flex-1 overflow-auto px-1.5 py-1">
            <Section label={elastic ? "Indices" : "Collections"} count={filtered.length}>
              {filtered.map((c) => (
                <button
                  key={c}
                  onClick={() => void open(c)}
                  className={`flex h-[var(--row-h)] w-full items-center gap-2 rounded-md px-2 text-left font-mono text-[12.5px] ${
                    selected === c ? "bg-selection-bg text-text" : "text-text hover:bg-surface-2"
                  }`}
                >
                  <IconView size={12} className="text-muted" />
                  <span className="min-w-0 flex-1 truncate font-mono">{c}</span>
                  {counts[c] !== undefined && (
                    <span className="shrink-0 rounded bg-bg px-1 text-[10px] tabular-nums text-muted" title={`${elastic ? "" : "~"}${counts[c].toLocaleString("en-US")} documents${elastic ? "" : " (estimated)"}`}>
                      {COMPACT.format(counts[c])}
                    </span>
                  )}
                </button>
              ))}
              {filtered.length === 0 && (
                <div className={`px-3 py-2 text-[11px] ${listError ? "text-danger" : "text-muted"}`} role={listError ? "alert" : undefined}>
                  {listError ?? `No ${kind}.`}
                </div>
              )}
            </Section>
          </div>
        </aside>

        <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
          {selected ? (
            <>
              <div className="flex shrink-0 items-center border-b border-border bg-surface">
                <span className="px-3 font-mono text-xs font-semibold text-text">{selected}</span>
                <div className="flex-1">
                  <Tabs
                    tabs={[
                      { key: "documents", label: "Documents" },
                      elastic ? { key: "mapping", label: "Mapping" } : { key: "indexes", label: "Indexes" },
                      { key: "aggregate", label: "Aggregate" },
                    ]}
                    active={pane}
                    onSelect={(k) => setPane(k as Pane)}
                  />
                </div>
              </div>
              {/* All three stay mounted: switching panes must not drop staged edits or a typed pipeline. */}
              <div className="min-h-0 flex-1">
                <DocCollection key={`${id}.${selected}`} connId={id} collection={selected} visible={pane === "documents"} readOnly={readOnly} />
                {elastic ? (
                  <DocMapping key={`m.${id}.${selected}`} connId={id} collection={selected} active={pane === "mapping"} />
                ) : (
                  <DocIndexes key={`i.${id}.${selected}`} connId={id} collection={selected} active={pane === "indexes"} />
                )}
                <DocAggregate key={`a.${id}.${selected}`} connId={id} collection={selected} active={pane === "aggregate"} elastic={elastic} />
              </div>
            </>
          ) : (
            <>
              <div className="flex flex-1 items-center justify-center text-sm text-muted">{`Select ${elastic ? "an index" : "a collection"} to view documents.`}</div>
              <StatusBar
                left={
                  <>
                    <StatusDot tone="ok" />
                    <span>Connected · {active.engine}</span>
                  </>
                }
                center={collections.length > 0 ? <span>{collections.length} {kind}</span> : null}
                right={<span>Reload {kind} in the header to re-list</span>}
              />
            </>
          )}
        </main>
      </div>
    </div>
  );
}
