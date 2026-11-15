import { ProductShot } from "@/components/blocks/product-shot";
import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";

export function Proof() {
  return (
    <SectionShell id="mcp-preview" surface="paper-3" label="proof-title">
      <SectionHeading id="proof-title" title="See your coding agent’s database activity.">
        Connect a coding agent through MCP, then inspect its recent calls, redacted SQL, row counts and outcomes in TableIQ.
      </SectionHeading>
      <ul className="mb-8 grid gap-6 text-ink-2 md:grid-cols-3">
        <li><strong className="block font-medium text-ink">Choose what to share.</strong> Select connections in Settings → MCP. Writes are off by default.</li>
        <li><strong className="block font-medium text-ink">Inspect recent calls.</strong> Open a connection and choose MCP Activity in the top toolbar to see what the agent requested.</li>
        <li><strong className="block font-medium text-ink">Check the outcome.</strong> See timing, row counts and whether a call succeeded, failed or was refused.</li>
      </ul>
      <figure className="rounded-card border border-rule bg-paper p-3 md:p-4">
        <ProductShot name="mcp" sizes="100vw" width={2360} height={688} alt="Illustrative MCP Activity interface with read queries and a refused write" />
        <figcaption className="mt-4 max-w-3xl px-1 text-ink-2">
          Illustrative interface preview with sample calls, including a refused write. The walkthrough shows real native TableIQ interactions with synthetic data.
        </figcaption>
      </figure>
      <p className="mt-6 max-w-3xl text-sm text-muted">
        Activity is a local view of the most recent 1,000 calls. SQL is redacted and can be shortened; full result sets are not stored here. It is not a permanent audit record.
      </p>
      <a href="/video/tableiq-argo-mcp-56s.mp4" className="mt-6 inline-flex min-h-11 items-center font-semibold underline decoration-rule underline-offset-4 hover:text-ink-2">Watch the 56-second MCP walkthrough</a>
    </SectionShell>
  );
}
