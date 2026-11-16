import { JsonLd } from "@/components/blocks/json-ld";
import { Film } from "@/components/sections/demo";
import { pageMeta } from "@/lib/meta";
import { APP_ID, GITHUB, SITE, graph } from "@/lib/site";

const DESCRIPTION = "Watch three TableIQ launch films: a motion introduction, a story and a guided walkthrough of the same sample investigation.";
export const metadata = pageMeta("/launch-films", "TableIQ launch films", DESCRIPTION);
const schema = graph({
  "@type": "WebPage", name: "TableIQ launch films", url: `${SITE}/launch-films`,
  description: DESCRIPTION, about: { "@id": APP_ID },
});

const FILMS = [
  {
    id: "motion",
    title: "The motion introduction",
    duration: "56 seconds",
    description: "Music and short headings lead from query tabs to the free database client, optional AI and visible MCP calls.",
    maker: "HyperFrames composition",
    src: "/video/tableiq-launch-motion-v2.mp4",
    poster: "/video/tableiq-launch-motion-v2-poster.jpg",
    captions: "/video/tableiq-launch-motion-v2.vtt",
  },
  {
    id: "story",
    title: "The story cut",
    duration: "57.5 seconds",
    description: "A narrated investigation follows paid orders without a shipped shipment, through the query and its two matching results.",
    maker: "HyperFrames composition with local narration",
    src: "/video/tableiq-launch-story-v2.mp4",
    poster: "/video/tableiq-launch-story-v2-poster.jpg",
    captions: "/video/tableiq-launch-story-v2.vtt",
  },
  {
    id: "walkthrough",
    title: "The guided walkthrough",
    duration: "57 seconds",
    description: "Narration guides the client workflow, explains bring your own key AI and shows the verified result and actual MCP Activity.",
    maker: "HyperFrames picture with Argo alignment and export",
    src: "/video/tableiq-launch-walkthrough-v2.mp4",
    poster: "/video/tableiq-launch-walkthrough-v2-poster.jpg",
    captions: "/video/tableiq-launch-walkthrough-v2.vtt",
  },
];

export default function LaunchFilmsPage() {
  return (
    <div className="mx-auto max-w-site px-gutter pt-14 pb-24 md:pt-24">
      <JsonLd data={schema} />
      <div className="max-w-3xl">
        <a href="/" className="text-sm text-muted underline underline-offset-4">TableIQ home</a>
        <h1 className="mt-6 text-section font-medium text-balance">Three ways to show TableIQ.</h1>
        <p className="mt-6 text-lead text-ink-2">Watch the motion introduction, story and guided walkthrough. Each follows the same sample database investigation.</p>
        <p className="mt-5 text-sm text-muted">The films combine real development build recordings with a previously verified native AI example. SQL excerpts and native result stills are identified on screen. The MCP calls come from a real local protocol client in a separate sample session.</p>
        <nav aria-label="Choose a launch film" className="mt-8 flex flex-wrap gap-x-6 gap-y-3 text-sm">
          <a href="#motion" className="underline underline-offset-4">Motion</a>
          <a href="#story" className="underline underline-offset-4">Story</a>
          <a href="#walkthrough" className="underline underline-offset-4">Walkthrough</a>
        </nav>
      </div>
      <div className="mt-14 space-y-16">
        {FILMS.map((film) => (
          <article key={film.id} id={film.id} aria-labelledby={`${film.id}-title`} className="max-w-5xl scroll-mt-28">
            <h2 id={`${film.id}-title`} className="text-title font-semibold">{film.title}</h2>
            <p className="mt-3 max-w-prose text-ink-2">{film.description}</p>
            <p className="mt-3 text-sm text-muted">{film.duration} · {film.maker}</p>
            <div className="mt-6"><Film {...film} /></div>
            <div className="mt-4 flex flex-wrap gap-x-6 gap-y-3 text-sm">
              <a href={film.src} download className="underline underline-offset-4">Download this film</a>
              <a href={film.captions} className="underline underline-offset-4">Read captions</a>
            </div>
          </article>
        ))}
      </div>
      <p className="mt-14 max-w-prose text-sm text-muted">Synthetic data and edited time. This example does not establish a general AI accuracy rate or a performance comparison. Cloud AI provider charges may apply. <a href={`${GITHUB}/blob/main/docs/launch/research/2026-11-16-native-ai-acceptance.md`} className="underline underline-offset-4">Read the native AI verification</a>.</p>
    </div>
  );
}
