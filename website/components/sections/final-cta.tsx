import { SectionShell } from "@/components/blocks/section-shell";
import { Button } from "@/components/ui/button";
import { DOWNLOAD_URL, GITHUB, PLATFORM_STATUS, PREVIEW } from "@/lib/site";

export function FinalCta() {
  return (
    <SectionShell id="download" surface="paper-3" label="download-title" tight>
      <div className="grid items-center gap-8 md:grid-cols-12">
        <img src="/img/brand/app-icon.png" alt="TableIQ app icon" width={72} height={72} loading="lazy" className="md:col-span-1" />
        <div className="md:col-span-7">
          <h2 id="download-title" className="text-section font-medium">Try the Mac developer preview.</h2>
          <p className="mt-4 text-ink-2">This free Apple silicon preview is unsigned and not notarized by Apple. Use a local database or a disposable copy for your first test. The release includes installation instructions, source, license notices and checksums.</p>
          <p className="mt-2 text-sm text-muted">v{PREVIEW.version} · {PREVIEW.platform} · Manual updates</p>
          <p className="mt-2 text-sm text-ink-2">{PLATFORM_STATUS}</p>
        </div>
        <div className="flex flex-wrap gap-4 md:col-span-4 md:justify-end">
          <Button href={DOWNLOAD_URL}>Download developer preview</Button>
          <Button href={GITHUB} variant="secondary">View source</Button>
        </div>
      </div>
      <div className="mt-8 rounded-card border border-rule bg-paper p-6">
        <h3 className="text-lg font-medium">Install the preview</h3>
        <ol className="mt-3 list-decimal space-y-2 pl-5 text-sm text-ink-2">
          <li>Download the DMG, open it and drag TableIQ into Applications. Quit an existing copy before replacing it.</li>
          <li>Try opening TableIQ once. If macOS blocks it, open System Settings → Privacy &amp; Security and choose Open Anyway for TableIQ. Approve only the build you downloaded from this release.</li>
          <li>Connect to disposable data and follow the <a href="/guides/postgres-investigation" className="underline decoration-rule underline-offset-4 hover:text-ink">Postgres walkthrough</a>. Check the release notes for tested macOS versions and known limitations.</li>
        </ol>
      </div>
    </SectionShell>
  );
}
