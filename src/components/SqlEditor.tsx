import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { EditorView, basicSetup } from "codemirror";
import { sql, PostgreSQL } from "@codemirror/lang-sql";
import { keymap } from "@codemirror/view";
import { extractRunTarget } from "../lib/sqlSelection";

export interface SqlEditorHandle {
  setText: (v: string) => void;
}

interface Props {
  initial: string;
  onChange: (v: string) => void;
  onRun: () => void;
  /** ⇧⌘↵ — runs the active selection or the statement under the cursor. */
  onRunSelection?: (sql: string) => void;
  /**
   * Schema for autocomplete. Maps `tableName` (and `schema.tableName`) to its
   * columns, the shape `@codemirror/lang-sql` expects. Re-mount the editor when
   * this object identity changes (rare — on connect / introspect).
   */
  schema?: Record<string, string[]>;
}

/** CodeMirror 6 SQL editor. Theme uses CSS vars so it adapts to light/dark. */
export const SqlEditor = forwardRef<SqlEditorHandle, Props>(function SqlEditor(
  { initial, onChange, onRun, onRunSelection, schema },
  ref,
) {
  const host = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onRunRef = useRef(onRun);
  const onChangeRef = useRef(onChange);
  const onRunSelectionRef = useRef(onRunSelection);
  onRunRef.current = onRun;
  onChangeRef.current = onChange;
  onRunSelectionRef.current = onRunSelection;

  useImperativeHandle(ref, () => ({
    setText: (v: string) => {
      const view = viewRef.current;
      if (!view) return;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } });
      onChangeRef.current(v);
    },
  }));

  // Schema identity in the dep array (referential): when the host passes a new
  // object, we destroy + recreate the editor with refreshed completions.
  const schemaRef = useRef(schema);
  schemaRef.current = schema;

  useEffect(() => {
    if (!host.current) return;
    const view = new EditorView({
      doc: initial,
      parent: host.current,
      extensions: [
        basicSetup,
        sql({ dialect: PostgreSQL, schema: schemaRef.current }),
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
            run: (v) => {
              if (!onRunSelectionRef.current) return false;
              const sel = v.state.selection.main;
              const target = extractRunTarget(v.state.doc.toString(), sel.from, sel.to);
              if (target.sql.trim().length === 0) return false;
              onRunSelectionRef.current(target.sql);
              return true;
            },
          },
        ]),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) onChangeRef.current(u.state.doc.toString());
        }),
        EditorView.theme({
          "&": { height: "100%", backgroundColor: "var(--bg)", color: "var(--text)" },
          ".cm-scroller": { fontFamily: "var(--font-mono)", fontSize: "13px" },
          ".cm-gutters": { backgroundColor: "var(--surface)", color: "var(--muted)", border: "none" },
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
    // Recreate when schema identity changes so completions stay current.
  }, [schema]);

  return <div ref={host} className="h-full overflow-auto" />;
});
