import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";
import { PlusIcon } from "@/components/ui/plus-icon";
import { EngineMark } from "@/components/blocks/engine-mark";
import { Badge } from "@/components/ui/badge";
import { ENGINE_GROUPS, ENGINE_STRIP, ENGINE_TOTAL } from "@/lib/engines";

export function Engines() {
  return (
    <SectionShell id="engines" label="engines-title">
      <SectionHeading id="engines-title" title={<>From Postgres to Qdrant,<br />in one window.</>}>
        Connect to PostgreSQL, MySQL, SQLite, MongoDB, Redis and more. Beta profiles are available but not yet verified against the live product.
      </SectionHeading>
      <p className="text-sm text-muted">Connects to {ENGINE_TOTAL} databases and cloud services</p>
      <ul className="mt-5 flex flex-wrap items-center gap-3">
        {ENGINE_STRIP.map((e) => (
          <li key={e.label}><EngineMark engine={e} size="lg" alt={e.label} /></li>
        ))}
        <li><a href="#engine-list" className="text-sm text-ink underline decoration-rule underline-offset-4 hover:decoration-accent">+{ENGINE_TOTAL - ENGINE_STRIP.length} more</a></li>
      </ul>
      <details id="engine-list" className="group mt-10 rounded-card border border-rule">
        <summary className="flex min-h-14 cursor-pointer list-none items-center justify-between px-6 font-medium [&::-webkit-details-marker]:hidden">
          Explore every database and cloud service
          <PlusIcon />
        </summary>
        <div className="grid gap-10 border-t border-rule p-6 md:grid-cols-2 lg:grid-cols-3">
          {ENGINE_GROUPS.map((g) => (
            <div key={g.title}>
              <h3 className="font-semibold">{g.title}</h3>
              <p className="mt-1 text-sm text-muted">{g.note}</p>
              <ul className="mt-4 space-y-2">
                {g.engines.map((e) => (
                  <li key={e.label} className="flex items-center gap-3 text-ink-2">
                    <EngineMark engine={e} />
                    {e.label}
                    {e.beta ? <Badge>Beta</Badge> : null}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </details>
    </SectionShell>
  );
}
