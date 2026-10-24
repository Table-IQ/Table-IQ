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
    <div className="@container/status shrink-0">
      <div
        className="tiq-chrome flex flex-wrap items-center gap-x-2 gap-y-1 border-t border-border px-3 py-1 text-[11px] text-muted"
        style={{ minHeight: "var(--statusbar-h)" }}
      >
        <span className="flex max-w-full flex-wrap items-center gap-2">{left}</span>
        <span className="order-last flex max-w-full basis-full flex-wrap items-center justify-center gap-2 empty:hidden @min-[760px]/status:order-none @min-[760px]/status:flex-1 @min-[760px]/status:basis-auto">{center}</span>
        <span className="ml-auto flex max-w-full flex-wrap items-center gap-2">{right}</span>
      </div>
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

/** Tight status-bar button used for paging chevrons + `+ Row` and similar. */
export function StatusBarButton({
  children,
  onClick,
  disabled,
  active,
  title,
  className = "",
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  active?: boolean;
  title?: string;
  className?: string;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={`inline-flex h-[18px] items-center gap-1 rounded-[var(--radius-sm)] px-1.5 text-[11px] transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-40 ${
        active ? "bg-surface-2 text-text" : "text-muted hover:bg-surface-2 hover:text-text"
      } ${className}`}
    >
      {children}
    </button>
  );
}

/** Segmented two-up toggle used in the status bar (e.g. Data | Structure). */
export function SegmentedToggle<T extends string>({
  options,
  value,
  onChange,
}: {
  options: { value: T; label: string; icon?: ReactNode }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <span className="inline-flex h-[18px] items-stretch rounded-[var(--radius-sm)] border border-border bg-bg">
      {options.map((o, i) => (
        <button
          key={o.value}
          onClick={() => onChange(o.value)}
          className={`px-2 text-[11px] transition-colors ${
            i > 0 ? "border-l border-border" : ""
          } ${
            value === o.value
              ? "bg-primary-soft font-medium text-primary"
              : "text-muted hover:text-text"
          }`}
        >
          <span className="inline-flex items-center gap-1">
            {o.icon}
            {o.label}
          </span>
        </button>
      ))}
    </span>
  );
}
