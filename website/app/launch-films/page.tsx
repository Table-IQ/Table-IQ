import { JsonLd } from "@/components/blocks/json-ld";
import { Film } from "@/components/sections/demo";
import { pageMeta } from "@/lib/meta";
import { APP_ID, GITHUB, SITE, graph } from "@/lib/site";

const DESCRIPTION = "Watch three TableIQ launch films: a guided walkthrough, a story and a motion introduction of the same sample investigation.";
export const metadata = pageMeta("/launch-films", "TableIQ launch films", DESCRIPTION);
const schema = graph({
  "@type": "WebPage", name: "TableIQ launch films", url: `${SITE}/launch-films`,
  description: DESCRIPTION, about: { "@id": APP_ID },
});

const FILMS = [
  {
    id: "walkthrough",
    title: "The guided walkthrough",
    duration: "75 seconds",
    description: "Follow real table and schema navigation, SQL entry and manual Run. Then explore optional AI and actual MCP Activity.",
    maker: "HyperFrames picture with Argo alignment and export",
    src: "/video/tableiq-launch-walkthrough-v5.mp4",
    poster: "/video/tableiq-launch-walkthrough-v5-poster.jpg",
    captions: "/video/tableiq-launch-walkthrough-v5.vtt",
  },
  {
    id: "story",
    title: "The story cut",
    duration: "75 seconds",
    description: "A narrated switching story follows everyday client work into a real SQL investigation and its two matching orders.",
    maker: "HyperFrames composition with local narration",
    src: "/video/tableiq-launch-story-v5.mp4",
    poster: "/video/tableiq-launch-story-v5-poster.jpg",
    captions: "/video/tableiq-launch-story-v5.vtt",
  },
  {
    id: "motion",
    title: "The motion introduction",
    duration: "75 seconds",
    description: "A focused introduction for Mac developers: room for more tabs, familiar database work, then optional AI and agent visibility.",
    maker: "HyperFrames composition",
    src: "/video/tableiq-launch-motion-v5.mp4",
    poster: "/video/tableiq-launch-motion-v5-poster.jpg",
    captions: "/video/tableiq-launch-motion-v5.vtt",
  },
];

export default function LaunchFilmsPage() {
  return (
    <div className="mx-auto max-w-site px-gutter pt-14 pb-24 md:pt-24">
      <JsonLd data={schema} />
      <div className="max-w-3xl">
        <a href="/" className="text-sm text-muted underline underline-offset-4">TableIQ home</a>
        <h1 className="mt-6 text-section font-medium text-balance">Three ways to show TableIQ.</h1>
        <p className="mt-6 text-lead text-ink-2">Start with the guided walkthrough, then compare the story and motion introduction. Each follows the same sample database investigation.</p>
        <p className="mt-5 text-sm text-muted">The films combine real development build recordings with a previously verified native AI example. SQL excerpts and native result stills are identified on screen. The MCP calls come from a real local protocol client in a separate sample session.</p>
        <nav aria-label="Choose a launch film" className="mt-8 flex flex-wrap gap-x-6 gap-y-3 text-sm">
          <a href="#walkthrough" className="underline underline-offset-4">Guided walkthrough</a>
          <a href="#story" className="underline underline-offset-4">Story</a>
          <a href="#motion" className="underline underline-offset-4">Motion</a>
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
