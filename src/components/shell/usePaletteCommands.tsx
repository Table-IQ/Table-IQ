import { useMemo, type Dispatch, type SetStateAction } from "react";
import type { Command } from "../CommandPalette";
import { isFileEngine } from "../ConnectionManager";
import { useSettingsUi } from "../Settings";
import {
  IconChip,
  IconDatabase,
  IconDiff,
  IconErd,
  IconExport,
  IconImport,
  IconLink,
  IconMetrics,
  IconOverview,
  IconPlay,
  IconPlus,
  IconSliders,
  IconSqlLog,
  IconStop,
  IconStructure,
  IconTable,
  IconUsers,
  IconView,
} from "../ui/Icon";
import type { ConnectedInfo, Engine, SavedQuery, SchemaModel } from "../../lib/commands";
import { savedQueriesForConn } from "../../lib/history";
import { supportsUserMgmt } from "../../lib/userMgmt";
import { supportsBackup, supportsRestore } from "../../lib/backup";
import type { ViewKind } from "../../state/tabsStore";

export interface UsePaletteCommandsOptions {
  active: ConnectedInfo | null;
  schema: SchemaModel | null;
  dbItems: string[];
  paletteSaved: SavedQuery[];
  openTable: (schema: string, table: string) => void;
  openQuery: (sql?: string, title?: string) => void;
  openErd: () => void;
  openView: (kind: ViewKind, title: string) => void;
  openDbPicker: () => Promise<void>;
  pickDatabase: (db: string) => Promise<void>;
  setShowConnPicker: Dispatch<SetStateAction<boolean>>;
  setShowWorkspaceOverview: Dispatch<SetStateAction<boolean>>;
  setShowSqlLog: Dispatch<SetStateAction<boolean>>;
  setShowStructure: Dispatch<SetStateAction<boolean>>;
  setShowImport: Dispatch<SetStateAction<boolean>>;
  setShowDiff: Dispatch<SetStateAction<boolean>>;
  setShowBackup: Dispatch<SetStateAction<boolean>>;
  setShowRestore: Dispatch<SetStateAction<boolean>>;
  cancelAllQueries: () => Promise<void>;
}

/**
 * The ⌘K command palette's `Command[]` (Commands, then Tables/Databases/Queries
 * groups — see `commandGroups.ts`). Extracted from `MainShell` (Task 11b) so
 * PR-C can add Backup/Restore/MCP entries without growing that file further.
 */
