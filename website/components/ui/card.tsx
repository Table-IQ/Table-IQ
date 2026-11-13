import Link from "next/link";
import type { ReactNode } from "react";

export function LinkCard({ href, title, children }: { href: string; title: string; children: ReactNode }) {
  return (
    <Link href={href} className="group block rounded-card border border-rule bg-paper p-8 transition hover:-translate-y-1 hover:border-accent motion-reduce:hover:translate-y-0">
      <h3 className="text-title font-medium">{title}</h3>
      <p className="mt-3 text-ink-2">{children}</p>
      <span aria-hidden="true" className="mt-6 inline-block text-accent transition-transform group-hover:translate-x-1">→</span>
    </Link>
  );
}
