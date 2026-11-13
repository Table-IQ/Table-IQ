// One source for the facts every page, schema and export repeats (GEO: same name and line everywhere).
export const SITE = "https://tableiq.ideolyst.com";
export const GITHUB = "https://github.com/RudreshNarwal/Table-IQ";
export const NAME = "TableIQ";
export const TITLE = "TableIQ: Mac database client with visible agent queries";
export const DESCRIPTION =
  "Free, open-source Mac database client for PostgreSQL, MySQL, SQL Server, SQLite, DuckDB and 30+ databases. Let Cursor, Claude Code, and other coding agents query connections you choose, read-only by default.";
export const APP_LINE =
  "Free, open-source Mac database client for browsing, querying and editing databases, with a local MCP server that lets coding agents query chosen connections read-only by default and logs every call.";
export const OG_ALT = "TableIQ: Let your agent in. See every query.";

export const NAV = [
  { href: "/#how", label: "How it works" },
  { href: "/#features", label: "Features" },
  { href: "/#engines", label: "Engines" },
  { href: "/#guides", label: "Guides" },
];

export type Faq = { q: string; a: string; link?: { text: string; href: string } };

// Approved answers from the previous site, verbatim. FAQPage JSON-LD is built from this list.
export const FAQ: Faq[] = [
  { q: "What is TableIQ?", a: "TableIQ is a free, open-source database client for macOS. Browse data, run SQL, review staged edits and explore schemas. Its local MCP server lets compatible coding agents query only connections you share, read-only by default, with every call shown in MCP Activity." },
  { q: "Is TableIQ really free?", a: "Yes. TableIQ-owned application code is open source under AGPL-3.0-only. Third-party components retain their original licenses; see the license exceptions. Previously released Apache-2.0 versions retain their permissions. Every database engine, the MCP server and its guard, BYO-key AI, staged edits, SSH tunnels, unlimited connections and every feature in the first public release stay free. There is no engine or device-count paywall.", link: { text: "license exceptions", href: "/license#third-party" } },
  { q: "Which Macs does it run on?", a: "The planned Mac release will support Apple silicon and Intel Macs. Windows and Linux builds come after the Mac release." },
  { q: "Does my data leave my Mac?", a: "TableIQ connects straight to your databases, or through your SSH bastion. There is no account and no required cloud service, and credentials stay in the macOS Keychain. The built-in AI sends context only to the provider you configure, and only when you use it. External agents follow their own provider’s rules." },
  { q: "Is it safe to point an agent at production?", a: "Start with a read-only, least-privilege role on a replica or a sanitized copy. TableIQ refuses writes by default and logs every agent call, but read access can still expose sensitive data or run expensive queries." },
  { q: "Why Rust, and does it matter?", a: "The work that touches your database runs in a compiled Rust core instead of JavaScript, and the app uses the WebKit already in macOS instead of shipping its own browser. In practice: big tables page by key, supported queries can be cancelled, and the window stays responsive. Cancellation support varies by engine." },
  { q: "Which agents can connect?", a: "Claude Code and Claude Desktop have built-in one-click configurations. Cursor, Windsurf, and other coding agents that support MCP over HTTP or Server-Sent Events (SSE) with a bearer token connect directly to TableIQ’s local endpoint." },
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
