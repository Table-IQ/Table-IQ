import type { ReactNode } from "react";

export function Badge({ children }: { children: ReactNode }) {
  return <span className="ml-2 rounded-pill border border-rule px-1.5 py-px text-xs text-muted">{children}</span>;
}
