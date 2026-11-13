import { SectionShell } from "@/components/blocks/section-shell";
import { Button } from "@/components/ui/button";

export function NotFoundSection() {
  return (
    <SectionShell label="nf-title">
      <h1 id="nf-title" className="text-section font-medium">This page does not exist.</h1>
      <p className="mt-4 text-ink-2">The link may be old. Start from the home page or the Postgres walkthrough.</p>
      <div className="mt-8 flex flex-wrap gap-4"><Button href="/">TableIQ home</Button><Button href="/guides/postgres-investigation" variant="secondary">Read the walkthrough</Button></div>
    </SectionShell>
  );
}
