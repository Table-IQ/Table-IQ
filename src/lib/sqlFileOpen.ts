import { create } from "zustand";
import { useEffect } from "react";
import { canPickFiles } from "./commands";
import { useConnections } from "../state/connectionStore";
import { tabGuard } from "../state/tabGuard";

export interface OpenedSqlFile {
  id: number;
  name: string;
  sql: string | null;
  error: string | null;
}

interface SqlFilesState {
  files: OpenedSqlFile[];
  errors: string[];
  opening: boolean;
  paused: boolean;
  receive: (files: OpenedSqlFile[]) => void;
  dismiss: () => void;
  retry: () => void;
}

export const useSqlFiles = create<SqlFilesState>((set) => ({
  files: [], errors: [], opening: false, paused: false,
  receive: (files) => set((s) => ({
    files: [...s.files, ...files.filter((f) => typeof f.sql === "string")],
    errors: [...s.errors, ...files.filter((f) => f.error).map((f) => `${f.name}: ${f.error}`)],
  })),
  dismiss: () => set({ files: [], errors: [], paused: false }),
  retry: () => set({ paused: false, errors: [] }),
}));

/** The engines rendered by MainShell, where a SQL editor exists. */
export function hasSqlEditor(engine: string | undefined): boolean {
  return engine !== undefined && ["postgres", "mysql", "sqlserver", "sqlite", "duckdb", "clickhouse", "libsql", "rqlite", "influxdb"].includes(engine);
}

type Unlisten = () => void;
/** Listen before draining: a startup event may precede the webview, and an event
 *  during the first drain must not fall into a subscribe/read gap. Native drains
 *  are atomic, so racing reads never duplicate a file. Once started, a read still
 *  delivers to the global store after cleanup (StrictMode cannot lose its files). */
export function sqlFileBridge(
  subscribe: (ready: () => void) => Promise<Unlisten>,
  take: () => Promise<OpenedSqlFile[]>,
  receive: (files: OpenedSqlFile[]) => void,
  onError: (e: unknown) => void,
): Unlisten {
  let cancelled = false;
  let off: Unlisten | undefined;
  const drain = () => {
    if (!cancelled) void take().then(receive).catch(onError);
  };
  void subscribe(drain).then((stop) => {
    if (cancelled) { stop(); return; }
    off = stop;
    drain();
  }).catch(onError);
  return () => { cancelled = true; off?.(); };
}

export function useSqlFileBridge(): void {
  useEffect(() => {
    if (!canPickFiles()) return;
    return sqlFileBridge(
      async (ready) => {
        const { listen } = await import("@tauri-apps/api/event");
        return listen("tiq:sql-files-ready", ready);
      },
      async () => {
        const { invoke } = await import("@tauri-apps/api/core");
        return invoke<OpenedSqlFile[]>("take_opened_sql_files");
      },
      (files) => useSqlFiles.getState().receive(files),
      () => useSqlFiles.setState((s) => ({ errors: [...s.errors, "SQL files could not be opened. Try Open With TableIQ again."] })),
    );
  }, []);
}

/** Drain only from a hydrated SQL shell. The async guard must finish before
 *  acknowledging each file; refusing it keeps the contents ready for an explicit
 *  retry, without repeatedly prompting or replacing any existing query tab. */
export async function openPendingSqlFiles(
  connId: string,
  open = tabGuard.openQueryForConnection,
  currentConnection = () => useConnections.getState().active?.id,
): Promise<void> {
  const state = useSqlFiles.getState();
  if (state.opening || state.paused || !state.files.length) return;
  useSqlFiles.setState({ opening: true });
  try {
    while (useSqlFiles.getState().files.length) {
      if (currentConnection() !== connId) {
        useSqlFiles.setState({ paused: true });
        break;
      }
      const file = useSqlFiles.getState().files[0];
      if (!(await open(connId, file.sql ?? "", file.name))) {
        useSqlFiles.setState({ paused: true });
        break;
      }
      useSqlFiles.setState((s) => ({ files: s.files.filter((f) => f.id !== file.id) }));
    }
  } catch {
    useSqlFiles.setState((s) => ({ paused: true, errors: [...s.errors, "Could not open the script. Your file is still waiting; choose Open in this connection to retry."] }));
  } finally {
    useSqlFiles.setState({ opening: false });
  }
}
