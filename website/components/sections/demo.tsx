import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";
import { DEMOS, DOWNLOAD_URL, GITHUB } from "@/lib/site";

// Only renders once a real recording, captions and duration are supplied centrally.
export function Demo() {
  return (
    <SectionShell id="demo" surface="paper-2" label="demo-title" tight>
      <SectionHeading id="demo-title" title="See a real investigation.">
        Follow a database question through TableIQ, inspect the agent’s calls and verify the result. Recorded with synthetic data in the native app.
      </SectionHeading>
      <div className="grid gap-8 lg:grid-cols-2">
        {DEMOS.map((demo) => <article key={demo.id}>
          <video controls playsInline preload="none" poster={demo.poster} className="aspect-video w-full rounded-card border border-rule bg-paper-3" aria-label={`${demo.title}: TableIQ real application demo`}>
            <source src={demo.src} type="video/mp4" />
            <track kind="captions" src={demo.captions} srcLang="en" label="English" default />
            <p>Your browser cannot play this recording. <a href={demo.src}>Download the demo video</a>.</p>
          </video>
          <h3 className="mt-4 text-xl font-semibold text-ink">{demo.title}</h3>
          <p className="mt-2 text-ink-2">{demo.description}</p>
          <p className="mt-2 text-sm text-muted">{demo.duration} seconds · Created with {demo.maker}</p>
          <a className="mt-3 inline-block text-sm underline underline-offset-4" href={demo.src} download>Download this video</a>
        </article>)}
      </div>
      <p className="mt-6 text-sm text-muted">Real native TableIQ 0.8.15 footage · Synthetic data · Edited demonstrations with time condensed · No performance benchmark</p>
      <div className="mt-5 flex flex-wrap gap-x-6 gap-y-3 text-sm">
        <a href={DOWNLOAD_URL} className="font-semibold underline underline-offset-4">Try the Mac developer preview</a>
        <a href={`${GITHUB}/issues/new?template=beta-feedback.yml`} className="underline underline-offset-4">Share beta feedback</a>
      </div>
    </SectionShell>
  );
}
