import { useEffect, useState } from "react";
import { listConnections, type ConnConfig } from "../lib/commands";
import { useConnections } from "../state/connectionStore";
import { IconChevronLeft, IconPlus } from "./ui/Icon";

/**
 * Far-left mini-rail of saved connections (TablePlus parity).
 * Two letters per chip; the connection's color tag is the dot on the corner.
 * Clicking switches via a fresh connect — handled in ConnectionManager.
 */
export function ConnectionRail({ onAdd, onCollapse }: { onAdd?: () => void; onCollapse?: () => void }) {
  const { saved, active, setSaved } = useConnections();
  const [hover, setHover] = useState<string | null>(null);

  useEffect(() => {
    listConnections().then(setSaved).catch(() => {});
  }, [setSaved]);

  const initials = (c: ConnConfig) =>
    (c.name || c.database || c.engine).slice(0, 2).toUpperCase();

  return (
    <aside
      className="flex shrink-0 flex-col items-center border-r border-border bg-surface py-2"
      style={{ width: "var(--rail-w)" }}
    >
      <div className="flex flex-1 flex-col items-center gap-1.5 overflow-y-auto px-2">
        {saved.map((c) => {
          const isActive = active?.id === c.id;
          return (
            <button
              key={c.id}
              onMouseEnter={() => setHover(c.id)}
              onMouseLeave={() => setHover((v) => (v === c.id ? null : v))}
              className={`relative flex h-10 w-10 flex-col items-center justify-center rounded-md border text-[10px] font-semibold transition-colors ${
                isActive
                  ? "border-primary bg-primary/10 text-primary"
                  : "border-border bg-bg text-text hover:bg-surface-2"
              }`}
              title={`${c.name} · ${c.engine}`}
            >
              {c.color && (
                <span
                  className="absolute -right-0.5 -top-0.5 inline-block h-2 w-2 rounded-full ring-1 ring-bg"
                  style={{ background: c.color }}
                />
              )}
              {initials(c)}
              <span className="text-[8px] font-normal text-muted">{c.engine.slice(0, 4)}</span>
              {hover === c.id && (
                <span className="pointer-events-none absolute left-full top-1/2 z-50 ml-2 -translate-y-1/2 whitespace-nowrap rounded-md border border-border bg-bg px-2 py-1 text-[11px] text-text shadow-[var(--shadow-soft)]">
                  {c.name}
                  <span className="ml-1 text-muted">· {c.engine}</span>
                </span>
              )}
            </button>
          );
        })}
        {saved.length === 0 && (
          <div className="px-1 text-center text-[10px] text-muted">
            No saved connections yet.
          </div>
        )}
        <button
          onClick={onAdd}
          className="mt-1 flex h-10 w-10 items-center justify-center rounded-md border border-dashed border-border text-muted hover:border-primary hover:text-primary"
          title="Add connection (Disconnect to access)"
        >
          <IconPlus />
        </button>
      </div>
      <button
        onClick={onCollapse}
        className="mt-2 flex h-6 w-6 items-center justify-center rounded text-muted hover:bg-surface-2 hover:text-text"
        title="Collapse rail"
      >
        <IconChevronLeft size={12} />
      </button>
    </aside>
  );
}
