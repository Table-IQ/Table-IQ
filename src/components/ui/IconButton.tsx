import type { ButtonHTMLAttributes, ReactNode } from "react";

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
  const dim = size === "sm" ? "h-5 w-5" : "h-6 w-6";
  const tone =
    variant === "solid"
      ? "bg-primary text-white hover:bg-primary-hover"
      : variant === "danger"
      ? "text-danger hover:bg-surface-2"
      : active
      ? "bg-surface-2 text-text"
      : "text-muted hover:bg-surface-2 hover:text-text";
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