export function usePaletteCommands(options: UsePaletteCommandsOptions): Command[] {
  const {
    active,
    schema,
    dbItems,
    paletteSaved,
    openTable,
    openQuery,
    openErd,
    openView,
    openDbPicker,
    pickDatabase,
    setShowConnPicker,
    setShowWorkspaceOverview,
    setShowSqlLog,
    setShowStructure,
    setShowImport,
    setShowDiff,
    setShowBackup,
    setShowRestore,
    cancelAllQueries,
  } = options;

  return useMemo<Command[]>(() => {
    const list: Command[] = [
      { id: "act:erd", label: "Diagram", group: "Commands", icon: <IconErd />, run: openErd },
      // Hidden for engines with no user-management SQL (sqlite/duckdb/mongodb/redis).
      ...((active && supportsUserMgmt(active.engine as Engine)
        ? [
            {
              id: "act:users",
              label: "Database User Management",
              group: "Commands",
              icon: <IconUsers />,
              run: () => openView("users", "Users"),
            },
          ]
        : []) as Command[]),
      {
        id: "act:metrics",
        label: "Metrics Board",
        group: "Commands",
        icon: <IconMetrics />,
        run: () => openView("metrics", "Metrics Board"),
      },
      {
        id: "act:open-conn",
        label: "Open Connection",
        group: "Commands",
        icon: <IconLink />,
        shortcut: "⌘O",
        run: () => setShowConnPicker(true),
      },
      // A file engine (sqlite/duckdb) has no other database to switch to — same
      // condition that hides the Databases group below and no-ops openDbPicker.
      ...((!active || !isFileEngine(active.engine as Engine)
        ? [
            {
              id: "act:open-db",
              label: "Open Database",
              group: "Commands",
              icon: <IconDatabase />,
              shortcut: "⌘D",
              run: () => void openDbPicker(),
            },
          ]
        : []) as Command[]),
      {
        id: "act:workspace-overview",
        label: "Workspace Overview",
        group: "Commands",
        shortcut: "⌘⇧\\",
        run: () => setShowWorkspaceOverview(true),
      },
      {
        id: "act:sql-log",
        label: "Toggle SQL Log",
        group: "Commands",
        icon: <IconSqlLog />,
        shortcut: "⌘⇧C",
        run: () => setShowSqlLog((v) => !v),
      },
      {
        id: "act:settings",
        label: "Settings",
        group: "Commands",
        icon: <IconSliders />,
        shortcut: "⌘,",
        run: () => useSettingsUi.getState().show(),
      },
      {
        id: "act:mcp",
        label: "MCP Server",
        group: "Commands",
        icon: <IconChip />,
        run: () => useSettingsUi.getState().show("mcp"),
      },
      {
        id: "act:customize-toolbar",
        label: "Customize Toolbar",
        group: "Commands",
        icon: <IconOverview />,
        run: () => useSettingsUi.getState().show("toolbar"),
      },
      {
        id: "act:query",
        label: "New Query",
        group: "Commands",
        icon: <IconPlus />,
        shortcut: "⌘T",
        run: () => openQuery(),
      },
      {
        id: "act:table",
        label: "New Table…",
        group: "Commands",
        icon: <IconStructure />,
        run: () => setShowStructure(true),
      },
      {
        id: "act:import",
        label: "Import CSV / JSON…",
        group: "Commands",
        icon: <IconImport />,
        run: () => setShowImport(true),
      },
      {
        id: "act:diff",
        label: "Diff with another connection…",
        group: "Commands",
        icon: <IconDiff />,
        run: () => setShowDiff(true),
      },
      // Hidden for mongodb/redis, which have no backup here.
      ...((active && supportsBackup(active.engine as Engine)
        ? [
            {
              id: "act:backup",
              label: "Backup Database",
              group: "Commands",
              icon: <IconExport />,
              run: () => setShowBackup(true),
            },
          ]
        : []) as Command[]),
      // Hidden where there is nothing to restore into (mongodb/redis, an in-memory file db).
      ...((active && supportsRestore(active.engine as Engine, active.database)
        ? [
            {
              id: "act:restore",
              label: "Restore Database",
              group: "Commands",
              icon: <IconImport />,
              run: () => setShowRestore(true),
            },
          ]
        : []) as Command[]),
      {
        id: "act:cancel-all",
        label: "Cancel Running Queries",
        group: "Commands",
        icon: <IconStop />,
        shortcut: "⌘.",
        run: () => void cancelAllQueries(),
      },
    ];
    for (const s of schema?.schemas ?? []) {
      for (const o of s.objects) {
        list.push({
          id: `tbl:${s.name}.${o.name}`,
          label: `${s.name}.${o.name}`,
          hint: o.kind,
          group: "Tables",
          icon: o.kind === "View" ? <IconView /> : <IconTable />,
          run: () => openTable(s.name, o.name),
        });
      }
    }
    // File engines (sqlite/duckdb) are their own database — nothing to list or switch to.
    if (active && !isFileEngine(active.engine as Engine)) {
      for (const db of dbItems) {
        list.push({
          id: `db:${db}`,
          label: db,
          hint: db === active.database ? "current" : undefined,
          group: "Databases",
          icon: <IconDatabase />,
          run: () => void pickDatabase(db),
        });
      }
    }
    if (active) {
      for (const q of savedQueriesForConn(paletteSaved, active.id)) {
        list.push({
          id: `q:${q.id}`,
          label: q.name,
          group: "Queries",
          icon: <IconPlay />,
          run: () => openQuery(q.sql, q.name),
        });
      }
    }
    return list;
    // `active`, `openDbPicker` and `pickDatabase` are intentionally not deps — they
    // close over `active` fresh every render, and `schema`/`dbItems` already
    // change whenever a connection or database switch would make them stale.
    // Recomputing on every unrelated render (e.g. a keystroke) would churn the
    // palette unnecessarily.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [schema, openTable, openQuery, openErd, openView, dbItems, paletteSaved]);
}
