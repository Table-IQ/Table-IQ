import { JsonLd } from "@/components/blocks/json-ld";
import { DocumentSection } from "@/components/sections/document";
import { pageMeta } from "@/lib/meta";
import { APP_ID, SITE, graph } from "@/lib/site";

const TITLE = "TableIQ licensing: AGPL-3.0-only and third-party notices";
const DESCRIPTION = "TableIQ application code uses AGPL-3.0-only, including unsigned previews. Read the full terms, third-party exceptions, and earlier-release licensing policy.";

export const metadata = pageMeta("/license", TITLE, DESCRIPTION);

const schema = graph({
  "@type": "WebPage", name: "TableIQ licensing", url: `${SITE}/license`,
  description: "AGPL-3.0-only for TableIQ-owned application code, including unsigned previews; third-party licenses preserved.",
  about: { "@id": APP_ID },
});

export default function Page() {
  return (
    <>
      <JsonLd data={schema} />
      <DocumentSection fragment="license" />
    </>
  );
}
