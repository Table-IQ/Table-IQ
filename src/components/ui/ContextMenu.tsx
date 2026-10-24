import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

export interface ContextMenuItem {
  /** Stable id for keying; also used as the test selector. */
  id: string;
  label: string;
  /** Optional accent / muted styling. */
  tone?: "default" | "danger" | "muted";
  /** Right-aligned secondary text (e.g. a shortcut). */
  shortcut?: string;
  disabled?: boolean;
  /** Set on radio-style items: renders a fixed check slot + aria-checked. */
  checked?: boolean;
  /** Leaf action. Omitted on items that only open a submenu. */
  onSelect?: () => void;
  /** Nested submenu — opens as a flyout on hover/click (TablePlus parity). */
  children?: (ContextMenuItem | "separator")[];
}

export interface ContextMenuProps {
  x: number;
  y: number;
  items: (ContextMenuItem | "separator")[];
  onClose: () => void;
}

function menuButtons(menu: HTMLElement) {
  return Array.from(menu.querySelectorAll<HTMLButtonElement>("button[data-menu-id]:not(:disabled)"))
    .filter((button) => button.closest('[role="menu"]') === menu);
}

/** Recursive item list — owns its open-submenu state so nesting composes. */
function MenuItems({
  items,
  onClose,
  flipLeft,
  autoFocus,
  onBack,
}: {
  items: (ContextMenuItem | "separator")[];
  onClose: () => void;
  flipLeft: boolean;
  autoFocus: boolean;
  onBack?: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [openSub, setOpenSub] = useState<{ id: string; keyboard: boolean } | null>(null);
  useEffect(() => {
    if (autoFocus && ref.current) menuButtons(ref.current)[0]?.focus();
  }, [autoFocus]);
  return (
    <div
      ref={ref}
      role="menu"
      onKeyDown={(e) => {
        // Portals still bubble through the owning dialog in React. Keep menu
        // navigation local so the dialog's Tab trap cannot steal focus.
        e.stopPropagation();
        if (e.key === "Escape" || e.key === "Tab" || (e.key === "ArrowLeft" && onBack)) {
          e.preventDefault();
          if (e.key !== "Tab" && onBack) onBack();
          else onClose();
          return;
        }
        if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
        e.preventDefault();
        const buttons = menuButtons(e.currentTarget);
        const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
        const next = e.key === "Home" ? 0 : e.key === "End" ? buttons.length - 1
          : (current + (e.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
        buttons[next]?.focus();
      }}
    >
      {items.map((item, i) =>
        item === "separator" ? (
          <div key={`sep-${i}`} className="my-1 h-px bg-border" />
        ) : (
          <div
            key={item.id}
            className="relative"
            onMouseEnter={() => setOpenSub(item.children ? { id: item.id, keyboard: false } : null)}
          >
            <button
              disabled={item.disabled}
              tabIndex={-1}
              data-menu-id={item.id}
              onKeyDown={(e) => {
                if (e.key === "ArrowRight" && item.children) {
                  e.preventDefault();
                  e.stopPropagation();
                  setOpenSub({ id: item.id, keyboard: true });
                }
              }}
              onClick={() => {
                if (item.disabled) return;
                if (item.children) {
                  setOpenSub((s) => (s?.id === item.id ? null : { id: item.id, keyboard: true }));
                  return;
                }
                item.onSelect?.();
                onClose();
              }}
              className={`flex w-full items-center justify-between gap-4 px-3 py-1.5 text-left transition-colors hover:bg-bg focus-visible:bg-primary-soft focus-visible:outline focus-visible:outline-primary disabled:cursor-not-allowed disabled:opacity-40 ${
                item.tone === "danger"
                  ? "text-danger"
                  : item.tone === "muted"
                    ? "text-muted"
                    : "text-text"
              }`}
              role={item.checked === undefined ? "menuitem" : "menuitemradio"}
              aria-checked={item.checked}
              aria-haspopup={item.children ? "menu" : undefined}
              aria-expanded={item.children ? openSub?.id === item.id : undefined}
            >
              <span className="truncate">
                {item.checked !== undefined && (
                  <span aria-hidden className="inline-block w-4">
                    {item.checked ? "✓" : ""}
                  </span>
                )}
                {item.label}
              </span>
              {item.children ? (
                <span aria-hidden className="ml-auto text-muted">
                  ›
                </span>
              ) : (
                item.shortcut && (
                  <span className="ml-auto text-[10px] text-muted">{item.shortcut}</span>
                )
              )}
            </button>
            {item.children && openSub?.id === item.id && (
              <div
                className={`tiq-glass-panel absolute top-0 z-50 min-w-[190px] rounded-md border border-border bg-surface py-1 text-xs shadow-xl ${
                  flipLeft ? "right-full -mr-1" : "left-full -ml-1"
                }`}
              >
                <MenuItems
                  items={item.children}
                  onClose={onClose}
                  flipLeft={flipLeft}
                  autoFocus={openSub.keyboard}
                  onBack={() => {
                    setOpenSub(null);
                    if (ref.current) menuButtons(ref.current).find((button) => button.dataset.menuId === item.id)?.focus();
                  }}
                />
              </div>
            )}
          </div>
        ),
      )}
    </div>
  );
}

/**
 * Headless context menu — positions at (x, y), auto-flips when near a viewport edge,
 * closes on outside click / Escape / leaf activation. Items may nest via `children`,
 * which open as flyout submenus (TablePlus parity).
 */
export function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });

  useLayoutEffect(() => {
    const trigger = document.activeElement as HTMLElement | null;
    const menu = ref.current;
    return () => {
      // Do not steal focus from another dialog opened by a menu action.
      if (trigger?.isConnected && (menu?.contains(document.activeElement) || document.activeElement === document.body)) {
        trigger.focus({ preventScroll: true });
      }
    };
  }, []);

  // Outside click + Escape close.
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", onDown, true);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown, true);
      document.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  // Auto-flip near right/bottom viewport edges.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    let nx = x;
    let ny = y;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    if (x + r.width > vw - 4) nx = Math.max(4, vw - r.width - 4);
    if (y + r.height > vh - 4) ny = Math.max(4, vh - r.height - 4);
    if (nx !== pos.x || ny !== pos.y) setPos({ x: nx, y: ny });
    // intentionally only depend on x/y — content height is stable post-mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [x, y]);

  // Submenus flow left when the menu sits in the right portion of the viewport.
  const flipLeft = pos.x > (typeof window !== "undefined" ? window.innerWidth : 1440) * 0.55;

  // Glass ancestors establish containing blocks; viewport coordinates belong
  // at the document root so menus cannot be offset or clipped by a sheet.
  return createPortal(
    <div
      ref={ref}
      data-testid="context-menu"
      className="tiq-glass-panel fixed z-[80] min-w-[190px] rounded-md border border-border bg-surface py-1 text-xs shadow-xl"
      style={{ left: pos.x, top: pos.y }}
      onClick={(e) => e.stopPropagation()}
    >
      <MenuItems items={items} onClose={onClose} flipLeft={flipLeft} autoFocus />
    </div>,
    document.body,
  );
}
