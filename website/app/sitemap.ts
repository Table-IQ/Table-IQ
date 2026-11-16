import type { MetadataRoute } from "next";
import { SITE } from "@/lib/site";

export const dynamic = "force-static";

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    { url: SITE, priority: 1 },
    { url: `${SITE}/guides/mcp-permissions`, priority: 0.7 },
    { url: `${SITE}/guides/postgres-investigation`, priority: 0.7 },
    { url: `${SITE}/license`, priority: 0.5 },
    { url: `${SITE}/launch-films`, priority: 0.4 },
  ];
}
