import type { ButtonHTMLAttributes, ReactNode } from "react";

/** A rounded container that groups related toolbar icons into one "card" —
 *  TablePlus's top-bar idiom (Slice 33). Buttons sit flush inside with a faint
 *  surface + border so the group reads as a single unit. */
export function IconGroup({ children }: { children: ReactNode }) {
  return (
    <div className="flex items-center gap-0.5 rounded-lg border border-border bg-bg/50 p-0.5">
      {children}
    </div>
  );
}

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon: ReactNode;
  label: string;
  shortcut?: string;
  active?: boolean;
  variant?: "ghost" | "solid" | "danger";
  size?: "sm" | "md";
}

export function IconButton({
  icon,
  label,
  shortcut,
  active,
  variant = "ghost",
  size = "md",
  className = "",
  ...rest
}: Props) {
  // Larger hit targets + a hover overlay that's clearly visible in BOTH themes
  // (`bg-text/N` = a tint of the foreground, so it reads on light and dark).
  const dim = size === "sm" ? "h-6 w-6" : "h-7 w-7";
  const tone =
    variant === "solid"
      ? "bg-primary text-white hover:bg-primary-hover"
      : variant === "danger"
        ? "text-danger hover:bg-danger/10"
        : active
          ? "bg-text/12 text-text"
          : "text-muted hover:bg-text/10 hover:text-text";
  const title = shortcut ? `${label} (${shortcut})` : label;
  return (
    <button
      aria-label={label}
      title={title}
      className={`inline-flex ${dim} items-center justify-center rounded-md transition-colors ${tone} ${className}`}
      {...rest}
    >
      {icon}
    </button>
  );
}
