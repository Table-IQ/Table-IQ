import { useEffect, useRef } from "react";
import { canLeaveSession, useConnections } from "../state/connectionStore";
import { useTabs, type Tab } from "../state/tabsStore";
import {
  closeTabIn,
  focusTabIn,
  hydrateForConnection,
  persistForConnection,
  type PersistedTabs,
} from "../lib/tabsPersist";
import { IconClose, IconTable } from "./ui/Icon";
import { ColorDot } from "./ui/Chip";

interface Card {
  connId: string;
  connName: string;
  connColor: string | null;
  tab: Tab;
}

const EMPTY_SLICE: PersistedTabs = { tabs: [], activeKey: null, queryCount: 0 };

/**
 * ⌘⇧\ — TablePlus-style Workspace Overview: every open tab across every open
 * connection, in one grid. Click focuses it (switching connection first if
 * it belongs to a different one); × closes it in place.
 *
 * The active connection's tabs live in `useTabs` (source of truth until the
 * next persist); every other open connection's live only in its persisted
 * slice (`tabsPersist.ts`) — hydrated here, and rewritten directly on click/×
 * since there's no mounted shell for that connection to do it.
 */
export function WorkspaceOverview({ onClose }: { onClose: () => void }) {
  const { open, active, saved } = useConnections();
  const liveTabs = useTabs((s) => s.tabs);
  const containerRef = useRef<HTMLDivElement>(null);
  const cardRefs = useRef<(HTMLButtonElement | null)[]>([]);

  useEffect(() => {
    containerRef.current?.focus();
  }, []);

  // Every OTHER open session's persisted slice — read once per render so the
  // cards below and the click handlers agree on the same snapshot.
  const otherSlices = new Map<string, PersistedTabs>();
  for (const o of open) {
    if (o.info.id === active?.id) continue;
    otherSlices.set(o.info.id, hydrateForConnection(o.info.id, null, o.schema) ?? EMPTY_SLICE);
  }

  const cards: Card[] = [];
  for (const o of open) {
    const connColor = saved.find((c) => c.id === o.info.id)?.color ?? null;
    const tabs = o.info.id === active?.id ? liveTabs : otherSlices.get(o.info.id)!.tabs;
    for (const tab of tabs) cards.push({ connId: o.info.id, connName: o.info.name, connColor, tab });
  }

  const focusCard = async (card: Card) => {
    if (card.connId === active?.id) {
      useTabs.getState().setActive(card.tab.key);
    } else {
      // Mirrors ConnectionRail's switchTo guard: jumping away from the active
      // connection must not silently discard its staged grid edits.
      if (!(await canLeaveSession())) return;
      const slice = otherSlices.get(card.connId) ?? EMPTY_SLICE;
      persistForConnection(card.connId, focusTabIn(slice, card.tab.key), null);
      useConnections.getState().switchTo(card.connId);
    }
    onClose();
  };

  const closeCard = (card: Card) => {
    if (card.connId === active?.id) {
      useTabs.getState().close(card.tab.key);
    } else {
      const slice = otherSlices.get(card.connId) ?? EMPTY_SLICE;
      persistForConnection(card.connId, closeTabIn(slice, card.tab.key), null);
    }
  };

  return (
    <div
      className="tiq-overlay flex items-start justify-center overflow-auto p-6 pt-[10vh]"
      onClick={onClose}
    >
      <div
        ref={containerRef}
        tabIndex={-1}
        role="dialog"
        aria-label="Workspace Overview"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            onClose();
            return;
          }
          const forward = e.key === "ArrowRight" || e.key === "ArrowDown";
          const backward = e.key === "ArrowLeft" || e.key === "ArrowUp";
          if (!forward && !backward) return;
          e.preventDefault();
          const buttons = cardRefs.current.filter((b): b is HTMLButtonElement => !!b);
          if (buttons.length === 0) return;
          const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
          if (i < 0) {
            buttons[0]?.focus();
            return;
          }
          const next = forward ? Math.min(i + 1, buttons.length - 1) : Math.max(i - 1, 0);
          buttons[next]?.focus();
        }}
        className="flex max-h-[80vh] w-full max-w-3xl flex-col overflow-hidden rounded-[10px] border border-border bg-bg shadow-[var(--shadow-pop)] outline-none"
      >
        <div className="border-b border-border bg-surface px-4 py-2.5 text-center">
          <div className="text-[13px] font-semibold text-text">Workspace Overview</div>
        </div>
        <div className="min-h-0 flex-1 overflow-auto p-4">
          {cards.length === 0 ? (
            <div className="px-4 py-10 text-center text-[11px] text-muted">No open tabs.</div>
          ) : (
            <div className="grid grid-cols-3 gap-3">
              {cards.map((card, i) => (
                <div key={`${card.connId}:${card.tab.key}`} className="group relative">
                  <button
                    ref={(el) => {
                      cardRefs.current[i] = el;
                    }}
                    onClick={() => void focusCard(card)}
                    className="flex w-full flex-col gap-1.5 rounded-[var(--radius)] border border-border bg-surface p-3 text-left transition-colors hover:bg-surface-2"
                  >
                    <div className="flex items-center gap-1.5 text-[12px] text-text">
                      {card.tab.kind === "query" && <span className="text-primary">◇ </span>}
                      {card.tab.kind === "erd" && <span className="text-accent">◫ </span>}
                      {card.tab.kind === "table" && <IconTable size={12} className="shrink-0 text-muted" />}
                      <span className="truncate font-mono">{card.tab.title}</span>
                    </div>
                    <div className="flex items-center gap-1.5 text-[10.5px] text-muted">
                      {card.connColor && <ColorDot color={card.connColor} size={7} />}
                      <span className="truncate">{card.connName}</span>
                    </div>
                  </button>
                  <button
                    onClick={() => closeCard(card)}
                    aria-label={`Close ${card.tab.title}`}
                    className="absolute right-1 top-1 hidden h-4 w-4 items-center justify-center rounded text-muted hover:bg-surface-2 hover:text-text group-focus-within:flex group-hover:flex"
                  >
                    <IconClose size={9} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
        <footer className="flex items-center justify-between border-t border-border bg-surface px-4 py-1.5 text-[10px] text-muted">
          <span>
            {cards.length} tab{cards.length === 1 ? "" : "s"} across {open.length} connection
            {open.length === 1 ? "" : "s"}
          </span>
          <span className="tiq-kbd">ESC</span>
        </footer>
      </div>
    </div>
  );
}
