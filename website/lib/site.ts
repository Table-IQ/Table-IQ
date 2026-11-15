// One source for the facts every page, schema and export repeats (GEO: same name and line everywhere).
export const SITE = "https://tableiq.ideolyst.com";
export const GITHUB = "https://github.com/Table-IQ/Table-IQ";
export const NAME = "TableIQ";

// Update only after the release and exact asset are verified publicly accessible.
// The release page remains the fallback until a DMG URL is confirmed.
const PREVIEW_VERSION = "0.8.15";
export const PREVIEW = {
  version: PREVIEW_VERSION,
  tag: `preview-v${PREVIEW_VERSION}`,
  releaseUrl: `${GITHUB}/releases/tag/preview-v${PREVIEW_VERSION}`,
  dmgUrl: "",
  platform: "Mac · Apple silicon",
};
export const DOWNLOAD_URL = PREVIEW.dmgUrl || PREVIEW.releaseUrl;

// Populate with the real, verified recording and its captions; no placeholder player.
export const DEMOS = [
  { id: "huashu", title: "Follow the investigation", maker: "Huashu Motion", src: "/video/tableiq-huashu-motion-56s.mp4", poster: "/video/tableiq-huashu-poster.png", captions: "/video/tableiq-huashu.vtt", duration: 56, description: "Choose an agent connection, inspect its calls and SQL, then verify two returned orders." },
  { id: "take-one", title: "Trace the answer", maker: "Take One", src: "/video/tableiq-take-one-story-50s.mp4", poster: "/video/tableiq-take-one-poster.png", captions: "/video/tableiq-take-one.vtt", duration: 50, description: "A short story about finding paid orders that haven’t shipped and checking the evidence." },
];
export const TITLE = "TableIQ: Mac database client with visible agent queries";
export const DESCRIPTION =
  "Free, open-source Mac database client for PostgreSQL, MySQL, SQL Server, SQLite, DuckDB and 30+ databases. Let Cursor, Claude Code, and other coding agents query connections you choose, read-only by default.";
export const APP_LINE =
  "Free, open-source Mac database client for browsing, querying and editing databases, with a local MCP server that lets coding agents query chosen connections read-only by default and logs every call.";
export const OG_ALT = "TableIQ: Let your agent in. See every query.";

export const NAV = [
  { href: "/#demo", label: "Demo" },
  { href: "/#how", label: "How it works" },
  { href: "/#features", label: "Features" },
  { href: "/#engines", label: "Engines" },
  { href: "/#compare", label: "Compare" },
  { href: "/#guides", label: "Guides" },
];

export type Faq = { q: string; a: string; link?: { text: string; href: string } };

// Shared current answers used by visible FAQ and structured data. FAQPage JSON-LD is built from this list.
export const FAQ: Faq[] = [
  { q: "What is TableIQ?", a: "TableIQ is a free, open-source database client for macOS. Browse data, run SQL, review staged edits and explore schemas. Its local MCP server lets compatible coding agents query only connections you share, read-only by default, with recent calls shown in MCP Activity. Activity stores redacted SQL, row counts and outcomes, not full result sets." },
  { q: "Is TableIQ really free?", a: "Yes. TableIQ-owned application code is open source under AGPL-3.0-only. Third-party components retain their original licenses; see the license exceptions. Previously released Apache-2.0 versions retain their permissions. Every database engine, the MCP server and its guard, BYO-key AI, staged edits, SSH tunnels, unlimited connections and every feature in the first public release stay free. There is no engine or device-count paywall.", link: { text: "license exceptions", href: "/license#third-party" } },
  { q: "Which Macs does it run on?", a: "The developer preview is for Apple silicon Macs. It is unsigned and not notarized by Apple. See the release notes for tested macOS versions. Intel, Windows and Linux installers are not part of this preview." },
  { q: "Does my data leave my Mac?", a: "TableIQ connects straight to your databases, or through your SSH bastion. There is no required account or product telemetry, and credentials stay in the macOS Keychain. Startup update checks fetch public release information from GitHub and can be disabled in Settings. No installation ID, email, SQL or connection details are sent; GitHub receives normal network information such as your IP address. The built-in AI sends context to the provider you configure when you use it. External agents follow their own provider’s rules." },
  { q: "Is it safe to point an agent at production?", a: "Start with a read-only, least-privilege role on a replica or a sanitized copy. TableIQ refuses writes by default and logs every agent call, but read access can still expose sensitive data or run expensive queries." },
  { q: "Why Rust, and does it matter?", a: "The work that touches your database runs in a compiled Rust core instead of JavaScript, and the app uses the WebKit already in macOS instead of shipping its own browser. In practice: big tables page by key, supported queries can be cancelled, and the window stays responsive. Cancellation support varies by engine." },
  { q: "Which agents can connect?", a: "Settings → MCP provides copyable setup for Claude Code, Codex, OpenCode 2, legacy OpenCode 1 and Claude Desktop. Local clients use authenticated Streamable HTTP; Claude Desktop uses a Node-based bridge. Keep TableIQ running on the same Mac. Hosted cloud agents cannot reach your Mac through their own localhost.", link: { text: "Client setup guide", href: `${GITHUB}/blob/main/docs/launch/AGENT-SETUP.md` } },
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
