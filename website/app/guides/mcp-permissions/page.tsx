import { JsonLd } from "@/components/blocks/json-ld";
import { DocumentSection } from "@/components/sections/document";
import { guideSchema, pageMeta } from "@/lib/meta";

const ROUTE = "/guides/mcp-permissions";
const TITLE = "Troubleshoot MCP permissions: TableIQ";
const DESCRIPTION = "How TableIQ's MCP server shares connections with coding agents: tokens, read-only defaults, refused writes, row caps and engine-specific protections, with fixes for common errors.";

export const metadata = pageMeta(ROUTE, TITLE, DESCRIPTION);

export default function Page() {
  return (
    <>
      <JsonLd data={guideSchema(ROUTE, "Troubleshoot TableIQ agent permissions", DESCRIPTION)} />
      <DocumentSection fragment="mcp-permissions" />
    </>
  );
}
