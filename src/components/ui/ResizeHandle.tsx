import { useRef } from "react";

/**
 * 4-px wide drag handle for resizing flex-column siblings (left sidebar / right
 * inspector in MainShell). The visible part is 1px hairline of border with a
 * 3px-wide invisible hitbox on the side opposite the resized panel — wider grab
 * target without stealing visual space. Hover/drag glow uses primary so the
 * affordance is unambiguous (TablePlus parity).
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
}: {
  onResize: (delta: number) => void;
  direction?: "horizontal" | "vertical";
  /** Which side of the visible hairline holds the wider grab hitbox. */
  side?: "left" | "right" | "top" | "bottom";
  className?: string;
}) {
  const start = useRef<number | null>(null);

  const onDown = (e: React.MouseEvent) => {
    e.preventDefault();
    start.current = direction === "horizontal" ? e.clientX : e.clientY;
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
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  const isHoriz = direction === "horizontal";
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
      aria-orientation={isHoriz ? "vertical" : "horizontal"}
      onMouseDown={onDown}
      onDoubleClick={() => onResize(0)} // hook for parents that want to snap to default
      className={`group relative shrink-0 ${
        isHoriz ? `w-px cursor-col-resize ${overshoot}` : `h-px cursor-row-resize ${overshoot}`
      } bg-border transition-colors hover:bg-primary/40 active:bg-primary ${className}`}
    >
      <span
        aria-hidden
        className={`pointer-events-none absolute ${
          isHoriz
            ? "inset-y-0 left-1/2 -translate-x-1/2 w-px"
            : "inset-x-0 top-1/2 -translate-y-1/2 h-px"
        } bg-transparent group-hover:bg-primary group-active:bg-primary`}
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
