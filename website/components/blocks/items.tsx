import type { ReactNode } from "react";

export function StepItem({ n, title, children }: { n: number; title: string; children: ReactNode }) {
  return (
    <div className="relative border-t border-rule pt-6">
      <span className="font-mono text-sm text-signal">0{n}</span>
      <h3 className="mt-3 text-title font-medium">{title}</h3>
      <p className="mt-3 text-ink-2">{children}</p>
    </div>
  );
}

export function FeatureItem({ title, children, large = false }: { title: string; children: ReactNode; large?: boolean }) {
  return (
    <div className={`rounded-card border border-rule bg-paper p-6 ${large ? "md:p-8" : ""}`}>
      <h3 className={`${large ? "text-title" : "text-lg"} font-medium`}>{title}</h3>
      <p className="mt-2 text-ink-2">{children}</p>
    </div>
  );
}

export function StatItem({ value, label }: { value: string; label: string }) {
  return (
    <div className="border-t border-band-muted pt-5">
      <p className="text-section font-medium">{value}</p>
      <p className="mt-2 text-band-muted">{label}</p>
    </div>
  );
}

export function ContrastColumn({ title, items, tone }: { title: string; items: string[]; tone: "before" | "after" }) {
  const mark = tone === "after" ? "text-signal" : "text-error";
  return (
    <div className={`rounded-card border p-8 ${tone === "after" ? "border-accent bg-paper" : "border-rule"}`}>
      <h3 className="text-title font-medium">{title}</h3>
      <ul className="mt-6 space-y-4">
        {items.map((item) => (
          <li key={item} className="flex gap-3 text-ink-2">
            <span aria-hidden="true" className={`font-mono ${mark}`}>{tone === "after" ? "✓" : "×"}</span>
            {item}
          </li>
        ))}
      </ul>
    </div>
  );
}
