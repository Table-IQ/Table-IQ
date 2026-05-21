import { invoke } from "@tauri-apps/api/core";
import { synthWindowIpc, MOCK_TOTAL_ROWS } from "./mockSource";

// ── shared types (mirror the Rust serde shapes) ──────────────────────────
export type Engine = "postgres" | "mysql" | "sqlite";
export interface ConnConfig {
  id: string;
  name: string;
  color?: string | null;
  engine: Engine;
  host: string;
  port: number;
  database: string;
  username: string;
  sslmode: string;
}
export interface ColumnInfo {
  name: string;
  data_type: string;
  nullable: boolean;
}
export type ObjectKind = "Table" | "View";
export interface ObjectInfo {
  name: string;
  kind: ObjectKind;
  columns: ColumnInfo[];
}
export interface SchemaInfo {
  name: string;
  objects: ObjectInfo[];
}
export interface SchemaModel {
  schemas: SchemaInfo[];
}
export interface ConnectedInfo {
  id: string;
  name: string;
  database: string;
}

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

// ── browser-mode mock backend (lets /browse exercise the full flow) ──────
const mockStore: ConnConfig[] = [
  {
    id: "mock-local",
    name: "Local Postgres",
    color: "#4f46e5",
    engine: "postgres",
    host: "127.0.0.1",
    port: 55432,
    database: "tableiq",
    username: "postgres",
    sslmode: "prefer",
  },
];
const mockSchema: SchemaModel = {
  schemas: [
    {
      name: "public",
      objects: [
        {
          name: "wide",
          kind: "Table",
          columns: [
            { name: "id", data_type: "bigint", nullable: false },
            { name: "amount_numeric", data_type: "numeric", nullable: false },
            { name: "created_at", data_type: "timestamptz", nullable: false },
            { name: "blob_text", data_type: "text", nullable: false },
            { name: "nullable_int", data_type: "bigint", nullable: true },
          ],
        },
        { name: "users", kind: "Table", columns: [{ name: "id", data_type: "bigint", nullable: false }] },
        { name: "active_users", kind: "View", columns: [{ name: "id", data_type: "bigint", nullable: false }] },
      ],
    },
  ],
};

// ── API ──────────────────────────────────────────────────────────────────
export async function listConnections(): Promise<ConnConfig[]> {
  if (!inTauri()) return [...mockStore];
  return invoke<ConnConfig[]>("list_connections");
}

export async function saveConnection(cfg: ConnConfig, password?: string): Promise<void> {
  if (!inTauri()) {
    const i = mockStore.findIndex((c) => c.id === cfg.id);
    if (i >= 0) mockStore[i] = cfg;
    else mockStore.push(cfg);
    return;
  }
  await invoke("save_connection", { cfg, password: password ?? null });
}

export async function deleteConnection(id: string): Promise<void> {
  if (!inTauri()) {
    const i = mockStore.findIndex((c) => c.id === id);
    if (i >= 0) mockStore.splice(i, 1);
    return;
  }
  await invoke("delete_connection", { id });
}

export async function testConnection(cfg: ConnConfig, password: string): Promise<void> {
  if (!inTauri()) return; // mock: always ok
  await invoke("test_connection", { cfg, password });
}

export async function connect(id: string): Promise<ConnectedInfo> {
  if (!inTauri()) {
    const c = mockStore.find((x) => x.id === id) ?? mockStore[0];
    return { id: c.id, name: c.name, database: c.database };
  }
  return invoke<ConnectedInfo>("connect", { id });
}

export async function disconnect(id: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("disconnect", { id });
}

export async function listObjects(id: string): Promise<SchemaModel> {
  if (!inTauri()) return mockSchema;
  return invoke<SchemaModel>("list_objects", { id });
}

export async function openTable(
  id: string,
  schema: string,
  table: string,
): Promise<{ rowEstimate: number }> {
  if (!inTauri()) return { rowEstimate: MOCK_TOTAL_ROWS };
  const info = await invoke<{ row_estimate: number }>("open_table", { id, schema, table });
  return { rowEstimate: info.row_estimate };
}

export async function fetchTableWindow(
  id: string,
  schema: string,
  table: string,
  offset: number,
  len: number,
): Promise<{ bytes: Uint8Array; ms: number }> {
  const t0 = performance.now();
  if (!inTauri()) {
    return { bytes: synthWindowIpc(offset, len), ms: performance.now() - t0 };
  }
  const ab = await invoke<ArrayBuffer>("get_table_window", { id, schema, table, offset, len });
  return { bytes: new Uint8Array(ab), ms: performance.now() - t0 };
}

// ── query & edit ─────────────────────────────────────────────────────────
export interface QueryResultMeta {
  resultId: string;
  columns: string[];
  rowCount: number;
  truncated: boolean;
  execMs: number;
}

