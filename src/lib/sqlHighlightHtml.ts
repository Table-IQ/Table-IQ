import { highlightCode } from "@lezer/highlight";
import { StandardSQL } from "@codemirror/lang-sql";
import { sqlHighlight } from "./sqlHighlight";

/** SQL → highlighted HTML outside a CodeMirror editor. Shared by the SQL log panel and the
 *  lobby's recent queries, both of which inject the result with `dangerouslySetInnerHTML`. */

let highlightCssInjected = false;
/**
 * `sqlHighlight` is a `HighlightStyle` built for a CodeMirror `EditorView`,
 * which injects its generated CSS into the document itself. We drive
 * `highlightCode` directly (no editor instance), so nothing does that for
 * us — mount the same CSS text once, by hand.
 */
export function ensureHighlightCss(): void {
  if (highlightCssInjected || typeof document === "undefined") return;
  const css = sqlHighlight.module?.getRules();
  if (!css) return;
  highlightCssInjected = true;
  const style = document.createElement("style");
  style.setAttribute("data-tiq", "sql-log-highlight");
  style.textContent = css;
  document.head.appendChild(style);
}

/** Pure string transform. Output only ever lands in HTML text content, never an attribute. */
export function escapeHtml(s: string): string {
  return s.replace(/[&<>]/g, (c) => (c === "&" ? "&amp;" : c === "<" ? "&lt;" : "&gt;"));
}

/** Highlight one SQL statement to an HTML string using the editor's own
 * `sqlHighlight` classes/colours (dialect-agnostic `StandardSQL`, since a
 * statement can come from any connected engine). Every text run is escaped.
 *
 * `ensureHighlightCss` no-ops when `document` is undefined (no jsdom in this
 * project), so this whole function is safe to call from a plain Node test. */
export function highlightSql(sqlText: string): string {
  ensureHighlightCss();
  const tree = StandardSQL.language.parser.parse(sqlText);
  let html = "";
  highlightCode(
    sqlText,
    tree,
    sqlHighlight,
    (text, classes) => {
      const escaped = escapeHtml(text);
      html += classes ? `<span class="${classes}">${escaped}</span>` : escaped;
    },
    () => {
      html += "\n";
    },
  );
  return html;
}
