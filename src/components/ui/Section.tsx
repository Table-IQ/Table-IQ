import { useState, type ReactNode } from "react";
import { IconChevronDown, IconChevronRight } from "./Icon";

export function Section({
  label,
  count,
  defaultOpen = true,
  right,
  children,
}: {
  label: string;
  count?: number;
  defaultOpen?: boolean;
  right?: ReactNode;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="mb-1">
      <div className="flex h-6 items-center gap-1 px-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted">
        <button
          onClick={() => setOpen((v) => !v)}
          className="inline-flex h-4 w-4 items-center justify-center rounded text-muted hover:bg-surface-2"
          aria-label={open ? "Collapse" : "Expand"}
        >
          {open ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
        </button>
        <span className="flex-1">{label}</span>
        {typeof count === "number" && <span className="text-muted/70">{count}</span>}
        {right}
      </div>
      {open && <div>{children}</div>}
    </div>
  );
}
