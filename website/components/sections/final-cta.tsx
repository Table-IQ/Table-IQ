import { SectionShell } from "@/components/blocks/section-shell";
import { Button } from "@/components/ui/button";
import { GITHUB } from "@/lib/site";

export function FinalCta() {
  return (
    <SectionShell id="download" surface="paper-3" label="download-title" tight>
      <div className="grid items-center gap-8 md:grid-cols-12">
        <img src="/img/brand/app-icon.png" alt="TableIQ app icon" width={72} height={72} loading="lazy" className="md:col-span-1" />
        <div className="md:col-span-7">
          <h2 id="download-title" className="text-section font-medium">TableIQ for Mac is coming soon.</h2>
          <p className="mt-4 text-ink-2">The signed Mac download is not available yet. The planned release will support Apple silicon and Intel Macs. Source and preview releases currently require repository access.</p>
          <p className="mt-2 font-medium">Download for Mac: coming soon.</p>
        </div>
        <div className="flex flex-wrap gap-4 md:col-span-4 md:justify-end">
          <Button href="/guides/postgres-investigation">Read the walkthrough</Button>
          <Button href={GITHUB} variant="secondary">GitHub (access required)</Button>
        </div>
      </div>
    </SectionShell>
  );
}
