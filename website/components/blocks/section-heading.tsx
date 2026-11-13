import type { ReactNode } from "react";
import { Reveal } from "@/components/motion/reveal";

export function SectionHeading({ id, title, children, wide = false }: { id: string; title: ReactNode; children?: ReactNode; wide?: boolean }) {
  return (
    <Reveal className={`mb-12 grid gap-6 md:mb-16 ${wide ? "" : "lg:grid-cols-2 lg:items-end lg:gap-8"}`}>
      <h2 id={id} className="text-section font-medium text-balance">{title}</h2>
      {children ? <p className="max-w-xl text-lead text-ink-2">{children}</p> : null}
    </Reveal>
  );
}
