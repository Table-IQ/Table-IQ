import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";
import { DEMOS, DOWNLOAD_URL, GITHUB } from "@/lib/site";

const CLIENT_DEMO = {
  title: "TableIQ for everyday database work",
  src: "/video/tableiq-launch-walkthrough-v3.mp4",
  poster: "/video/tableiq-launch-walkthrough-v3-poster.jpg",
  captions: "/video/tableiq-launch-walkthrough-v3.vtt",
};

type FilmProps = { title: string; src: string; poster: string; captions: string };

export function Film({ title, src, poster, captions }: FilmProps) {
  return (
    <video controls playsInline preload="none" poster={poster} className="aspect-video w-full rounded-card border border-rule bg-paper-3" aria-label={title}>
      <source src={src} type="video/mp4" />
      <track kind="captions" src={captions} srcLang="en" label="English" default />
      <p>Your browser cannot play this recording. <a href={src}>Download the demo video</a>.</p>
    </video>
  );
}

export function Demo() {
  return (
    <SectionShell id="demo" surface="paper-2" label="demo-title" tight>
      <SectionHeading id="demo-title" title="A database connection. A query. An answer.">
        See the everyday workflow: inspect your schema, open SQL, run it and check the returned rows. Configure your own AI provider when you want assistance.
      </SectionHeading>
      <Film {...CLIENT_DEMO} />
      <p className="mt-4 text-sm text-muted">Argo + HyperFrames · Real development recordings · Labelled native 0.8.17 AI evidence · Synthetic data · Edited time</p>
      <div className="mt-5 flex flex-wrap gap-x-6 gap-y-3 text-sm">
        <a href={DOWNLOAD_URL} className="font-semibold underline underline-offset-4">Try the Mac developer preview</a>
        <a href={CLIENT_DEMO.src} download className="underline underline-offset-4">Download the video</a>
        <a href="/launch-films/" className="underline underline-offset-4">Compare the launch films</a>
        <a href={`${GITHUB}/issues/new?template=beta-feedback.yml`} className="underline underline-offset-4">Share beta feedback</a>
      </div>
    </SectionShell>
  );
}

/** Advanced examples belong with the optional agent integration, below the client. */
export function AdvancedDemos() {
  return (
    <div className="mt-10">
      <h3 className="mb-6 text-xl font-semibold text-ink">Explore the agent workflow</h3>
      <div className="grid gap-8 lg:grid-cols-2">
        {DEMOS.map((demo) => <article key={demo.id}>
          <Film {...demo} />
          <h4 className="mt-4 text-lg font-semibold text-ink">{demo.title}</h4>
          <p className="mt-2 text-ink-2">{demo.description}</p>
          <p className="mt-2 text-sm text-muted">{demo.duration} seconds · Created with {demo.maker}</p>
          <a className="mt-3 inline-block text-sm underline underline-offset-4" href={demo.src} download>Download this video</a>
        </article>)}
      </div>
      <p className="mt-6 text-sm text-muted">These three MCP walkthroughs use native TableIQ 0.8.15 footage and synthetic data. Cuts condense time; they are not performance benchmarks.</p>
    </div>
  );
}
