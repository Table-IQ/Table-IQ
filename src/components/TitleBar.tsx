import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ConnConfig } from "../lib/commands";
import { isReadOnlyEngine } from "../lib/profiles";
import { envTone, pillSegments, type PillSegments } from "../lib/connEnv";
import { useStaged } from "../state/stagedStore";
import { useSettingsUi } from "./Settings";
import { StagedReview } from "./StagedReview";
import { Capsule, CapsuleButton, CapsuleDivider } from "./ui/Capsule";
import { ColorDot } from "./ui/Chip";
import { ContextMenu, type ContextMenuItem } from "./ui/ContextMenu";
import {
  IconCommitList,
  IconDatabase,
  IconDiscard,
  IconErd,
  IconEye,
  IconPlug,
  IconPlus,
  IconPower,
  IconSidebar,
  IconSidebarRight,
  IconSliders,
  IconStop,
} from "./ui/Icon";

/** One customizable G4 item, already filtered and ordered by the user's toolbar prefs. */
export interface TitleBarItem {
  id: string;
  label: string;
  icon?: ReactNode;
  onClick?: () => void;
  shortcut?: string;
  active?: boolean;
  /** Items that are their own stateful component (ThemeToggle). */
  render?: () => ReactNode;
}

export interface TitleBarProps {
  cfg: Pick<ConnConfig, "engine" | "profile" | "sslmode" | "ssh" | "database" | "name" | "env" | "color">;
  /** "schema.table" of the active table tab. */
  activeObject: string | null;
  fileEngine: boolean;
  /** Show the spinner + "Loading schema…" in the pill. */
  connecting: boolean;
  showSidebar: boolean;
  showInspector: boolean;
  /** G4, from `titlebarActions` ∘ `visibleTitlebarIds`. */
  items: TitleBarItem[];
  canBackup: boolean;
  canRestore: boolean;
  onToggleSidebar(): void;
  onToggleInspector(): void;
  /** ⌘O picker. */
  onOpenConnection(): void;
  /** ⌘D picker. */
  onOpenDatabase(): void;
  onNewQuery(): void;
  onOpenErd(): void;
  onNewTable(): void;
  onImport(): void;
  onDiff(): void;
  onBackup(): void;
  onRestore(): void;
  onCancelQueries(): void;
  /** Header right-click → Settings → Toolbar. */
  onCustomizeToolbar(): void;
  onDisconnect(): void;
}

/** The pill's full text for its tooltip — never truncated, database always included. */
export function pillText(seg: PillSegments): string {
  const rest = [seg.engine, seg.ssl && "SSL", seg.ssh && "SSH", seg.connection, seg.database, seg.object].filter(Boolean);
  return seg.env ? `${seg.env} | ${rest.join(" : ")}` : rest.join(" : ");
}

const ICON = { size: 20, strokeWidth: 1.5 } as const;

type Menu = { x: number; y: number; items: (ContextMenuItem | "separator")[] };

function Sep({ children }: { children: string }) {
  return (
    <span aria-hidden className="shrink-0 opacity-45">
      {children}
    </span>
  );
}

/** TablePlus-style titlebar: glass capsule groups around an env-tinted status pill.
 *  G1 sidebar/connection · G2 staged edits · G3 database/SQL/ERD/more · pill ·
 *  G4 the user's customizable items + Cancel · G5 settings/inspector/disconnect. */
