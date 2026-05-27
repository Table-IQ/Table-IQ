import type { ReactNode } from "react";

export function Chip({
  children,
  tone = "default",
  onClick,
  onClose,
  title,
}: {
  children: ReactNode;
  tone?: "default" | "primary" | "danger" | "warn" | "ok" | "muted";
  onClick?: () => void;
  onClose?: () => void;
  title?: string;
}) {
  const map = {
    default: "bg-surface-2 text-text border-border",
    primary: "bg-primary/10 text-primary border-primary/20",
    danger: "bg-danger/10 text-danger border-danger/20",
    warn: "bg-warn/10 text-warn border-warn/20",
    ok: "bg-ok/10 text-ok border-ok/20",
    muted: "bg-surface text-muted border-border",
  } as const;
  const Comp = onClick ? "button" : "span";
  return (
    <Comp
      onClick={onClick}
      title={title}
      className={`inline-flex h-5 items-center gap-1 rounded-full border px-2 text-[11px] leading-none transition-colors ${map[tone]} ${onClick ? "hover:bg-surface-2" : ""}`}
    >
      {children}
      {onClose && (
        <span
          role="button"
          onClick={(e) => {
            e.stopPropagation();
            onClose();
          }}
          className="ml-0.5 inline-flex h-3 w-3 items-center justify-center rounded-full text-muted hover:bg-surface-2 hover:text-text"
          aria-label="Remove"
        >
          ×
        </span>
      )}
    </Comp>
  );
}

export function ColorDot({ color, size = 10 }: { color: string; size?: number }) {
  return (
    <span
      aria-hidden
      className="inline-block shrink-0 rounded-full"
      style={{ width: size, height: size, background: color }}
    />
  );
}
