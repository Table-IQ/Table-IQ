import { describe, expect, it } from "vitest";
import { ENGINE_COLORS, ENGINE_MONOGRAMS } from "../components/ui/EngineIcon";
import cases from "./agentAccessCases.json";
import { AGENT_REV, MONGODB_AGENT_SINCE, PROFILES, REDIS_AGENT_SINCE, applyPreset, capsOf, explainUnavailable, presetFor, profileForUrl, profileOf, profilesFor } from "./profiles";

/** WCAG relative luminance of a `#rrggbb` colour. */
function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
const contrast = (a: string, b: string) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};

const entries = Object.entries(PROFILES);

describe("profile table", () => {
  it("every entry has every key with the right type", () => {
    for (const [id, p] of entries) {
      expect(["postgres", "mysql"], id).toContain(p.base);
      for (const k of ["label", "monogram", "color"] as const) expect(typeof p[k], `${id}.${k}`).toBe("string");
      expect(Number.isInteger(p.port), id).toBe(true);
      for (const k of ["real", "transactions", "backup", "userMgmt", "tested"] as const) expect(typeof p[k], `${id}.${k}`).toBe("boolean");
      for (const k of ["schemes", "hosts", "detect"] as const) expect(Array.isArray(p[k]), `${id}.${k}`).toBe(true);
      for (const k of ["user", "database", "ssl"] as const) expect(["undefined", "string"], `${id}.${k}`).toContain(typeof p[k]);
      expect(["undefined", "boolean"], id).toContain(typeof p.explain);
      expect(p.monogram, id).toHaveLength(2);
      expect(p.color, id).toMatch(/^#[0-9a-f]{6}$/);
    }
  });

  it("monograms and schemes are unique, monograms including the seven base engines", () => {
    const monos = [...Object.values(ENGINE_MONOGRAMS), ...entries.map(([, p]) => p.monogram)];
    expect(new Set(monos).size).toBe(monos.length);
    const schemes = entries.flatMap(([, p]) => p.schemes);
    expect(new Set(schemes).size).toBe(schemes.length);
  });

  it("fills are distinct, legible under white text, and visible on the dark surface", () => {
    const fills = [...Object.values(ENGINE_COLORS), ...entries.map(([, p]) => p.color)];
    expect(new Set(fills).size).toBe(fills.length);
    for (const [id, p] of entries) {
      expect(contrast("#ffffff", p.color), `${id} under white`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(p.color, "#15171c"), `${id} on dark`).toBeGreaterThanOrEqual(2);
    }
  });
});

describe("profileOf / profilesFor", () => {
  it("is null for no profile, an unknown id, or another base", () => {
    expect(profileOf("postgres")).toBeNull();
    expect(profileOf("postgres", null)).toBeNull();
    expect(profileOf("postgres", "from-the-future")).toBeNull();
    expect(profileOf("postgres", "tidb")).toBeNull();
    expect(profileOf("sqlite", "tidb")).toBeNull();
    expect(profileOf("mysql", "tidb")?.label).toBe("TiDB");
  });

  it("lists a base's profiles, tested first, then by label", () => {
    const ids = profilesFor("postgres").map(([id]) => id);
    expect(ids).toContain("cockroachdb");
    expect(ids).not.toContain("tidb");
    const ps = profilesFor("mysql").map(([, p]) => p);
    const firstUntested = ps.findIndex((p) => !p.tested);
    expect(firstUntested).toBeGreaterThan(0);
    expect(ps.slice(firstUntested).every((p) => !p.tested)).toBe(true);
    for (const group of [ps.slice(0, firstUntested), ps.slice(firstUntested)]) {
      const labels = group.map((p) => p.label);
      expect(labels).toEqual([...labels].sort((a, b) => a.localeCompare(b)));
    }
  });
});

describe("profileForUrl", () => {
  it("reads the scheme first, then a host suffix on the matching base", () => {
    expect(profileForUrl("cockroachdb", "h", null)).toBe("cockroachdb");
    expect(profileForUrl("postgres", "ep-a.eu-central-1.aws.neon.tech", "postgres")).toBe("neon");
    expect(profileForUrl("mysql", "aws.connect.psdb.cloud", "mysql")).toBe("planetscale");
    expect(profileForUrl("postgres", "x.psdb.cloud", "postgres")).toBeNull();
    expect(profileForUrl("postgres", "localhost", "postgres")).toBeNull();
    expect(profileForUrl("postgres", "evilneon.tech", "postgres")).toBeNull();
  });

  it("maps a managed cloud host to its preset on the matching base only", () => {
    expect(profileForUrl("postgres", "d.x.us-east-1.rds.amazonaws.com", "postgres")).toBe("rds-postgres");
    expect(profileForUrl("mysql", "d.x.us-east-1.rds.amazonaws.com", "mysql")).toBe("rds-mysql");
    expect(profileForUrl("postgres", "s.postgres.database.azure.com", "postgres")).toBe("azure-postgres");
    expect(profileForUrl("mysql", "s.mysql.database.azure.com", "mysql")).toBe("azure-mysql");
    expect(profileForUrl("postgres", "s.mysql.database.azure.com", "postgres")).toBeNull();
    // Redshift keeps its own, longer suffix.
    expect(profileForUrl("postgres", "c.x.redshift.amazonaws.com", "postgres")).toBe("redshift");
  });

  it("every cloud preset is the base engine itself: agents may reach it, TLS is on", () => {
    for (const id of ["rds-postgres", "rds-mysql", "azure-postgres", "azure-mysql"]) {
      expect(PROFILES[id], id).toMatchObject({ real: true, tested: false, ssl: "require", detect: [] });
      expect(capsOf(PROFILES[id].base, id).agents, id).toBe(true);
    }
    for (const id of ["cloudsql-postgres", "cloudsql-mysql"]) {
      expect(PROFILES[id], id).toMatchObject({ real: true, tested: false, hosts: [], detect: [] });
    }
  });
});

describe("explainUnavailable", () => {
  it("names a product whose soft check documents Explain as failing (docs/ENGINES.md)", () => {
    expect(explainUnavailable("postgres", "cockroachdb")).toBe("Explain isn't available on CockroachDB yet.");
  });

  it("is null for the base engines, products that explain, and unknown or foreign ids", () => {
    expect(explainUnavailable("postgres")).toBeNull();
    expect(explainUnavailable("postgres", null)).toBeNull();
    expect(explainUnavailable("postgres", "yugabytedb")).toBeNull();
    // MySQL products explain as a result set, not through the JSON plan the soft check runs.
    expect(explainUnavailable("mysql", "tidb")).toBeNull();
    expect(explainUnavailable("mysql", "cockroachdb")).toBeNull();
    expect(explainUnavailable("postgres", "from-the-future")).toBeNull();
  });
});

describe("agent access", () => {
  it("follows `real` unless a profile sets `agent`, and only CockroachDB does", () => {
    const differing = Object.entries(PROFILES).filter(([, p]) => (p.agent ?? p.real) !== p.real).map(([id]) => id);
    expect(differing).toEqual(["cockroachdb"]);
    expect(PROFILES.cockroachdb.real).toBe(false);
  });

  it("a compatible product open to agents dates its opening, which AGENT_REV is the latest of", () => {
    for (const [id, p] of Object.entries(PROFILES)) {
      expect((p.agent ?? p.real) && !p.real, id).toBe((p.agentSince ?? 0) > 0);
    }
    expect(AGENT_REV).toBe(REDIS_AGENT_SINCE);
    expect(MONGODB_AGENT_SINCE).toBeGreaterThan(PROFILES.cockroachdb.agentSince ?? 0);
    expect(REDIS_AGENT_SINCE).toBeGreaterThan(MONGODB_AGENT_SINCE);
  });
});

describe("capsOf", () => {
  it("is all true for a base engine, from the profile's flags otherwise", () => {
    expect(capsOf("postgres")).toEqual({ transactions: true, backup: true, userMgmt: true, agents: true });
    expect(capsOf("mysql", "doris")).toEqual({ transactions: false, backup: false, userMgmt: false, agents: false });
    expect(capsOf("postgres", "neon").agents).toBe(true);
    expect(capsOf("mysql", "tidb").agents).toBe(false);
    expect(capsOf("postgres", "cockroachdb").agents).toBe(true);
    expect(capsOf("postgres", "cockroachdb").backup).toBe(false); // agent access is its own flag
  });

  it("treats an unknown profile as the base engine, without agents", () => {
    expect(capsOf("mysql", "from-the-future")).toEqual({ transactions: true, backup: true, userMgmt: true, agents: false });
  });

  it("ClickHouse is a read-only browser: no transactions, backup, users or agents", () => {
    expect(capsOf("clickhouse")).toEqual({ transactions: false, backup: false, userMgmt: false, agents: false });
    expect(presetFor("clickhouse", null)).toEqual({ port: 8123, username: "default", database: "", sslmode: "prefer" });
  });

  it("agrees with the shared agent-access case table", () => {
    for (const c of cases) expect(capsOf(c.engine, c.profile).agents, `${c.engine}/${c.profile}`).toBe(c.allowed);
  });
});

describe("presets", () => {
  const blank = { port: 5432, username: "postgres", database: "", sslmode: "prefer" };

  it("presetFor: base engine defaults, or the profile's own", () => {
    expect(presetFor("postgres", null)).toEqual(blank);
    expect(presetFor("mysql", null)).toEqual({ port: 3306, username: "", database: "", sslmode: "prefer" });
    expect(presetFor("postgres", "cockroachdb")).toEqual({ port: 26257, username: "root", database: "defaultdb", sslmode: "prefer" });
    expect(presetFor("postgres", "neon").sslmode).toBe("require");
  });

  it("switching profiles and back leaves the blank form as it was", () => {
    let form = { ...blank };
    let prev = presetFor("postgres", null);
    for (const p of ["cockroachdb", "neon", null]) {
      const next = presetFor("postgres", p);
      form = applyPreset(form, prev, next);
      prev = next;
    }
    expect(form).toEqual(blank);
  });

  it("keeps what the user typed", () => {
    const form = applyPreset({ ...blank, username: "alice" }, presetFor("postgres", null), presetFor("postgres", "cockroachdb"));
    expect(form).toMatchObject({ username: "alice", port: 26257, database: "defaultdb" });
  });
});

describe("Turso / libSQL", () => {
  it("is a read-only browser: no transactions, backup, users or agents, and a port its URL carries", () => {
    expect(capsOf("libsql")).toEqual({ transactions: false, backup: false, userMgmt: false, agents: false });
    expect(presetFor("libsql", null)).toEqual({ port: 8080, username: "", database: "", sslmode: "prefer" });
  });
  it("draws a teal fill that is legible under its white monogram and visible on the dark surface", () => {
    expect(ENGINE_MONOGRAMS.libsql).toBe("Tu");
    expect(contrast("#ffffff", ENGINE_COLORS.libsql)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(ENGINE_COLORS.libsql, "#15171c")).toBeGreaterThanOrEqual(2);
  });
});

describe("Elasticsearch", () => {
  it("is a read-only browser: no transactions, backup, users or agents, and port 9200", () => {
    expect(capsOf("elasticsearch")).toEqual({ transactions: false, backup: false, userMgmt: false, agents: false });
    expect(presetFor("elasticsearch", null)).toEqual({ port: 9200, username: "", database: "", sslmode: "prefer" });
  });
  it("draws a blue fill that is legible under its white monogram and visible on the dark surface", () => {
    expect(ENGINE_MONOGRAMS.elasticsearch).toBe("Es");
    expect(contrast("#ffffff", ENGINE_COLORS.elasticsearch)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(ENGINE_COLORS.elasticsearch, "#15171c")).toBeGreaterThanOrEqual(2);
  });
});

describe("rqlite", () => {
  it("is a read-only browser: no transactions, backup, users or agents, and port 4001", () => {
    expect(capsOf("rqlite")).toEqual({ transactions: false, backup: false, userMgmt: false, agents: false });
    expect(presetFor("rqlite", null)).toEqual({ port: 4001, username: "", database: "", sslmode: "prefer" });
  });
  it("draws a rose fill that is legible under its white monogram and visible on the dark surface", () => {
    expect(ENGINE_MONOGRAMS.rqlite).toBe("Rq");
    expect(contrast("#ffffff", ENGINE_COLORS.rqlite)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(ENGINE_COLORS.rqlite, "#15171c")).toBeGreaterThanOrEqual(2);
  });
});

describe("Meilisearch", () => {
  it("is a read-only browser: no transactions, backup, users or agents, and port 7700", () => {
    expect(capsOf("meilisearch")).toEqual({ transactions: false, backup: false, userMgmt: false, agents: false });
    expect(presetFor("meilisearch", null)).toEqual({ port: 7700, username: "", database: "", sslmode: "prefer" });
  });
  it("draws a violet fill that is legible under its white monogram and visible on the dark surface", () => {
    expect(ENGINE_MONOGRAMS.meilisearch).toBe("Me");
    expect(contrast("#ffffff", ENGINE_COLORS.meilisearch)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(ENGINE_COLORS.meilisearch, "#15171c")).toBeGreaterThanOrEqual(2);
  });
});

describe("InfluxDB", () => {
  it("is a read-only browser: no transactions, backup, users or agents, and port 8181", () => {
    expect(capsOf("influxdb")).toEqual({ transactions: false, backup: false, userMgmt: false, agents: false });
    expect(presetFor("influxdb", null)).toEqual({ port: 8181, username: "", database: "", sslmode: "prefer" });
  });
  it("draws an indigo fill that is legible under its white monogram and visible on the dark surface", () => {
    expect(ENGINE_MONOGRAMS.influxdb).toBe("In");
    expect(contrast("#ffffff", ENGINE_COLORS.influxdb)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(ENGINE_COLORS.influxdb, "#15171c")).toBeGreaterThanOrEqual(2);
  });
});

describe("ChromaDB", () => {
  it("is a read-only browser: no transactions, backup, users or agents, and port 8000", () => {
    expect(capsOf("chroma")).toEqual({ transactions: false, backup: false, userMgmt: false, agents: false });
    expect(presetFor("chroma", null)).toEqual({ port: 8000, username: "", database: "", sslmode: "prefer" });
  });
  it("draws an orange fill that is legible under its white monogram and visible on the dark surface", () => {
    expect(ENGINE_MONOGRAMS.chroma).toBe("Cd");
    expect(contrast("#ffffff", ENGINE_COLORS.chroma)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(ENGINE_COLORS.chroma, "#15171c")).toBeGreaterThanOrEqual(2);
  });
});

describe("Weaviate", () => {
  it("is a read-only browser: no transactions, backup, users or agents, and port 8080", () => {
    expect(capsOf("weaviate")).toEqual({ transactions: false, backup: false, userMgmt: false, agents: false });
    expect(presetFor("weaviate", null)).toEqual({ port: 8080, username: "", database: "", sslmode: "prefer" });
  });
  it("draws an olive fill that is legible under its white monogram and visible on the dark surface", () => {
    expect(ENGINE_MONOGRAMS.weaviate).toBe("We");
    expect(contrast("#ffffff", ENGINE_COLORS.weaviate)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(ENGINE_COLORS.weaviate, "#15171c")).toBeGreaterThanOrEqual(2);
  });
});

describe("Qdrant", () => {
  it("is a read-only browser: no transactions, backup, users or agents, and port 6333", () => {
    expect(capsOf("qdrant")).toEqual({ transactions: false, backup: false, userMgmt: false, agents: false });
    expect(presetFor("qdrant", null)).toEqual({ port: 6333, username: "", database: "", sslmode: "prefer" });
  });
  it("draws a red fill that is legible under its white monogram and visible on the dark surface", () => {
    expect(ENGINE_MONOGRAMS.qdrant).toBe("Qr");
    expect(contrast("#ffffff", ENGINE_COLORS.qdrant)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(ENGINE_COLORS.qdrant, "#15171c")).toBeGreaterThanOrEqual(2);
  });
});
