import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { EditorView, basicSetup } from "codemirror";
import { Compartment, Prec } from "@codemirror/state";
import { sql, PostgreSQL, MySQL, MSSQL, SQLite, type SQLDialect } from "@codemirror/lang-sql";
import { keymap, placeholder } from "@codemirror/view";
import { syntaxHighlighting } from "@codemirror/language";
import { sqlHighlight } from "../lib/sqlHighlight";
import { loadEditorFontSize } from "../lib/prefs";
import type { Engine } from "../lib/commands";

export interface SqlEditorHandle {
  setText: (v: string) => void;
  getDoc: () => string;
  getSelection: () => { from: number; to: number };
  /** Move the cursor to `offset`, scrolling it into view. */
  gotoOffset: (offset: number) => void;
}

interface Props {
  initial: string;
  onChange: (v: string) => void;
  /** ⌘↵ — the parent decides selection vs the statement under the cursor
   *  (via the handle's `getSelection()`/`getDoc()`). */
  onRun: () => void;
  /** ⇧⌘↵ — run every statement in the document. */
  onRunAll?: () => void;
  /** Focus left the editor (e.g. a click on the connection rail). */
  onBlur?: () => void;
  /**
   * Schema for autocomplete. Maps `tableName` (and `schema.tableName`) to its
   * columns, the shape `@codemirror/lang-sql` expects. Reconfigured in place
   * (no editor remount) when this object identity changes.
   */
  schema?: Record<string, string[]>;
  /** Engine driving SQL-dialect highlighting/completion. Defaults to Postgres. */
  dialect?: Engine;
}

// ponytail: redis/mongodb don't use this SQL editor today (they get their own tab
// kinds); default unmapped/absent engines to Postgres rather than adding a fifth
// CodeMirror dialect nobody drives yet.
/** Settings' Editor section persists `tiq:editor:fontSize`; kept in its own
 *  compartment (not the static theme below) so it can be reconfigured live
 *  without recreating the EditorView. */
function fontSizeTheme(px: number) {
  return EditorView.theme({ ".cm-scroller": { fontSize: `${px}px` } });
}

function cmDialectFor(engine: Engine | undefined): SQLDialect {
  switch (engine) {
    // CodeMirror has no ClickHouse dialect: MySQL's lexing (backtick identifiers, `#` comments,
    // backslash escapes) is the closest.
    case "mysql":
    case "clickhouse":
      return MySQL;
    case "sqlserver":
      return MSSQL;
    case "sqlite":
    case "libsql":
    case "rqlite":
      return SQLite;
    default:
      return PostgreSQL;
  }
}

/** CodeMirror 6 SQL editor. Theme uses CSS vars so it adapts to light/dark. */
export const SqlEditor = forwardRef<SqlEditorHandle, Props>(function SqlEditor(
  { initial, onChange, onRun, onRunAll, onBlur, schema, dialect },
  ref,
) {
  const host = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onRunRef = useRef(onRun);
  const onChangeRef = useRef(onChange);
  const onRunAllRef = useRef(onRunAll);
  onRunRef.current = onRun;
  onChangeRef.current = onChange;
  onRunAllRef.current = onRunAll;
  const onBlurRef = useRef(onBlur);
  onBlurRef.current = onBlur;

  // Mirrored into refs so the mount-only effect below can read the values current
  // at creation time without depending on them (they're re-applied immediately
  // after by the reconfigure effect anyway).
  const schemaRef = useRef(schema);
  const dialectRef = useRef(dialect);
  schemaRef.current = schema;
  dialectRef.current = dialect;

  const sqlCompartment = useRef(new Compartment()).current;
  const fontSizeCompartment = useRef(new Compartment()).current;

  useImperativeHandle(ref, () => ({
    setText: (v: string) => {
      const view = viewRef.current;
      if (!view) return;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } });
      onChangeRef.current(v);
    },
    getDoc: () => viewRef.current?.state.doc.toString() ?? "",
    getSelection: () => {
      const sel = viewRef.current?.state.selection.main;
      return { from: sel?.from ?? 0, to: sel?.to ?? 0 };
    },
    gotoOffset: (offset: number) => {
      const view = viewRef.current;
      if (!view) return;
      view.dispatch({ selection: { anchor: offset }, scrollIntoView: true });
      view.focus();
    },
  }));

  // Create the EditorView once. Later schema/dialect changes reconfigure the
  // `sqlCompartment` in place (see below) instead of recreating the view, so
  // cursor position, undo history, and scroll offset survive.
  useEffect(() => {
    if (!host.current) return;
    const view = new EditorView({
      doc: initial,
      parent: host.current,
      extensions: [
        basicSetup,
        syntaxHighlighting(sqlHighlight),
        sqlCompartment.of(
          sql({ dialect: cmDialectFor(dialectRef.current), schema: schemaRef.current }),
        ),
        fontSizeCompartment.of(fontSizeTheme(loadEditorFontSize())),
        // `basicSetup`'s defaultKeymap binds Mod-Enter to insertBlankLine; without
        // Prec.highest that binding wins (same precedence, registered first) and
        // ⌘↵ inserts a newline instead of running the query.
        Prec.highest(
          keymap.of([
            {
              key: "Mod-Enter",
              run: () => {
                onRunRef.current();
                return true;
              },
            },
            {
              key: "Mod-Shift-Enter",
              run: () => {
                if (!onRunAllRef.current) return false;
                onRunAllRef.current();
                return true;
              },
            },
          ]),
        ),
        placeholder("Write SQL…  ⌘↵ run statement · ⇧⌘↵ run all"),
        EditorView.domEventHandlers({ blur: () => void onBlurRef.current?.() }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) onChangeRef.current(u.state.doc.toString());
        }),
        EditorView.theme({
          "&": { height: "100%", backgroundColor: "var(--bg)", color: "var(--syn-text)" },
          ".cm-scroller": { fontFamily: "var(--font-mono)" },
          ".cm-gutters": { backgroundColor: "var(--bg)", color: "var(--syn-gutter)", border: "none" },
          ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--primary) 6%, transparent)" },
          ".cm-activeLineGutter": { backgroundColor: "transparent" },
          ".cm-content": { caretColor: "var(--text)" },
          "&.cm-focused": { outline: "none" },
        }),
      ],
    });
    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Reconfigure the sql() language in place when schema or dialect changes —
  // does NOT recreate the EditorView (see the mount effect above).
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: sqlCompartment.reconfigure(sql({ dialect: cmDialectFor(dialect), schema })),
    });
  }, [schema, dialect, sqlCompartment]);

  // Settings' Editor section has no wiring to this specific editor instance —
  // it just persists a preference and fires "tiq:prefs" (a "storage" event
  // fires too, from other windows/tabs). Re-read on either so an already-open
  // editor updates without remounting.
  useEffect(() => {
    const applyFontSize = () => {
      const view = viewRef.current;
      if (!view) return;
      view.dispatch({
        effects: fontSizeCompartment.reconfigure(fontSizeTheme(loadEditorFontSize())),
      });
    };
    window.addEventListener("tiq:prefs", applyFontSize);
    window.addEventListener("storage", applyFontSize);
    return () => {
      window.removeEventListener("tiq:prefs", applyFontSize);
      window.removeEventListener("storage", applyFontSize);
    };
  }, [fontSizeCompartment]);

  return <div ref={host} className="h-full overflow-auto" />;
});
