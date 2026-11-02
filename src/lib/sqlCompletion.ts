import { CompletionContext, type CompletionSource, type CompletionSection } from "@codemirror/autocomplete";
import { keywordCompletionSource, schemaCompletionSource, type SQLDialect, type SQLNamespace } from "@codemirror/lang-sql";
import { syntaxTree } from "@codemirror/language";

const KEYWORDS: CompletionSection = { name: "SQL", rank: "dynamic" };
const COMMON = new Set(["SELECT", "FROM", "WHERE", "JOIN", "ON", "AS", "AND", "OR", "GROUP", "ORDER", "BY", "LIMIT", "INSERT", "UPDATE", "DELETE", "WITH"]);

/** Keep native SQL alias/path resolution and fuzzy matching. Only narrow the
 * candidate sources at clear syntax boundaries, without inventing a SQL parser. */
export function sqlCompletionSource(dialect: SQLDialect, schema?: SQLNamespace): CompletionSource {
  const names = schemaCompletionSource({ dialect, schema });
  const words = keywordCompletionSource(dialect, true, (label, type) => ({
    label, type, detail: type === "type" ? "Data type" : type === "keyword" ? "Keyword" : "Built-in",
    section: KEYWORDS, boost: COMMON.has(label) ? 10 : -5,
  }));
  return async (context) => {
    const tree = syntaxTree(context.state);
    const at = tree.resolveInner(context.pos, -1);
    if (/Comment|String|Bits|Bytes/.test(at.name)) return null;
    const word = context.matchBefore(/[\w$]+/);
    const from = word?.from ?? context.pos;
    let before = from;
    let previous;
    // Whitespace belongs to the parent syntax node, so step back to the last
    // real token. Comments between FROM/JOIN and a name do not change that context.
    do {
      while (before > 0 && /\s/.test(context.state.sliceDoc(before - 1, before))) before--;
      previous = tree.resolveInner(before, -1);
      if (!/Comment/.test(previous.name)) break;
      before = previous.from;
    } while (before > 0);
    const qualified = previous.name === "." || at.name === ".";
    const keyword = previous.name === "Keyword" ? context.state.sliceDoc(previous.from, previous.to).toUpperCase() : "";
    const tablePosition = /^(FROM|JOIN|UPDATE|INTO|TABLE)$/.test(keyword);
    if (!context.explicit && !qualified && !tablePosition && (word?.text.length ?? 0) < 2) return null;

    // A space after FROM/JOIN is useful even before the next identifier is typed.
    const lookup = tablePosition ? new CompletionContext(context.state, context.pos, true, context.view) : context;
    const schemaResult = await names(lookup);
    if (tablePosition && schemaResult) return {
      ...schemaResult,
      options: schemaResult.options.filter((o) => o.type !== "constant").map((o) => ({
        ...o,
        section: o.type === "namespace" ? { name: "Schemas", rank: 1 } : { name: "Tables & views", rank: 0 },
      })),
    };
    if (qualified || tablePosition || at.name === "QuotedIdentifier") return schemaResult;
    const keywordResult = await words(context);
    if (!schemaResult) return keywordResult;
    if (!keywordResult) return schemaResult;
    return { ...schemaResult, options: [...schemaResult.options, ...keywordResult.options] };
  };
}
