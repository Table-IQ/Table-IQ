import type { ReactNode } from "react";

export function Toolbar({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={`tiq-chrome flex h-[var(--toolbar-h)] items-center gap-1 border-b border-border px-2 ${className}`}
      style={{ height: "var(--toolbar-h)" }}
    >
      {children}
    </div>
  );
}

export function ToolbarDivider() {
  return <span aria-hidden className="mx-1 inline-block h-4 w-px bg-border" />;
}

export function ToolbarLabel({ children }: { children: ReactNode }) {
  return <span className="px-1 text-xs text-muted">{children}</span>;
}

export function ToolbarSpacer() {
  return <span className="flex-1" />;
}

export function ToolbarButton({
  icon,
  label,
  shortcut,
  active,
  onClick,
  disabled,
}: {
  icon?: ReactNode;
  label: string;
  shortcut?: string;
  active?: boolean;
  onClick?: () => void;
  disabled?: boolean;
}) {
  const title = shortcut ? `${label} · ${shortcut}` : label;
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={`inline-flex h-6 items-center gap-1 rounded-[var(--radius)] px-1.5 text-[11px] transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-50 ${
        active ? "bg-surface-2 text-text" : "text-muted hover:bg-surface-2 hover:text-text"
      }`}
    >
      {icon}
      <span>{label}</span>
      {shortcut && <span className="tiq-kbd">{shortcut}</span>}
    </button>
  );
}
