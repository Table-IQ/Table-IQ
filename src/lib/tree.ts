import type { SchemaModel } from "./commands";

/** Filter the schema tree by an object-name substring (case-insensitive).
 *  Empty query returns the model unchanged; schemas with no matches are dropped. */
export function filterSchema(model: SchemaModel, query: string): SchemaModel {
  const q = query.trim().toLowerCase();
  if (!q) return model;
  return {
    schemas: model.schemas
      .map((s) => ({
        ...s,
        objects: s.objects.filter((o) => o.name.toLowerCase().includes(q)),
      }))
      .filter((s) => s.objects.length > 0),
  };
}
