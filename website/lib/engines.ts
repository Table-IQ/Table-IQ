import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";

// Built at `next build` from the app's own profile list, so the site never drifts from the picker.
// ponytail: logo files in public/img/engines are copies from src/assets/engines; recopy when the app adds a native driver.
type Profile = { base: string; label: string; monogram: string; color: string; tested: boolean };
export type Engine = { label: string; src?: string; monogram?: string; color?: string; beta: boolean };
export type EngineGroup = { title: string; note: string; engines: Engine[] };

const ROOT = process.cwd();
const profiles: Record<string, Profile> = JSON.parse(readFileSync(path.join(ROOT, "../src/lib/profiles.json"), "utf8"));
const marks = readdirSync(path.join(ROOT, "public/img/engines/profiles"));

const SQL: [string, string][] = [["postgres", "PostgreSQL"], ["mysql", "MySQL"], ["sqlserver", "SQL Server"], ["sqlite", "SQLite"], ["duckdb", "DuckDB"]];
const DOC: [string, string][] = [["mongodb", "MongoDB"], ["redis", "Redis"]];
const HTTP: [string, string][] = [["clickhouse", "ClickHouse"], ["libsql", "Turso / libSQL"], ["elasticsearch", "Elasticsearch"], ["opensearch", "OpenSearch"], ["rqlite", "rqlite"], ["meilisearch", "Meilisearch"], ["qdrant", "Qdrant"], ["influxdb", "InfluxDB 3"], ["chroma", "Chroma"], ["weaviate", "Weaviate"]];
const CLOUD = ["rds-", "azure-", "cloudsql-", "neon", "supabase", "alloydb"];
const isCloud = (k: string) => CLOUD.some((p) => k.startsWith(p));

const native = ([key, label]: [string, string]): Engine => ({ label, src: `/img/engines/${key}.svg`, beta: false });
function product(key: string, p: Profile): Engine {
  const mark = marks.find((f) => f.startsWith(`${key}.`));
  return mark ? { label: p.label, src: `/img/engines/profiles/${mark}`, beta: !p.tested } : { label: p.label, monogram: p.monogram, color: p.color, beta: !p.tested };
}

// Verified first, then Beta: the order the app's picker uses.
const sorted = Object.entries(profiles).sort(([, a], [, b]) => Number(!a.tested) - Number(!b.tested));
const seen = new Set<string>();
const cloud = sorted.filter(([k, p]) => isCloud(k) && !seen.has(p.label) && seen.add(p.label)).map(([k, p]) => product(k, p));

export const ENGINE_GROUPS: EngineGroup[] = [
  { title: "Full SQL client", note: "Browse, query, edit, structure, ERD, backup and restore.", engines: [...SQL.map(native), product("mariadb", profiles.mariadb)] },
  { title: "Postgres compatible", note: "Runs on the PostgreSQL driver.", engines: sorted.filter(([k, p]) => p.base === "postgres" && !isCloud(k)).map(([k, p]) => product(k, p)) },
  { title: "MySQL compatible", note: "Runs on the MySQL driver.", engines: sorted.filter(([k, p]) => p.base === "mysql" && !isCloud(k) && k !== "mariadb").map(([k, p]) => product(k, p)) },
  { title: "Managed cloud", note: "Your hosted Postgres and MySQL, on the same drivers.", engines: cloud },
  { title: "Document and key-value", note: "Dedicated browsers with staged edits.", engines: DOC.map(native) },
  { title: "Search, analytics and vector", note: "Read-only over HTTP.", engines: HTTP.map(native) },
];

export const ENGINE_TOTAL = SQL.length + DOC.length + HTTP.length + new Set(Object.values(profiles).map((p) => p.label)).size;
const STRIP = ["postgres", "mysql", "sqlserver", "sqlite", "duckdb", "mongodb", "redis", "clickhouse", "elasticsearch", "qdrant"];
const labels = Object.fromEntries([...SQL, ...DOC, ...HTTP]);
export const ENGINE_STRIP = STRIP.map((k) => ({ label: labels[k] as string, src: `/img/engines/${k}.svg` }));