/** RowKey round-trip payload (mirrors Rust EditSpec). */
export interface EditSpec {
  pk: [string, string | null][];
  column: string;
  new_value: string | null;
  orig_value: string | null;
}

export async function runQuery(id: string, sql: string): Promise<QueryResultMeta> {
  if (!inTauri()) {
    return {
      resultId: "mock-q",
      columns: ["id", "amount_numeric", "blob_text"],
      rowCount: 50,
      truncated: false,
      execMs: 3,
    };
  }
  const m = await invoke<{
    result_id: string;
    columns: string[];
    row_count: number;
    truncated: boolean;
    exec_ms: number;
  }>("run_query", { id, sql });
  return {
    resultId: m.result_id,
    columns: m.columns,
    rowCount: m.row_count,
    truncated: m.truncated,
    execMs: m.exec_ms,
  };
}

export async function fetchResultWindow(
  resultId: string,
  offset: number,
  len: number,
): Promise<{ bytes: Uint8Array; ms: number }> {
  const t0 = performance.now();
  if (!inTauri()) {
    return { bytes: synthWindowIpc(offset, len), ms: performance.now() - t0 };
  }
  const ab = await invoke<ArrayBuffer>("get_result_window", { resultId, offset, len });
  return { bytes: new Uint8Array(ab), ms: performance.now() - t0 };
}

export async function primaryKey(id: string, schema: string, table: string): Promise<string[]> {
  if (!inTauri()) return ["id"];
  return invoke<string[]>("primary_key", { id, schema, table });
}

export async function applyEdit(
  id: string,
  schema: string,
  table: string,
  edit: EditSpec,
): Promise<{ rowsAffected: number }> {
  if (!inTauri()) return { rowsAffected: 1 };
  const r = await invoke<{ rows_affected: number }>("apply_edit", { id, schema, table, edit });
  return { rowsAffected: r.rows_affected };
}

// ── export & transactions ─────────────────────────────────────────────────
export type ExportFormat = "csv" | "json";

export async function exportResult(resultId: string, format: ExportFormat): Promise<string> {
  if (!inTauri()) return format === "json" ? '[{"id":"1","demo":"true"}]' : "id,demo\n1,true\n";
  return invoke<string>("export_result", { resultId, format });
}

export async function exportTable(
  id: string,
  schema: string,
  table: string,
  format: ExportFormat,
): Promise<string> {
  if (!inTauri()) return format === "json" ? '[{"id":"1"}]' : "id\n1\n";
  return invoke<string>("export_table", { id, schema, table, format });
}

export async function runScript(id: string, sql: string): Promise<{ rowsAffected: number }> {
  if (!inTauri()) return { rowsAffected: 0 };
  const r = await invoke<{ rows_affected: number }>("run_script", { id, sql });
  return { rowsAffected: r.rows_affected };
}

export async function explainQuery(id: string, sql: string): Promise<string> {
  if (!inTauri())
    return JSON.stringify([
      {
        Plan: {
          "Node Type": "Limit",
          "Total Cost": 12.5,
          "Plan Rows": 10,
          Plans: [
            {
              "Node Type": "Seq Scan",
              "Total Cost": 1840.0,
              "Plan Rows": 1000000,
              "Relation Name": "wide",
            },
          ],
        },
      },
    ]);
  return invoke<string>("explain_query", { id, sql });
}

// ── AI (BYOK NL→SQL) ───────────────────────────────────────────────────────
export interface AiConfig {
  provider: string;
  model: string;
}

export async function getAiConfig(): Promise<AiConfig> {
  if (!inTauri()) return { provider: "anthropic", model: "claude-sonnet-4-6" };
  return invoke<AiConfig>("get_ai_config");
}

export async function saveAiConfig(provider: string, model: string, apiKey?: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("save_ai_config", { provider, model, apiKey: apiKey ?? null });
}

export async function aiEgressPreview(id: string): Promise<string> {
  if (!inTauri())
    return "public.wide (id bigint NOT NULL, amount_numeric numeric NOT NULL, blob_text text NOT NULL, nullable_int bigint)";
  return invoke<string>("ai_egress_preview", { id });
}

export async function aiGenerateSql(id: string, prompt: string): Promise<string> {
  if (!inTauri()) return "SELECT id, amount_numeric\nFROM wide\nORDER BY amount_numeric DESC\nLIMIT 10;";
  return invoke<string>("ai_generate_sql", { id, prompt });
}

/** Save text to a file (Blob download — works in the WebView and the browser). */
export function downloadText(filename: string, text: string, mime = "text/plain") {
  const blob = new Blob([text], { type: mime });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = filename;
  a.click();
  URL.revokeObjectURL(a.href);
}
