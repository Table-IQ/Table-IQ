import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";

const ROWS = [
  { label: "Source license", tableiq: "AGPL-3.0-only for application code", tableplus: "Proprietary", dbeaver: "Apache-2.0 Community; separate commercial editions" },
  { label: "Free desktop use", tableiq: "Current app free, with no engine or device limit", tableplus: "Limited free use; paid perpetual license available", dbeaver: "Free Community Edition; paid PRO editions" },
  { label: "Desktop approach", tableiq: "Rust core with the system webview", tableplus: "Native desktop client", dbeaver: "Java and Eclipse, with JDBC drivers" },
  { label: "Agent integration", tableiq: "Local MCP server; chosen connections; writes off by default", tableplus: "Built-in Mac MCP server with token scopes and query approval", dbeaver: "PRO AI tools can connect to external MCP servers; availability varies by edition" },
  { label: "Release scope", tableiq: "Apple silicon Mac developer preview; unsigned and not notarized", tableplus: "Established releases across multiple platforms", dbeaver: "Established releases across multiple platforms" },
];

export function Comparison() {
  return (
    <SectionShell id="compare" surface="paper-2" label="compare-title">
      <SectionHeading id="compare-title" title="Considering TablePlus or DBeaver?">
        TableIQ is another free, open-source choice for database work. Try one task beside your current client and see whether it fits. DBeaver Community is also open source; all three have different strengths and tradeoffs.
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
        Checked November 15, 2026 using vendor documentation. This is not a performance benchmark or a feature parity claim. TableIQ checks for updates but uses manual installation; it has no persistent per-tab server sessions. MCP server access and connecting an AI assistant to external MCP tools are different workflows.
      </p>
      <p className="mt-3 text-sm text-muted">
        Sources: <a href="/license" className="underline decoration-rule underline-offset-4 hover:text-ink">TableIQ license</a>{" · "}
        <a href="https://tableplus.com/pricing" className="underline decoration-rule underline-offset-4 hover:text-ink">TablePlus pricing</a>{" · "}
        <a href="https://tableplus.com/docs/preferences/mcp" className="underline decoration-rule underline-offset-4 hover:text-ink">TablePlus MCP</a>{" · "}
        <a href="https://github.com/dbeaver/dbeaver" className="underline decoration-rule underline-offset-4 hover:text-ink">DBeaver Community</a>{" · "}
        <a href="https://dbeaver.com/docs/dbeaver/AI-Tools-and-MCP/" className="underline decoration-rule underline-offset-4 hover:text-ink">DBeaver AI and MCP</a>.
      </p>
    </SectionShell>
  );
}
