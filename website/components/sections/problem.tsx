import { ContrastColumn } from "@/components/blocks/items";
import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";

export function Problem() {
  return (
    <SectionShell surface="paper-2" label="problem-title">
      <SectionHeading id="problem-title" title="Keep agent calls beside your database work.">
        Database and agent logs can exist in different places. TableIQ gives you a local view of calls made through its MCP server, alongside the client you use to inspect the data.
      </SectionHeading>
      <div className="grid gap-6 md:grid-cols-2">
        <ContrastColumn
          tone="before"
          title="Check the access you give an agent"
          items={["The database role determines which data it can reach.", "Read access can still expose data or run expensive queries.", "Your agent and model provider have their own data handling rules."]}
        />
        <ContrastColumn
          tone="after"
          title="A connection shared through TableIQ"
          items={["Only the connections you choose are visible.", "Writes are refused unless you allow them.", "Recent calls show redacted SQL, row counts and outcomes in MCP Activity."]}
        />
      </div>
    </SectionShell>
  );
}
