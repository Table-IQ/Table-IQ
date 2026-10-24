import type { Engine } from "../../lib/commands";
import { profileOf } from "../../lib/profiles";
import postgres from "../../assets/engines/postgres.svg";
import mysql from "../../assets/engines/mysql.svg";
import sqlite from "../../assets/engines/sqlite.svg";
import duckdb from "../../assets/engines/duckdb.svg";
import redis from "../../assets/engines/redis.svg";
import mongodb from "../../assets/engines/mongodb.svg";
import sqlserver from "../../assets/engines/sqlserver.svg";
import clickhouse from "../../assets/engines/clickhouse.svg";
import libsql from "../../assets/engines/libsql.svg";
import elasticsearch from "../../assets/engines/elasticsearch.svg";
import rqlite from "../../assets/engines/rqlite.svg";
import meilisearch from "../../assets/engines/meilisearch.svg";
import qdrant from "../../assets/engines/qdrant.svg";
import chroma from "../../assets/engines/chroma.svg";
import weaviate from "../../assets/engines/weaviate.svg";
import influxdb from "../../assets/engines/influxdb.svg";
import opensearch from "../../assets/engines/opensearch.svg";

/**
 * Bundled vendor marks identify each engine consistently in sheets, lists and
 * pickers. Compatible products retain their own badge from profiles.json instead
 * of being mislabeled with the underlying wire protocol's vendor logo.
 */
export type EngineIconSize = "sm" | "md" | "lg" | "xl";

const PX: Record<EngineIconSize, number> = { sm: 16, md: 22, lg: 32, xl: 44 };
const FONT: Record<EngineIconSize, number> = { sm: 9, md: 10, lg: 12, xl: 15 };
const LOGOS: Record<Engine, string> = {
  postgres, mysql, sqlite, duckdb, redis, mongodb, sqlserver, clickhouse, libsql,
  elasticsearch, rqlite, meilisearch, qdrant, chroma, weaviate, influxdb,
};

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
  rqlite: "#be185d",
  meilisearch: "#6d28d9",
  qdrant: "#dc244c",
  chroma: "#c2410c",
  weaviate: "#4d7c0f",
  influxdb: "#4f46e5",
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
  rqlite: "Rq",
  meilisearch: "Me",
  qdrant: "Qr",
  chroma: "Cd",
  weaviate: "We",
  influxdb: "In",
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
  rqlite: "rqlite",
  meilisearch: "Meilisearch",
  qdrant: "Qdrant",
  chroma: "ChromaDB",
  weaviate: "Weaviate",
  influxdb: "InfluxDB",
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
  if (!product) {
    const logo = engine === "elasticsearch" && profile === "opensearch" ? opensearch : LOGOS[engine];
    return <span
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={!title}
      title={title ?? engineLabel(engine, profile)}
      data-size={size}
      className={`tiq-engine-mark inline-flex shrink-0 items-center justify-center ${className}`}
      style={{ width: px, height: px }}
    ><img src={logo} alt="" draggable={false} width={px} height={px} /></span>;
  }
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
