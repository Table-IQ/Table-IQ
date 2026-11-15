import type { Metadata } from "next";
import { APP_ID, NAME, OG_ALT, ORG, SITE, breadcrumbs, graph } from "@/lib/site";

// Next replaces (does not merge) openGraph/twitter per page, so every page gets the full set here.
export function pageMeta(route: string, title: string, description: string): Metadata {
  const url = route === "/" ? `${SITE}/` : SITE + route;
  const image = { url: `${SITE}/img/og-client-first.png`, width: 1200, height: 630, alt: OG_ALT };
  return {
    title, description,
    alternates: { canonical: url, types: { "text/markdown": `${SITE}${route === "/" ? "/index" : route}.md` } },
    openGraph: { type: "website", siteName: NAME, url, title, description, images: [image] },
    twitter: { card: "summary_large_image", title, description, images: [image] },
  };
}

export function guideSchema(route: string, headline: string, description: string) {
  const url = SITE + route;
  return graph(
    { "@type": "TechArticle", headline, url, description, about: { "@id": APP_ID }, author: { "@id": ORG["@id"] }, publisher: { "@id": ORG["@id"] } },
    breadcrumbs([["TableIQ", `${SITE}/`], ["Guides", `${SITE}/#guides`], [headline, url]]),
  );
}
