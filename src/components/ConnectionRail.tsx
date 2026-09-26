import { useState } from "react";
import { disconnect } from "../lib/commands";
import { logError } from "../lib/report";
import { useConnections } from "../state/connectionStore";
import { ContextMenu } from "./ui/ContextMenu";
import { IconClose, IconDatabase, IconSliders } from "./ui/Icon";

const LABEL_KEY = "tiq:rail:label";
type Label = "database" | "connection";

/**
 * Far-left rail of the *open* connections (TablePlus parity). Only shown once two
 * or more are open; clicking one brings it to the front without reconnecting.
 */
export function ConnectionRail() {
  const { open, active, saved, switchTo, closeSession } = useConnections();
  const [label, setLabel] = useState<Label>(() =>
    localStorage.getItem(LABEL_KEY) === "connection" ? "connection" : "database",
  );
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);

  if (open.length < 2) return null;

  const pickLabel = (l: Label) => {
    localStorage.setItem(LABEL_KEY, l);
    setLabel(l);
  };
  const onClose = (id: string) =>
    disconnect(id)
      .catch((e) => logError("connection rail: disconnect", e))
      .finally(() => closeSession(id));

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
          const text = label === "database" ? info.database || info.name : info.name;
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
                onClick={() => void onClose(info.id)}
                aria-label={`Close ${info.name}`}
                title="Close connection"
                className="absolute right-0.5 top-0.5 hidden h-4 w-4 items-center justify-center rounded-full bg-surface-2 text-muted hover:text-text group-hover:flex"
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
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            {
              id: "rail-db",
              label: `${label === "database" ? "✓ " : "   "}Show database name`,
              onSelect: () => pickLabel("database"),
            },
            {
              id: "rail-conn",
              label: `${label === "connection" ? "✓ " : "   "}Show connection name`,
              onSelect: () => pickLabel("connection"),
            },
          ]}
        />
      )}
    </nav>
  );
}
