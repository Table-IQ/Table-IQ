import type { Completion, CompletionSection } from "@codemirror/autocomplete";
import type { SQLNamespace } from "@codemirror/lang-sql";
import type { Engine, SchemaModel } from "./commands";
import { quoteIdent } from "./tableScript";

const TABLES: CompletionSection = { name: "Tables & views", rank: "dynamic" };
const COLUMNS: CompletionSection = { name: "Columns", rank: "dynamic" };
const SCHEMAS: CompletionSection = { name: "Schemas", rank: "dynamic" };
// CodeMirror interprets dots in namespace keys as path separators.
const key = (name: string) => name.replace(/\./g, "\\.");

/** Schema, table and column metadata for CodeMirror's native SQL completion source.
 * Unique table names are also available without a schema prefix. Ambiguous names
 * stay under their own schema, so columns from different tables are never merged.
 * Completion labels remain readable; insertion quotes each actual identifier. */
export function schemaModelToCompletionMap(model: SchemaModel | null | undefined, engine?: Engine): SQLNamespace {
  const out: Record<string, SQLNamespace> = Object.create(null);
  if (!model) return out;
  const counts = new Map<string, number>();
  const schemas = new Set(model.schemas.map((s) => s.name));
  for (const s of model.schemas) for (const o of s.objects) counts.set(o.name, (counts.get(o.name) ?? 0) + 1);
  for (const s of model.schemas) {
    const tables: Record<string, SQLNamespace> = Object.create(null);
    for (const o of s.objects) {
      const columns: Completion[] = o.columns.map((c) => ({
        label: c.name, type: "property", detail: c.data_type,
        apply: quoteIdent(c.name, engine), section: COLUMNS,
      }));
      const table = {
        self: { label: o.name, type: "type", detail: `${s.name} · ${o.kind.toLowerCase()}`,
          apply: quoteIdent(o.name, engine), section: TABLES },
        children: columns,
      };
      tables[key(o.name)] = table;
      if (counts.get(o.name) === 1 && !schemas.has(o.name)) out[key(o.name)] = table;
    }
    out[key(s.name)] = {
      self: { label: s.name, type: "namespace", detail: "Schema", apply: quoteIdent(s.name, engine), section: SCHEMAS },
      children: tables,
    };
  }
  return out;
}
