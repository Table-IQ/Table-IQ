import { JsonLd } from "@/components/blocks/json-ld";
import { DocumentSection } from "@/components/sections/document";
import { guideSchema, pageMeta } from "@/lib/meta";

const ROUTE = "/guides/postgres-investigation";
const TITLE = "Investigate a Postgres data issue: TableIQ";
const DESCRIPTION = "Step-by-step: connect Claude Code to TableIQ's local MCP server, investigate unshipped paid orders in Postgres with a read-only role, and check the agent's SQL and a refused write.";

export const metadata = pageMeta(ROUTE, TITLE, DESCRIPTION);

export default function Page() {
  return (
    <>
      <JsonLd data={guideSchema(ROUTE, "Investigate a Postgres data issue with TableIQ", DESCRIPTION)} />
      <DocumentSection fragment="postgres-investigation" />
    </>
  );
}
