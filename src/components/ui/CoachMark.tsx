import { useEffect, useState, type ReactNode } from "react";

export interface CoachMarkSpec {
  target: string; // CSS selector
  title: string;
  body: ReactNode;
  placement?: "top" | "right" | "bottom" | "left";
}

export function CoachMark({
  spec,
  index,
  total,
  onNext,
  onSkip,
}: {
  spec: CoachMarkSpec;
  index: number;
  total: number;
  onNext: () => void;
  onSkip: () => void;
}) {
  const [rect, setRect] = useState<DOMRect | null>(null);

  useEffect(() => {
    const compute = () => {
      const el = document.querySelector(spec.target) as HTMLElement | null;
      if (!el) {
        setRect(null);
        return;
      }
      setRect(el.getBoundingClientRect());
    };
    compute();
    const id = window.setInterval(compute, 250);
    window.addEventListener("resize", compute);
    return () => {
      window.clearInterval(id);
      window.removeEventListener("resize", compute);
    };
  }, [spec.target]);

  if (!rect) {
    // Target not yet mounted — float the bubble in the bottom-right.
    return (
      <div className="tiq-overlay">
        <Bubble {...{ spec, index, total, onNext, onSkip }} style={{ right: 24, bottom: 24 }} />
      </div>
    );
  }

  const placement = spec.placement ?? "right";
  const margin = 12;
  const pos: React.CSSProperties = (() => {
    switch (placement) {
      case "top":
        return { left: rect.left, top: rect.top - margin, transform: "translateY(-100%)" };
      case "bottom":
        return { left: rect.left, top: rect.bottom + margin };
      case "left":
        return { left: rect.left - margin, top: rect.top, transform: "translateX(-100%)" };
      case "right":
      default:
        return { left: rect.right + margin, top: rect.top };
    }
  })();

  return (
    <>
      {/* Spotlight halo */}
      <div className="pointer-events-none fixed inset-0 z-[60]">
        <div
          className="absolute rounded-md ring-4 ring-primary/40"
          style={{
            left: rect.left - 6,
            top: rect.top - 6,
            width: rect.width + 12,
            height: rect.height + 12,
            boxShadow: "0 0 0 9999px rgb(0 0 0 / 0.35)",
          }}
        />
      </div>
      <Bubble {...{ spec, index, total, onNext, onSkip }} style={{ position: "fixed", zIndex: 70, ...pos }} />
    </>
  );
}

function Bubble({
  spec,
  index,
  total,
  onNext,
  onSkip,
  style,
}: {
  spec: CoachMarkSpec;
  index: number;
  total: number;
  onNext: () => void;
  onSkip: () => void;
  style?: React.CSSProperties;
}) {
  return (
    <div
      className="tiq-glass-panel z-[70] w-72 rounded-lg border border-border bg-bg p-3 shadow-[var(--shadow-pop)]"
      style={style}
    >
      <div className="mb-1 text-xs font-semibold text-text">{spec.title}</div>
      <div className="mb-3 text-xs text-muted">{spec.body}</div>
      <div className="flex items-center justify-between">
        <span className="text-[11px] text-muted">
          {index + 1} / {total}
        </span>
        <div className="flex items-center gap-1">
          <button
            onClick={onSkip}
            className="rounded px-2 py-1 text-xs text-muted hover:bg-surface-2 hover:text-text"
          >
            Skip tour
          </button>
          <button
            onClick={onNext}
            className="rounded-md bg-primary px-3 py-1 text-xs text-white hover:bg-primary-hover"
          >
            {index + 1 === total ? "Done" : "Next"}
          </button>
        </div>
      </div>
    </div>
  );
}
