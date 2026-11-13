import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";
import { LinkCard } from "@/components/ui/card";

export function Guides() {
  return (
    <SectionShell id="guides" surface="paper-2" label="guides-title">
      <SectionHeading id="guides-title" title="Investigate a Postgres data issue.">
        Follow the Postgres walkthrough, or learn how connection sharing, tokens and read-only permissions work.
      </SectionHeading>
      <div className="grid gap-6 md:grid-cols-2">
        <LinkCard href="/guides/postgres-investigation" title="Investigate a Postgres data issue with your agent">
          Connect Claude Code, find paid orders that never shipped, and check its SQL in TableIQ.
        </LinkCard>
        <LinkCard href="/guides/mcp-permissions" title="Understand MCP permissions">
          Connection sharing, tokens, refused writes and what differs between engines.
        </LinkCard>
      </div>
    </SectionShell>
  );
}
