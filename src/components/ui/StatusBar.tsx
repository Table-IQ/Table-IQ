import type { ReactNode } from "react";

export function StatusBar({
  left,
  center,
  right,
}: {
  left?: ReactNode;
  center?: ReactNode;
  right?: ReactNode;
}) {
  return (
    <div
      className="flex items-center gap-2 border-t border-border bg-surface px-3 text-xs text-muted"
      style={{ height: "var(--statusbar-h)" }}
    >
      <span className="flex items-center gap-2">{left}</span>
      <span className="flex flex-1 items-center justify-center gap-2">{center}</span>
      <span className="flex items-center gap-2">{right}</span>
    </div>
  );
}

export function StatusDot({ tone = "ok" }: { tone?: "ok" | "warn" | "danger" | "muted" }) {
  const cls =
    tone === "ok"
      ? "bg-ok"
      : tone === "warn"
      ? "bg-warn"
      : tone === "danger"
      ? "bg-danger"
      : "bg-muted";
  return <span aria-hidden className={`h-1.5 w-1.5 rounded-full ${cls}`} />;
}
