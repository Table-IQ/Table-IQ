import { ContrastColumn } from "@/components/blocks/items";
import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";

export function Problem() {
  return (
    <SectionShell surface="paper-2" label="problem-title">
      <SectionHeading id="problem-title" title="An agent with a connection string leaves no trail.">
        Pasting credentials into an agent gives it everything the login can do, and you only see what it chooses to tell you.
      </SectionHeading>
      <div className="grid gap-6 md:grid-cols-2">
        <ContrastColumn
          tone="before"
          title="A raw connection string"
          items={["The agent can reach every database the login can.", "Writes are possible whenever the role allows them.", "What ran is whatever the agent's reply says ran."]}
        />
        <ContrastColumn
          tone="after"
          title="A connection shared through TableIQ"
          items={["Only the connections you choose are visible.", "Writes are refused unless you allow them.", "Every call's SQL, rows and outcome sit in MCP Activity."]}
        />
      </div>
    </SectionShell>
  );
}
