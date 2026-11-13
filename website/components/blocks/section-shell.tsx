import type { ReactNode } from "react";

const surfaces = {
  paper: "bg-paper text-ink",
  "paper-2": "bg-paper-2 text-ink",
  "paper-3": "bg-paper-3 text-ink",
  band: "bg-band text-band-ink",
};

// Owns width, vertical rhythm and surface so sections never copy spacing.
export function SectionShell({ id, surface = "paper", children, tight = false, label }: { id?: string; surface?: keyof typeof surfaces; children: ReactNode; tight?: boolean; label?: string }) {
  return (
    <section id={id} aria-labelledby={label} className={`${surfaces[surface]} scroll-mt-16`}>
      <div className={`mx-auto max-w-site px-gutter ${tight ? "py-16 md:py-24" : "py-20 md:py-32"}`}>{children}</div>
    </section>
  );
}
