import type { Engine } from "../../lib/commands";
import { profileOf } from "../../lib/profiles";

/**
 * TablePlus-style engine badge — a soft-coloured circle with a 2-letter monogram.
 * Used everywhere a connection is identified (sidebar list, ⌘O picker, MainShell
 * breadcrumb dot). Deliberately matches TablePlus's idiom rather than rendering
 * vendor logos, both for trademark hygiene and so the colour is what carries
 * recognition at a glance. A connection saved as a compatible product (TiDB,
 * CockroachDB, …) takes its fill, monogram and label from `profiles.json`.
 */
export type EngineIconSize = "sm" | "md" | "lg" | "xl";

const PX: Record<EngineIconSize, number> = { sm: 16, md: 22, lg: 32, xl: 44 };
const FONT: Record<EngineIconSize, number> = { sm: 9, md: 10, lg: 12, xl: 15 };

export const ENGINE_COLORS: Record<Engine, string> = {
  postgres: "#336791",
  mysql: "#4479a1",
  sqlite: "#003b57",
  duckdb: "#b8860b",
  redis: "#dc382d",
  mongodb: "#13aa52",
  sqlserver: "#cc2927",
  clickhouse: "#946c00",
  libsql: "#0f766e",
  elasticsearch: "#0b6e99",
};

export const ENGINE_MONOGRAMS: Record<Engine, string> = {
  postgres: "Pg",
  mysql: "My",
  sqlite: "Sq",
  duckdb: "Dk",
  redis: "Re",
  mongodb: "Mo",
  sqlserver: "Ms",
  clickhouse: "Ch",
  libsql: "Tu",
  elasticsearch: "Es",
};

export const ENGINE_LABELS: Record<Engine, string> = {
  postgres: "PostgreSQL",
  mysql: "MySQL",
  sqlite: "SQLite",
  duckdb: "DuckDB",
  redis: "Redis",
  mongodb: "MongoDB",
  sqlserver: "SQL Server",
  clickhouse: "ClickHouse",
  libsql: "Turso / libSQL",
  elasticsearch: "Elasticsearch",
};

export function EngineIcon({
  engine,
  profile,
  size = "md",
  className = "",
  title,
}: {
  engine: Engine;
  /** The connection's product (a key of `profiles.json`); unknown ids draw the base engine. */
  profile?: string | null;
  size?: EngineIconSize;
  className?: string;
  /** Tooltip; passing one also names the icon for assistive tech (it is decorative otherwise). */
  title?: string;
}) {
  const px = PX[size];
  const font = FONT[size];
  const product = profileOf(engine, profile);
  const color = product?.color ?? ENGINE_COLORS[engine];
  return (
    <span
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={!title}
      title={title ?? engineLabel(engine, profile)}
      className={`inline-flex shrink-0 items-center justify-center font-semibold text-white ${className}`}
      style={{
        width: px,
        height: px,
        borderRadius: "50%",
        fontSize: font,
        letterSpacing: "-0.02em",
        // TablePlus uses a flat fill plus a faint inner highlight that reads as
        // glassy depth. The inset shadow is what stops the circle from feeling
        // like a Tailwind avatar — it gives the tile a manufactured edge.
        background: color,
        boxShadow:
          "inset 0 1px 0 rgb(255 255 255 / 0.18), inset 0 -1px 0 rgb(0 0 0 / 0.18)",
      }}
    >
      {product?.monogram ?? ENGINE_MONOGRAMS[engine]}
    </span>
  );
}

/** The product's label when the profile is one this build knows, else the engine's. */
export function engineLabel(engine: Engine, profile?: string | null): string {
  // OpenSearch is the Elasticsearch engine under the name the server reported at connect (no `profiles.json` entry).
  if (engine === "elasticsearch" && profile === "opensearch") return "OpenSearch";
  return profileOf(engine, profile)?.label ?? ENGINE_LABELS[engine] ?? engine;
}
