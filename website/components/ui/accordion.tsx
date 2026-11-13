import { PlusIcon } from "./plus-icon";
import type { ReactNode } from "react";

// Native disclosure: keyboard and no-JS support for free.
export function AccordionItem({ title, children }: { title: ReactNode; children: ReactNode }) {
  return (
    <details className="group border-b border-rule">
      <summary className="flex min-h-14 cursor-pointer list-none items-center justify-between gap-6 py-4 text-lg font-medium [&::-webkit-details-marker]:hidden">
        {title}
        <PlusIcon />
      </summary>
      <div className="pb-6 text-ink-2">{children}</div>
    </details>
  );
}
