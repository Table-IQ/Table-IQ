import { ProductShot } from "@/components/blocks/product-shot";
import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";

export function Proof() {
  return (
    <SectionShell id="mcp-preview" surface="paper-3" label="proof-title">
      <SectionHeading id="proof-title" title="MCP Activity, at a glance." wide />
      <figure className="rounded-card border border-rule bg-paper p-3 md:p-4">
        <ProductShot name="mcp" sizes="100vw" width={2360} height={688} alt="Illustrative MCP Activity interface with read queries and a refused write" />
        <figcaption className="mt-4 max-w-3xl px-1 text-ink-2">
          An interface preview with sample calls, including a refused write. This still is illustrative; the recordings below show actual native app interactions with synthetic data.
        </figcaption>
      </figure>
      <p className="mt-6 max-w-3xl text-sm text-muted">
        Activity is a local view of the most recent 1,000 calls. SQL is redacted and can be shortened; full result sets are not stored here. It is not a permanent audit record.
      </p>
    </SectionShell>
  );
}
