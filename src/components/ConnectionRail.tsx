import { useState } from "react";
import { createPortal } from "react-dom";
import type { Engine } from "../lib/commands";
import { closeConnection, useConnections } from "../state/connectionStore";
import { isFileEngine } from "./ConnectionManager";
import { ContextMenu } from "./ui/ContextMenu";
import { IconClose, IconDatabase, IconSliders } from "./ui/Icon";

const LABEL_KEY = "tiq:rail:label";
type Label = "database" | "connection";

/**
 * Far-left rail of the *open* connections (TablePlus parity). Only shown once two
 * or more are open; clicking one brings it to the front without reconnecting.
 */
export function ConnectionRail() {
  const { open, active, saved, switchTo } = useConnections();
  const [label, setLabel] = useState<Label>(() =>
    localStorage.getItem(LABEL_KEY) === "connection" ? "connection" : "database",
  );
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);

  if (open.length < 2) return null;

  const pickLabel = (l: Label) => {
    localStorage.setItem(LABEL_KEY, l);
    setLabel(l);
  };

  return (
    <nav
      aria-label="Open connections"
      className="tiq-vibrancy flex shrink-0 flex-col border-r border-border"
      style={{ width: "var(--rail-w)" }}
    >
      <div className="flex flex-1 flex-col gap-1 overflow-y-auto px-1.5 py-2">
        {open.map(({ info }) => {
          const isActive = active?.id === info.id;
          const color = saved.find((c) => c.id === info.id)?.color;
          // A file engine's "database" is its full path: show just the file name.
          const db = isFileEngine(info.engine as Engine)
            ? info.database.split(/[\\/]/).pop()
            : info.database;
          const text = label === "database" ? db || info.name : info.name;
          return (
            <div key={info.id} className="group relative">
              <button
                onClick={() => switchTo(info.id)}
                aria-current={isActive ? "true" : undefined}
                title={`${info.name} · ${info.engine} · ${info.database}`}
                className={`flex w-full flex-col items-center gap-1 rounded-[var(--radius)] px-1 py-2 transition-colors ${
                  isActive
                    ? "bg-bg text-text shadow-[var(--shadow-soft)]"
                    : "text-muted hover:bg-surface-2 hover:text-text"
                }`}
              >
                <IconDatabase size={20} style={color ? { color } : undefined} />
                <span className="w-full truncate text-center text-[11px] leading-tight">{text}</span>
              </button>
              <button
                onClick={() => void closeConnection(info.id)}
                aria-label={`Close ${info.name}`}
                title="Close connection"
                className="absolute right-0.5 top-0.5 hidden h-4 w-4 items-center justify-center rounded-full bg-surface-2 text-muted hover:text-text group-focus-within:flex group-hover:flex"
              >
                <IconClose size={9} />
              </button>
            </div>
          );
        })}
      </div>
      <button
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          setMenu({ x: r.left, y: r.top - 4 });
        }}
        aria-label="Rail options"
        title="Rail options"
        className="mx-auto mb-2 flex h-6 w-6 items-center justify-center rounded text-muted hover:bg-surface-2 hover:text-text"
      >
        <IconSliders size={14} />
      </button>
      {/* Portalled: the rail's backdrop-filter would otherwise be the containing block
          for this fixed menu, offsetting it and trapping it under the sidebar. */}
      {menu &&
        createPortal(
          <ContextMenu
            x={menu.x}
            y={menu.y}
            onClose={() => setMenu(null)}
            items={[
              {
                id: "rail-db",
                label: "Show database name",
                checked: label === "database",
                onSelect: () => pickLabel("database"),
              },
              {
                id: "rail-conn",
                label: "Show connection name",
                checked: label === "connection",
                onSelect: () => pickLabel("connection"),
              },
            ]}
          />,
          document.body,
        )}
    </nav>
  );
}
