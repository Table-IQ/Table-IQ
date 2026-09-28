import { HighlightStyle } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

/**
 * SQL syntax colours, Xcode-style like TablePlus: keywords recede in a soft blue
 * while identifiers stay at full body contrast. Colours are CSS vars (`--syn-*`
 * in index.css) so light/dark swap with the theme — the editor is DOM, not canvas.
 * Replaces basicSetup's `defaultHighlightStyle`, which is light-only.
 */
export const sqlHighlight = HighlightStyle.define([
  { tag: [t.keyword, t.typeName, t.operatorKeyword], color: "var(--syn-keyword)" },
  { tag: t.string, color: "var(--syn-string)" },
  // lang-sql tags quoted identifiers ("Users", `order`) as special(string) — a
  // sub-tag of string — so pin them back to body contrast: names carry the eye.
  { tag: t.special(t.string), color: "var(--syn-text)" },
  { tag: [t.lineComment, t.blockComment], color: "var(--syn-comment)" },
  { tag: [t.number, t.bool, t.null], color: "var(--syn-number)" },
]);
