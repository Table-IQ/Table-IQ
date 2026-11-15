import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";

const ROWS = [
  { label: "Source license", tableiq: "AGPL-3.0-only for application code", tableplus: "Proprietary", dbeaver: "Apache-2.0 Community; separate commercial editions" },
  { label: "Free desktop use", tableiq: "SQL/table tabs and connections free; no tab, engine or device-count paywall", tableplus: "Free trial: 2 tabs, 2 windows, 2 advanced filters; paid perpetual license available", dbeaver: "Free Community Edition; paid PRO editions" },
  { label: "Everyday data work", tableiq: "Browse tables, run SQL and review staged grid edits", tableplus: "SQL and data editors; preview changes as SQL before committing", dbeaver: "SQL and data editors; manual transactions and rollback" },
  { label: "Optional AI", tableiq: "Configure Anthropic, OpenAI, Gemini, OpenRouter or local Ollama; preview compatibility varies", tableplus: "Built-in assistant with API-key providers and local Ollama; check vendor plan terms", dbeaver: "Community has AI Chat with OpenAI/custom endpoints; additional providers and tools in PRO" },
  { label: "Release scope", tableiq: "Apple silicon Mac developer preview; unsigned and not notarized", tableplus: "Established releases across multiple platforms", dbeaver: "Established releases across multiple platforms" },
];

export function Comparison() {
  return (
    <SectionShell id="compare" surface="paper-2" label="compare-title">
      <SectionHeading id="compare-title" title="Considering TablePlus or DBeaver?">
        Keep more SQL and table tabs open without buying a license. Compare your everyday browsing, query and edit workflow beside TablePlus or DBeaver. DBeaver Community is also free and open source; both established clients already offer AI features.
      </SectionHeading>
      <p className="mb-3 text-sm text-muted lg:hidden">Swipe or scroll horizontally to compare all three clients.</p>
      <div className="overflow-x-auto rounded-card border border-rule" role="region" aria-label="Database client comparison" tabIndex={0}>
        <table className="w-full min-w-[48rem] border-collapse text-left text-sm">
          <caption className="sr-only">A factual comparison of TableIQ, TablePlus and DBeaver, checked November 15, 2026</caption>
          <thead className="bg-paper-3">
            <tr>
              <th scope="col" className="w-1/5 p-4 font-medium">What matters</th>
              <th scope="col" className="p-4 font-medium">TableIQ</th>
              <th scope="col" className="p-4 font-medium">TablePlus</th>
              <th scope="col" className="p-4 font-medium">DBeaver</th>
            </tr>
          </thead>
          <tbody>
            {ROWS.map((row) => (
              <tr key={row.label} className="border-t border-rule">
                <th scope="row" className="p-4 align-top font-medium">{row.label}</th>
                <td className="p-4 align-top text-ink-2">{row.tableiq}</td>
                <td className="p-4 align-top text-ink-2">{row.tableplus}</td>
                <td className="p-4 align-top text-ink-2">{row.dbeaver}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="mt-4 max-w-4xl text-sm text-muted">
        Checked November 15, 2026 using vendor documentation. This is not a performance benchmark or a feature parity claim. Provider API usage can cost money in any client. TableIQ uses manual updates and does not preserve a server session or open transaction between separate query runs. Compare your required engine and workflow before switching.
      </p>
      <p className="mt-3 text-sm text-muted">
        Sources: <a href="/license" className="underline decoration-rule underline-offset-4 hover:text-ink">TableIQ license</a>{" · "}
        <a href="https://tableplus.com/pricing" className="underline decoration-rule underline-offset-4 hover:text-ink">TablePlus pricing</a>{" · "}
        <a href="https://tableplus.com/docs/utilities/shortcut-keys" className="underline decoration-rule underline-offset-4 hover:text-ink">TablePlus SQL preview</a>{" · "}
        <a href="https://tableplus.com/docs/llm-plugin" className="underline decoration-rule underline-offset-4 hover:text-ink">TablePlus AI</a>{" · "}
        <a href="https://github.com/dbeaver/dbeaver" className="underline decoration-rule underline-offset-4 hover:text-ink">DBeaver Community</a>{" · "}
        <a href="https://dbeaver.com/docs/dbeaver/AI-Smart-Assistance/" className="underline decoration-rule underline-offset-4 hover:text-ink">DBeaver AI</a>{" · "}
        <a href="https://dbeaver.com/docs/dbeaver/Auto-and-Manual-Commit-Modes/" className="underline decoration-rule underline-offset-4 hover:text-ink">DBeaver transactions</a>.
      </p>
    </SectionShell>
  );
}
