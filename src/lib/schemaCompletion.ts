import type { SchemaModel } from "./commands";

/**
 * Convert TableIQ's SchemaModel into the `{ [tableName]: string[] }` shape that
 * `@codemirror/lang-sql`'s `schemaCompletionSource` expects.
 *
 * Tables that appear in multiple schemas are merged on bare name (UI defaults to
 * `dotName: false` so users type `users.id`); the qualified `schema.table` form is
 * also exposed so `public.users.<col>` completion works.
 */
export function schemaModelToCompletionMap(model: SchemaModel | null | undefined): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  if (!model) return out;
  for (const s of model.schemas) {
    for (const o of s.objects) {
      const cols = o.columns.map((c) => c.name);
      out[o.name] = mergeCols(out[o.name], cols);
      out[`${s.name}.${o.name}`] = mergeCols(out[`${s.name}.${o.name}`], cols);
    }
  }
  return out;
}

function mergeCols(prev: string[] | undefined, next: string[]): string[] {
  if (!prev) return next;
  const seen = new Set(prev);
  const out = prev.slice();
  for (const c of next) {
    if (!seen.has(c)) {
      out.push(c);
      seen.add(c);
    }
  }
  return out;
}