export function TitleBar(p: TitleBarProps) {
  const [menu, setMenu] = useState<Menu | null>(null);
  const [review, setReview] = useState(false);
  const moreRef = useRef<HTMLSpanElement>(null);
  const overflowRef = useRef<HTMLSpanElement>(null);

  const { count, committing, api } = useStaged();
  const noStaged = count === 0 || committing || !api;
  // The popover closes once nothing is staged (commit landed, or discarded); a failed
  // commit leaves the count up, so it stays open while the grid reports the error.
  useEffect(() => {
    if (count === 0) setReview(false);
  }, [count]);

  const seg = pillSegments(p.cfg, p.activeObject);
  const tone = envTone(p.cfg.env);

  const menuBelow = (el: HTMLElement | null, items: Menu["items"]) => {
    const r = el?.getBoundingClientRect();
    if (r) setMenu({ x: r.left, y: r.bottom + 4, items });
  };

  const reason = committing ? "Committing…" : noStaged ? "No staged changes" : undefined;

  return (
    <header
      data-tauri-drag-region
      className="tiq-titlebar-drag flex shrink-0 items-center gap-2 border-b border-border bg-surface pl-[84px] pr-3"
      style={{ height: "var(--titlebar-h)" }}
      onContextMenu={(e) => {
        e.preventDefault();
        // Only empty header space. Capsules, the pill and the review popover are all
        // `.tiq-no-drag` — which also covers a disabled button's tooltip wrapper span.
        if ((e.target as HTMLElement).closest(".tiq-no-drag")) return;
        setMenu({
          x: e.clientX,
          y: e.clientY,
          items: [{ id: "customize-toolbar", label: "Customize Toolbar…", onSelect: p.onCustomizeToolbar }],
        });
      }}
    >
      {/* G1 */}
      <Capsule>
        <CapsuleButton
          icon={<IconSidebar {...ICON} />}
          label="Toggle sidebar"
          shortcut="⌘["
          active={p.showSidebar}
          onClick={p.onToggleSidebar}
        />
        <CapsuleDivider />
        <CapsuleButton icon={<IconPlug {...ICON} />} label="Open connection" shortcut="⌘O" onClick={p.onOpenConnection} />
      </Capsule>

      {/* G2 — only TableView publishes to useStaged; query-result edits never drive this. */}
      <Capsule>
        <CapsuleButton
          icon={<IconDiscard {...ICON} />}
          label="Discard changes"
          disabled={noStaged}
          title={reason}
          onClick={() => api?.discard()}
        />
        <span className="relative inline-flex">
          <CapsuleButton
            icon={<IconEye {...ICON} />}
            label="Review changes"
            active={review}
            disabled={noStaged}
            title={reason}
            onClick={() => setReview((v) => !v)}
          />
          {review && api && count > 0 && (
            <StagedReview
              sql={api.preview()}
              total={count}
              committing={committing}
              commitBlockedReason={api.commitBlockedReason}
              onCommit={api.commit}
              onDiscard={api.discard}
              onClose={() => setReview(false)}
            />
          )}
        </span>
        <CapsuleDivider />
        <CapsuleButton
          icon={
            <span className="relative inline-flex">
              <IconCommitList {...ICON} />
              {count > 0 && (
                <span className="absolute -right-2 -top-1.5 inline-flex h-[14px] min-w-[14px] items-center justify-center rounded-full bg-text px-1 text-[10px] font-medium leading-none text-bg">
                  {count}
                </span>
              )}
            </span>
          }
          label="Commit changes"
          shortcut="⌘S"
          disabled={noStaged || !!api?.commitBlockedReason}
          title={api?.commitBlockedReason ?? reason}
          onClick={() => api?.commit()}
        />
      </Capsule>

      {/* G3 */}
      <Capsule>
        {!p.fileEngine && (
          <>
            <CapsuleButton icon={<IconDatabase {...ICON} />} label="Open database" shortcut="⌘D" onClick={p.onOpenDatabase} />
            <CapsuleDivider />
          </>
        )}
        <CapsuleButton
          icon={<span className="font-mono text-[12px] font-medium tracking-tight">SQL</span>}
          label="New query"
          shortcut="⌘T"
          onClick={p.onNewQuery}
        />
        <CapsuleButton icon={<IconErd {...ICON} />} label="ERD" onClick={p.onOpenErd} />
        <CapsuleDivider />
        <span ref={moreRef} className="inline-flex">
          <CapsuleButton
            icon={<IconPlus {...ICON} />}
            label="More"
            title="New table, import, diff, backup…"
            onClick={() =>
              menuBelow(moreRef.current, [
                // A read-only engine has no write path: nothing to create or import into.
                ...(isReadOnlyEngine(p.cfg.engine)
                  ? []
                  : [
                      { id: "new-table", label: "New Table…", onSelect: p.onNewTable },
                      { id: "import", label: "Import CSV / JSON…", onSelect: p.onImport },
                    ]),
                { id: "diff", label: "Diff with another connection…", onSelect: p.onDiff },
                ...((p.canBackup || p.canRestore ? ["separator"] : []) as "separator"[]),
                ...(p.canBackup ? [{ id: "backup", label: "Backup Database…", onSelect: p.onBackup }] : []),
                ...(p.canRestore ? [{ id: "restore", label: "Restore Database…", onSelect: p.onRestore }] : []),
              ])
            }
          />
        </span>
      </Capsule>

      {/* Status pill — env-tinted; full text always in the tooltip. */}
      <div
        data-testid="conn-capsule"
        title={pillText(seg)}
        className="tiq-no-drag flex h-9 min-w-[140px] flex-1 items-center gap-2 overflow-hidden rounded-full border px-4 font-mono text-[12px]"
        style={{
          background: `var(--env-${tone})`,
          color: `var(--env-${tone}-text)`,
          borderColor: "var(--glass-border)",
        }}
      >
        {seg.env && (
          <>
            <span className="shrink-0 font-medium uppercase">{seg.env}</span>
            <Sep>|</Sep>
          </>
        )}
        {p.connecting ? (
          <>
            <span
              aria-hidden
              className="inline-block h-3 w-3 shrink-0 animate-spin rounded-full border-[1.5px] border-current border-t-transparent opacity-80"
            />
            <span className="min-w-0 truncate">{`Connecting to ${seg.connection}… Loading schema…`}</span>
          </>
        ) : (
          <>
            <span className="shrink-0">{seg.engine}</span>
            {seg.ssl && (
              <>
                <Sep>:</Sep>
                <span className="shrink-0">SSL</span>
              </>
            )}
            {seg.ssh && (
              <>
                <Sep>:</Sep>
                <span className="shrink-0">SSH</span>
              </>
            )}
            <Sep>:</Sep>
            {p.cfg.color && <ColorDot color={p.cfg.color} size={7} />}
            <button
              type="button"
              onClick={p.onOpenConnection}
              title="Open connection (⌘O)"
              className="min-w-0 shrink truncate rounded-md px-1 hover:bg-text/8"
            >
              {seg.connection}
            </button>
            {seg.database && seg.database !== seg.connection && (
              <>
                <Sep>:</Sep>
                {p.fileEngine ? (
                  <span data-testid="breadcrumb-db" className="min-w-0 shrink-[2] truncate">
                    {seg.database}
                  </span>
                ) : (
                  <button
                    type="button"
                    data-testid="breadcrumb-db"
                    onClick={p.onOpenDatabase}
                    title="Open database (⌘D)"
                    className="min-w-0 shrink-[2] truncate rounded-md px-1 hover:bg-text/8"
                  >
                    {seg.database}
                  </button>
                )}
              </>
            )}
            {seg.object && (
              <>
                <Sep>:</Sep>
                <span className="min-w-0 shrink-[3] truncate">{seg.object}</span>
              </>
            )}
          </>
        )}
      </div>

      {/* G4 — the user's customizable items. Below 1180px only the first two show
          and the rest move under »; the breakpoint lives only in index.css. */}
      <Capsule>
        {p.items.map((it, i) => {
          const extra = i >= 2 ? "tiq-g4-extra" : "";
          return it.render ? (
            <span key={it.id} className={`inline-flex ${extra}`}>
              {it.render()}
            </span>
          ) : (
            <CapsuleButton
              key={it.id}
              icon={it.icon}
              label={it.label}
              shortcut={it.shortcut}
              active={it.active}
              onClick={it.onClick}
              className={extra}
            />
          );
        })}
        {p.items.length > 2 && (
          <span ref={overflowRef} className="tiq-g4-more">
            <CapsuleButton
              icon={<span className="text-[15px] leading-none">»</span>}
              label="More toolbar items"
              onClick={() =>
                menuBelow(
                  overflowRef.current,
                  p.items.slice(2).map((it) => ({
                    id: it.id,
                    label: it.label,
                    shortcut: it.shortcut,
                    disabled: !it.onClick,
                    onSelect: it.onClick,
                  })),
                )
              }
            />
          </span>
        )}
        {p.items.length > 0 && <CapsuleDivider />}
        <CapsuleButton
          icon={<IconStop {...ICON} />}
          label="Cancel running queries"
          shortcut="⌘."
          onClick={p.onCancelQueries}
        />
      </Capsule>

      {/* G5 */}
      <Capsule>
        <CapsuleButton
          icon={<IconSliders {...ICON} />}
          label="Settings"
          shortcut="⌘,"
          onClick={() => useSettingsUi.getState().show()}
        />
        <CapsuleButton
          icon={<IconSidebarRight {...ICON} />}
          label="Toggle inspector"
          shortcut="⌘]"
          active={p.showInspector}
          onClick={p.onToggleInspector}
        />
        <CapsuleDivider />
        <CapsuleButton icon={<IconPower {...ICON} />} label="Disconnect" onClick={p.onDisconnect} />
      </Capsule>

      {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
    </header>
  );
}
