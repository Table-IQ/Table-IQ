import type { ReactNode } from "react";

/** TablePlus's toolbar idiom: a 36px glass pill grouping related titlebar actions. */
export function Capsule({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={`tiq-no-drag flex h-9 min-w-0 shrink-0 items-center rounded-full border px-0.5 ${className}`}
      style={{
        background: "var(--glass-bg)",
        borderColor: "var(--glass-border)",
        boxShadow: "var(--glass-shadow)",
        // ponytail: no backdrop-filter — the titlebar behind is solid, so a blur
        // would cost GPU for nothing; add it with real window vibrancy.
      }}
    >
      {children}
    </div>
  );
}

export function CapsuleDivider() {
  return <span aria-hidden className="mx-0.5 h-4 w-px bg-border" />;
}

export function CapsuleButton({
  icon,
  label,
  shortcut,
  title,
  active,
  disabled,
  onClick,
  className = "",
}: {
  icon: ReactNode;
  label: string;
  shortcut?: string;
  title?: string;
  active?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  className?: string;
}) {
  const tip = title ?? (shortcut ? `${label} (${shortcut})` : label);
  const button = (
    <button
      type="button"
      aria-label={label}
      aria-pressed={active === undefined ? undefined : active}
      title={disabled ? undefined : tip}
      disabled={disabled}
      onClick={onClick}
      className={`inline-flex h-8 min-w-9 items-center justify-center rounded-full px-1.5 text-text/80 transition-colors duration-100 hover:bg-text/8 active:bg-text/14 disabled:pointer-events-none disabled:opacity-35 ${
        active ? "bg-text/12 text-text" : ""
      } ${className}`}
    >
      {icon}
    </button>
  );
  // Disabled buttons get no pointer events, so the wrapper carries the tooltip.
  return disabled ? <span title={tip} className={`inline-flex ${className}`}>{button}</span> : button;
}
