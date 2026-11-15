import { FeatureItem } from "@/components/blocks/items";
import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";

const LARGE = [
  { title: "Built in Rust. Fast where it counts.", body: "Drivers, paging, staged commits and the MCP safety guard run in a native Rust core. Built on Tauri with the WebKit already in macOS: no Electron, no extra copy of Chromium." },
  { title: "Keyset paging", body: "Pages load by key instead of OFFSET, the method that keeps deep pages practical. Exercised against a 20-million-row test table. Use ⌘. to cancel supported queries; cancellation support varies by engine." },
];
const SMALL = [
  { title: "Command palette", body: "⌘K finds tables, databases, saved queries and every action." },
  { title: "Many connections", body: "Keep several open. A rail switches between them without reconnecting." },
  { title: "SSH tunnels", body: "Password, agent or key-file auth, with host-key verification." },
  { title: "Environment tags", body: "Local to production. Production tints the title bar and guards commits." },
  { title: "Bring-your-own-key AI", body: "Text to SQL, explain, optimize and fix with Anthropic, OpenAI, Gemini, OpenRouter or local Ollama. Review generated SQL before running it." },
  { title: "Backup and restore", body: "pg_dump, mysqldump, SQL Server backups, SQLite and DuckDB files." },
  { title: "User management", body: "Create roles and change access on Postgres, MySQL and SQL Server, with a SQL preview." },
  { title: "Metrics board", body: "Read-only number, bar, line and table widgets that refresh on a timer." },
  { title: "CSV import", body: "Load a CSV into a table from the database lobby." },
];

export function Toolkit() {
  return (
    <SectionShell surface="paper-2" label="toolkit-title">
      <SectionHeading id="toolkit-title" title="The rest of a full desktop client.">
        Filter with stacked rules or raw WHERE, save queries and run history per connection. Passwords are masked in the SQL log.
      </SectionHeading>
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        {LARGE.map((f) => (
          <div key={f.title} className="md:col-span-2 lg:row-span-2">
            <FeatureItem title={f.title} large>{f.body}</FeatureItem>
          </div>
        ))}
        {SMALL.map((f) => (
          <FeatureItem key={f.title} title={f.title}>{f.body}</FeatureItem>
        ))}
      </div>
    </SectionShell>
  );
}
