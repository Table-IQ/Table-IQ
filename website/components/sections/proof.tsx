import { ProductShot } from "@/components/blocks/product-shot";
import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";

export function Proof() {
  return (
    <SectionShell id="agents" surface="paper-3" label="proof-title">
      <SectionHeading id="proof-title" title="A real session: reads answered, one write refused." wide />
      <figure className="rounded-card border border-rule bg-paper p-3 md:p-4">
        <ProductShot name="mcp" sizes="100vw" width={2360} height={688} alt="MCP Activity showing Claude Code listing connections, describing a table and running two SELECT queries, then an UPDATE that TableIQ refused" />
        <figcaption className="mt-4 max-w-3xl px-1 text-ink-2">
          A real Claude Code session against a read-only connection: it listed tables, ran two queries, and its <code>UPDATE</code> was refused before it reached the database.
        </figcaption>
      </figure>
      <p className="mt-6 max-w-3xl text-sm text-muted">
        Activity is a local view of the most recent 1,000 calls. SQL is redacted and can be shortened; full result sets are not stored here. It is not a permanent audit record.
      </p>
    </SectionShell>
  );
}
