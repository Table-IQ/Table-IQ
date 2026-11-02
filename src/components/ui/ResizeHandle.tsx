import { useEffect, useRef } from "react";

/**
 * 4-px wide drag handle for resizing flex-column siblings (left sidebar / right
 * inspector in MainShell). The visible part is 1px hairline of border with a
 * 3px-wide invisible hitbox on the side opposite the resized panel — wider grab
 * target without stealing visual space. Hover/drag glow uses primary so the
 * affordance is unambiguous (TablePlus parity). Vertical resizing uses an 8px
 * strip with a visible grip. Both sit above adjacent panes so their hitbox stays usable.
 *
 * `onResize(delta)` is called with the pixel delta on every mousemove (positive
 * = right / down). The parent owns the width state — handle is presentation +
 * mouse plumbing only.
 */
export function ResizeHandle({
  onResize,
  direction = "horizontal",
  side = "right",
  className = "",
  label = "Resize panels",
  value,
  min,
  max,
  onReset,
}: {
  onResize: (delta: number) => void;
  direction?: "horizontal" | "vertical";
  /** Which side of the visible hairline holds the wider grab hitbox. */
  side?: "left" | "right" | "top" | "bottom";
  className?: string;
  label?: string;
  value?: number;
  min?: number;
  max?: number;
  onReset?: () => void;
}) {
  const start = useRef<number | null>(null);
  const cleanup = useRef<(() => void) | null>(null);
  useEffect(() => () => cleanup.current?.(), []);

  const onDown = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    cleanup.current?.();
    e.currentTarget.focus();
    start.current = direction === "horizontal" ? e.clientX : e.clientY;
    const previousSelect = document.body.style.userSelect;
    const previousCursor = document.body.style.cursor;
    document.body.style.userSelect = "none";
    document.body.style.cursor = direction === "horizontal" ? "col-resize" : "row-resize";
    const onMove = (ev: MouseEvent) => {
      if (start.current === null) return;
      const now = direction === "horizontal" ? ev.clientX : ev.clientY;
      const delta = now - start.current;
      start.current = now;
      if (delta !== 0) onResize(delta);
    };
    const onUp = () => {
      start.current = null;
      document.body.style.userSelect = previousSelect;
      document.body.style.cursor = previousCursor;
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      window.removeEventListener("blur", onUp);
      cleanup.current = null;
    };
    cleanup.current = onUp;
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    window.addEventListener("blur", onUp);
  };

  const isHoriz = direction === "horizontal";
  const sizeDirection = side === "left" || side === "top" ? -1 : 1;
  // 1px hairline visible + 4px transparent overshoot on the chosen side.
  const overshoot =
    side === "left"
      ? "-ml-1 pl-1"
      : side === "right"
      ? "-mr-1 pr-1"
      : side === "top"
      ? "-mt-1 pt-1"
      : "-mb-1 pb-1";
  return (
    <div
      role="separator"
      tabIndex={0}
      aria-label={label}
      aria-orientation={isHoriz ? "vertical" : "horizontal"}
      aria-valuenow={value === undefined ? undefined : Math.round(value)}
      aria-valuemin={min}
      aria-valuemax={max}
      title={onReset ? "Drag to resize · Arrow keys adjust · Double-click to reset" : "Drag to resize · Arrow keys adjust"}
      onMouseDown={onDown}
      onDoubleClick={() => onReset ? onReset() : onResize(0)} // hook for parents that want to snap to default
      onKeyDown={(e) => {
        const step = e.shiftKey ? 40 : 10;
        let delta: number | undefined;
        if (e.key === (isHoriz ? "ArrowLeft" : "ArrowUp")) delta = -step;
        else if (e.key === (isHoriz ? "ArrowRight" : "ArrowDown")) delta = step;
        else if (e.key === "Home" && value !== undefined && min !== undefined) delta = (min - value) * sizeDirection;
        else if (e.key === "End" && value !== undefined && max !== undefined) delta = (max - value) * sizeDirection;
        else if (e.key === "Enter" && onReset) { e.preventDefault(); onReset(); return; }
        if (delta !== undefined) { e.preventDefault(); onResize(delta); }
      }}
      className={`group relative z-10 shrink-0 focus-visible:outline-2 focus-visible:outline-primary focus-visible:-outline-offset-2 ${
        isHoriz ? `w-px cursor-col-resize bg-border ${overshoot}` : "h-2 cursor-row-resize bg-surface"
      } transition-colors hover:bg-primary/20 active:bg-primary/30 ${className}`}
    >
      <span
        aria-hidden
        className={`pointer-events-none absolute ${
          isHoriz
            ? "inset-y-0 left-1/2 -translate-x-1/2 w-px"
            : "left-1/2 top-1/2 h-0.5 w-8 -translate-x-1/2 -translate-y-1/2 rounded-full"
        } ${isHoriz ? "bg-transparent" : "bg-border-strong"} group-hover:bg-primary group-active:bg-primary`}
      />
    </div>
  );
}

/**
 * Clamp a width to [min, max] — used by panels that don't want users to be
 * able to drag the sidebar smaller than its useful minimum, or wider than the
 * window can accommodate. Pure so the unit test exercises it directly.
 */
export function clampWidth(next: number, min: number, max: number): number {
  if (Number.isNaN(next)) return min;
  return Math.max(min, Math.min(max, next));
}
