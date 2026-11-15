// One source for the facts every page, schema and export repeats (GEO: same name and line everywhere).
export const SITE = "https://tableiq.ideolyst.com";
export const GITHUB = "https://github.com/Table-IQ/Table-IQ";
export const NAME = "TableIQ";

// Update only after the release and exact asset are verified publicly accessible.
// The release page remains the fallback until a DMG URL is confirmed.
const PREVIEW_VERSION = "0.8.17";
export const PREVIEW = {
  version: PREVIEW_VERSION,
  tag: `preview-v${PREVIEW_VERSION}`,
  releaseUrl: `${GITHUB}/releases/tag/preview-v${PREVIEW_VERSION}`,
  dmgUrl: `${GITHUB}/releases/download/preview-v${PREVIEW_VERSION}/TableIQ_${PREVIEW_VERSION}_aarch64.dmg`,
  platform: "Mac · Apple silicon",
};
export const DOWNLOAD_URL = PREVIEW.dmgUrl || PREVIEW.releaseUrl;
export const PLATFORM_STATUS = "Mac developer preview available · Windows and Linux coming soon.";

// Populate with the real, verified recording and its captions; no placeholder player.
export const DEMOS = [
  { id: "argo", title: "A narrated walkthrough", maker: "Argo", src: "/video/tableiq-argo-mcp-56s.mp4", poster: "/video/tableiq-argo-poster.png", captions: "/video/tableiq-argo.vtt", duration: 56, description: "Follow the real MCP setup, inspect the agent’s calls, then verify the answer with a manual query." },
  { id: "huashu", title: "Follow the investigation", maker: "Huashu Motion", src: "/video/tableiq-huashu-motion-56s.mp4", poster: "/video/tableiq-huashu-poster.png", captions: "/video/tableiq-huashu.vtt", duration: 56, description: "Choose an agent connection, inspect its calls and SQL, then verify two returned orders." },
  { id: "take-one", title: "Trace the answer", maker: "Take One", src: "/video/tableiq-take-one-story-50s.mp4", poster: "/video/tableiq-take-one-poster.png", captions: "/video/tableiq-take-one.vtt", duration: 50, description: "A short story about finding paid orders that haven’t shipped and checking the evidence." },
];
export const TITLE = "TableIQ | Free, open-source database client for Mac";
export const DESCRIPTION =
  "Free, open-source database client for Mac. SQL and table tabs without a paywall. Optional AI with your own key. Apple silicon developer preview.";
export const APP_LINE =
  "TableIQ is a free, open-source Mac database client for browsing tables, running SQL and reviewing staged data edits without a paid tab limit, with optional bring-your-own-key AI and external-agent integrations.";
export const OG_ALT = "TableIQ: A free, open-source database client for Mac. Browse. Query. Review your changes.";

export const NAV = [
  { href: "/#demo", label: "Demo" },
  { href: "/#how", label: "How it works" },
  { href: "/#features", label: "Features" },
  { href: "/#ai", label: "Optional AI" },
  { href: "/#compare", label: "Compare" },
  { href: "/#engines", label: "Engines" },
];

export type Faq = { q: string; a: string; link?: { text: string; href: string } };

// Shared current answers used by visible FAQ and structured data. FAQPage JSON-LD is built from this list.
export const FAQ: Faq[] = [
  { q: "What is TableIQ?", a: "TableIQ is a free, open-source database client for macOS. Browse tables, run SQL, review staged data edits and explore schemas. It works without AI or an external coding agent. Optional AI settings let you configure your own provider; advanced MCP integrations are available separately." },
  { q: "Is TableIQ really free?", a: "Yes. TableIQ-owned application code is open source under AGPL-3.0-only. Third-party components retain their original licenses; see the license exceptions. Previously released Apache-2.0 versions retain their permissions. Every database engine, the MCP server and its guard, BYO-key AI, staged edits, SSH tunnels, unlimited connections and every feature in the first public release stay free. There is no engine or device-count paywall.", link: { text: "license exceptions", href: "/license#third-party" } },
  { q: "Which platforms are available?", a: "The Mac developer preview is available for Apple silicon. Windows and Linux versions are coming soon. The current Mac preview is unsigned and not notarized by Apple; see the release notes for tested macOS versions. An Intel Mac installer is not included in this preview." },
  { q: "Does my data leave my Mac?", a: "TableIQ connects straight to your databases, or through your SSH bastion. There is no required account or product telemetry, and credentials stay in the macOS Keychain. Startup update checks fetch public release information from GitHub and can be disabled in Settings. No installation ID, email, SQL or connection details are sent; GitHub receives normal network information such as your IP address. The built-in AI sends context to the provider you configure when you use it. External agents follow their own provider’s rules." },
  { q: "Do I need AI or an account to use it?", a: "No. Connect, browse, run SQL and review grid edits without an account, AI key or coding agent. The current desktop features have no engine or device-count paywall. AI is optional, and external providers may charge separately for API usage." },
  { q: "Which AI providers can I configure?", a: "Settings → AI includes Anthropic, OpenAI, Gemini, OpenRouter and local Ollama. Cloud providers need their own API credentials and may bill for usage; a consumer chat subscription may not include API access. Provider and model compatibility varies in this preview. Check any suggested SQL before running it." },
  { q: "Can it replace my current database client?", a: "Try one everyday task beside your current client first. This is an Apple silicon developer preview, with capability and cancellation support varying by engine. It does not preserve a server session, SET state or an open transaction between separate query runs. Check the release notes and engine details before relying on a workflow.", link: { text: "Known limitations", href: `${GITHUB}#known-limitations` } },
];

export const ORG = { "@type": "Organization", "@id": "https://ideolyst.com/#org", name: "Ideolyst", url: "https://ideolyst.com" };
export const APP_ID = `${SITE}/#app`;

export function graph(...nodes: object[]) {
  return { "@context": "https://schema.org", "@graph": [ORG, ...nodes] };
}

export function breadcrumbs(items: [string, string][]) {
  return {
    "@type": "BreadcrumbList",
    itemListElement: items.map(([name, item], i) => ({ "@type": "ListItem", position: i + 1, name, item })),
  };
}

export type Story = { name: string; from: string; quote: string };

// Beta users, full names published with their consent (record kept privately, not in this repo).
// Wording drafted from their reported reasons for switching; each person must approve their exact text before deploy.
export const STORIES: Story[] = [
  { name: "Gulshan Yadav", from: "DBeaver", quote: "DBeaver felt really heavy for everyday work. I wanted a database client that stays light, so I moved to TableIQ." },
  { name: "Deepak Saxena", from: "TablePlus free plan", quote: "On the TablePlus free plan, a popup stopped me after two tabs. In TableIQ, no popup stops me mid-query." },
];
