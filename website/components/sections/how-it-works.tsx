import { StepItem } from "@/components/blocks/items";
import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";
import { Reveal } from "@/components/motion/reveal";

const STEPS = [
  { title: "Connect and browse", body: "Open a local SQLite file or add a database connection. Explore tables, inspect columns and filter the rows you need. Start with disposable data in this preview." },
  { title: "Write and run SQL", body: "Use the SQL editor or open an existing .sql file. Run your query, inspect the returned rows and save useful SQL for later." },
  { title: "Review a data edit", body: "Change a value in the table grid, inspect the staged SQL, then commit or discard the edit. Review applies to staged grid changes; SQL you run in the editor executes directly." },
];

export function HowItWorks() {
  return (
    <SectionShell id="how" label="how-title">
      <SectionHeading id="how-title" title="Connect. Query. Review your changes.">
        The everyday workflow, with your database and SQL in view. No AI setup or external agent required.
      </SectionHeading>
      <ol className="grid gap-10 md:grid-cols-3 md:gap-8">
        {STEPS.map((s, i) => (
          <li key={s.title}>
            <Reveal delay={i * 0.08}>
              <StepItem n={i + 1} title={s.title}>{s.body}</StepItem>
            </Reveal>
          </li>
        ))}
      </ol>
      <div className="mt-12 rounded-card border border-rule bg-paper-2 p-6 md:flex md:items-center md:justify-between md:gap-8">
        <p className="max-w-3xl text-ink-2">
          <strong className="text-ink">Start with one familiar task.</strong> Try TableIQ beside your current client. Capabilities vary by engine, and this preview does not preserve a server session or an open transaction between separate query runs.
        </p>
      </div>
    </SectionShell>
  );
}
