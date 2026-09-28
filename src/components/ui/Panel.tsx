import { useId, useState, type ReactNode } from "react";
import { IconClose } from "./Icon";

export function Panel({
  title,
  onClose,
  footer,
  toolbar,
  width = "lg",
  children,
}: {
  title: ReactNode;
  onClose?: () => void;
  toolbar?: ReactNode;
  footer?: ReactNode;
  width?: "sm" | "md" | "lg" | "xl" | "full";
  children: ReactNode;
}) {
  const w =
    width === "sm"
      ? "max-w-md"
      : width === "md"
      ? "max-w-2xl"
      : width === "lg"
      ? "max-w-4xl"
      : width === "xl"
      ? "max-w-6xl"
      : "max-w-[96vw]";
  // Close plays the exit animation first; the parent unmounts us when it ends.
  // (Under reduced motion it still ends, after 0.01ms — see index.css.)
  const [closing, setClosing] = useState(false);
  const titleId = useId();
  return (
    <div
      className={`tiq-overlay flex items-center justify-center p-6 ${closing ? "tiq-pop-out" : "tiq-pop-in"}`}
      onAnimationEnd={(e) => {
        if (closing && e.target === e.currentTarget) onClose?.();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className={`flex max-h-[92vh] w-full flex-col overflow-hidden rounded-lg border border-border bg-bg shadow-[var(--shadow-pop)] ${w}`}
      >
        <header className="flex h-10 shrink-0 items-center gap-2 border-b border-border bg-surface px-3">
          <div id={titleId} className="flex-1 truncate text-sm font-semibold text-text">{title}</div>
          {onClose && (
            <button
              onClick={() => setClosing(true)}
              disabled={closing}
              aria-label="Close"
              className="inline-flex h-6 w-6 items-center justify-center rounded text-muted hover:bg-surface-2 hover:text-text"
            >
              <IconClose />
            </button>
          )}
        </header>
        {toolbar && <div className="shrink-0">{toolbar}</div>}
        <div className="min-h-0 flex-1 overflow-auto">{children}</div>
        {footer && (
          <footer className="flex shrink-0 items-center gap-2 border-t border-border bg-surface px-3 py-2">
            {footer}
          </footer>
        )}
      </div>
    </div>
  );
}

export function Tabs({
  tabs,
  active,
  onSelect,
}: {
  tabs: { key: string; label: ReactNode; badge?: ReactNode }[];
  active: string;
  onSelect: (k: string) => void;
}) {
  return (
    <div className="flex items-center gap-0 border-b border-border bg-surface px-2">
      {tabs.map((t) => {
        const isActive = t.key === active;
        return (
          <button
            key={t.key}
            onClick={() => onSelect(t.key)}
            className={`relative inline-flex h-8 items-center gap-1.5 px-3 text-xs transition-colors ${
              isActive ? "text-text" : "text-muted hover:text-text"
            }`}
          >
            {t.label}
            {t.badge}
            {isActive && (
              <span className="absolute inset-x-2 -bottom-px h-0.5 rounded-full bg-primary" />
            )}
          </button>
        );
      })}
    </div>
  );
}
