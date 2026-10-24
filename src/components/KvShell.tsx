import { useCallback, useEffect, useRef, useState } from "react";
import { ConnectionRail } from "./ConnectionRail";
import { Wordmark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { listDatabases } from "../lib/commands";
import { canLeaveSession, closeConnection, switchDatabaseFlow, TAB_GUARD_LABELS, useConnections } from "../state/connectionStore";
import { shouldSwitchDatabase } from "../lib/databasePicker";
import { loadedLabel } from "../lib/kvTree";
import { loadKvPrefs, saveKvPrefs, type KvPrefs } from "../lib/kvView";
import type { StringFormat } from "../lib/kvFormat";
import { StatusBar, StatusDot } from "./ui/StatusBar";
import { ColorDot } from "./ui/Chip";
import { IconButton } from "./ui/IconButton";
import { Tabs } from "./ui/Panel";
import { IconRefresh } from "./ui/Icon";
import { DatabasePicker } from "./DatabasePicker";
import { KvKeyTree } from "./KvKeyTree";
import { KvKey } from "./KvKey";
import { KvConsole } from "./KvConsole";

type Pane = "key" | "console";

/**
 * Redis shell — same chrome as MainShell / DocShell: native titlebar region, wordmark + connection chip,
 * a key sidebar (SCAN pages folded on `:`), and for the open key a Key pane and a Console pane.
 */
export function KvShell() {
  const { active, saved } = useConnections();
  const connecting = useConnections((s) => s.connecting);
  const activeColor = active ? saved.find((c) => c.id === active.id)?.color ?? null : null;
  const id = active?.id;
  const database = active?.database ?? "0";

  const [prefs, setPrefs] = useState<KvPrefs>(() => loadKvPrefs(id ?? ""));
  const [selected, setSelected] = useState<string | null>(null);
  const [pane, setPane] = useState<Pane>("key");
  const [refreshKey, setRefreshKey] = useState(0);
  const [status, setStatus] = useState<{ loaded: number; more: boolean; dbsize: number | null }>({ loaded: 0, more: false, dbsize: null });
  const [dbPicker, setDbPicker] = useState<{ items: string[]; loading: boolean; error: string | null } | null>(null);
  const [connMsg, setConnMsg] = useState<string | null>(null);

  const patch = (p: Partial<KvPrefs>) =>
    setPrefs((cur) => {
      const n = { ...cur, ...p };
      if (id) saveKvPrefs(id, n);
      return n;
    });

  // Another database is another keyspace: drop the open key and re-scan.
  const lastDb = useRef(database);
  useEffect(() => {
    if (lastDb.current === database) return;
    lastDb.current = database;
    setSelected(null);
    setRefreshKey((k) => k + 1);
  }, [database]);

  const open = async (key: string) => {
    if (key === selected) return setPane("key");
    // Leaving the key drops its staged edits; ask first (same prompt as the SQL tab guard).
    if (!(await canLeaveSession("This key has unsaved changes. Discard them and switch keys?", TAB_GUARD_LABELS))) return;
    setSelected(key);
    setPane("key");
  };
  const gone = useCallback((to?: string) => {
    setRefreshKey((k) => k + 1);
    setSelected(to ?? null);
  }, []);

  const openDbPicker = async () => {
    if (!id) return;
    setDbPicker({ items: [], loading: true, error: null });
    try {
      setDbPicker({ items: await listDatabases(id), loading: false, error: null });
    } catch (e) {
      setDbPicker({ items: [], loading: false, error: String(e) });
    }
  };
  const pickDatabase = async (db: string) => {
    if (!id || useConnections.getState().connecting || !shouldSwitchDatabase(db, database)) return;
    if (!(await canLeaveSession())) return;
    setDbPicker(null);
    setConnMsg(null);
    switchDatabaseFlow(id, db).catch((e) => setConnMsg(`Could not switch to db ${db}: ${e}`));
  };

  const onStatus = useCallback((s: { loaded: number; more: boolean; dbsize: number | null }) => setStatus(s), []);

  if (!active || !id) return null;
  const switching = connecting?.id === id && connecting.stage === "switch";

  return (
    <div className="flex h-full flex-col">
      <header
        className="tiq-chrome tiq-titlebar-drag flex shrink-0 items-center gap-3 border-b border-border bg-surface pl-[88px] pr-3"
        style={{ height: "var(--header-h)" }}
      >
        <Wordmark className="text-sm" />
        <span className="inline-flex items-center gap-1.5 rounded-md bg-bg/60 px-1.5 py-0.5">
          {activeColor && <ColorDot color={activeColor} />}
          <span className="text-[11px] font-medium text-text">{active.name}</span>
        </span>
        <button
          onClick={() => void openDbPicker()}
          disabled={switching}
          title="Switch database (⌘D)"
          className="tiq-no-drag rounded-md px-1 font-mono text-[11px] text-muted hover:bg-surface-2 hover:text-text disabled:opacity-60"
        >
          redis · db {database}
          {switching ? " · switching…" : ""}
        </button>
        {connMsg && (
          <span role="alert" className="min-w-0 truncate text-[11px] text-danger" title={connMsg}>
            {connMsg}
          </span>
        )}
        <div className="ml-auto flex items-center gap-1">
          <IconButton icon={<IconRefresh />} label="Rescan keys" onClick={() => setRefreshKey((k) => k + 1)} />
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
        <aside className="tiq-vibrancy flex shrink-0 flex-col border-r border-border" style={{ width: "var(--sidebar-w)" }}>
          <KvKeyTree
            key={id}
            connId={id}
            pattern={prefs.pattern}
            onPattern={(pattern) => patch({ pattern })}
            selected={selected}
            onSelect={(k) => void open(k)}
            refreshKey={refreshKey}
            onStatus={onStatus}
          />
        </aside>

        <main className="flex min-w-0 flex-1 flex-col overflow-hidden bg-bg">
          <Tabs
            tabs={[
              { key: "key", label: "Key" },
              { key: "console", label: "Console" },
            ]}
            active={pane}
            onSelect={(k) => setPane(k as Pane)}
          />
          {/* Both stay mounted: switching panes must not drop staged edits or the console's output. */}
          <div className="min-h-0 flex-1">
            {selected ? (
              <KvKey
                key={`${id}.${database}.${selected}`}
                connId={id}
                keyName={selected}
                visible={pane === "key"}
                format={prefs.stringFormat}
                onFormat={(stringFormat: StringFormat) => patch({ stringFormat })}
                onGone={gone}
              />
            ) : (
              pane === "key" && <div className="flex h-full items-center justify-center text-sm text-muted">Select a key to view its value.</div>
            )}
            <KvConsole key={`c.${id}`} connId={id} database={database} visible={pane === "console"} />
          </div>
          {(!selected || pane === "console") && (
            <StatusBar
              left={
                <>
                  <StatusDot tone="ok" />
                  <span>Connected · redis</span>
                </>
              }
              center={
                <span className="tabular-nums">
                  {loadedLabel(status.loaded, status.more)} keys loaded{status.dbsize !== null ? ` of ${status.dbsize.toLocaleString("en-US")}` : ""}
                </span>
              }
              right={<span>Enter in the pattern box re-scans</span>}
            />
          )}
        </main>
      </div>

      {dbPicker && (
        <DatabasePicker
          databases={dbPicker.items}
          activeDatabase={database}
          busy={dbPicker.loading}
          error={dbPicker.error}
          onPick={(db) => void pickDatabase(db)}
          onClose={() => setDbPicker(null)}
        />
      )}
    </div>
  );
}
