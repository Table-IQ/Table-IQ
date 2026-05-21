import { invoke } from "@tauri-apps/api/core";
import { synthWindowIpc, MOCK_TOTAL_ROWS } from "./mockSource";

// ── shared types (mirror the Rust serde shapes) ──────────────────────────
export interface ConnConfig {
  id: string;
  name: string;
  color?: string | null;
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
