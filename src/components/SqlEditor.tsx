import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { EditorView, basicSetup } from "codemirror";
import { sql, PostgreSQL } from "@codemirror/lang-sql";
import { keymap } from "@codemirror/view";

export interface SqlEditorHandle {
  setText: (v: string) => void;
}

interface Props {
  initial: string;
  onChange: (v: string) => void;
  onRun: () => void;
}

/** CodeMirror 6 SQL editor. Theme uses CSS vars so it adapts to light/dark. */
export const SqlEditor = forwardRef<SqlEditorHandle, Props>(function SqlEditor(
  { initial, onChange, onRun },
  ref,
) {
  const host = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onRunRef = useRef(onRun);
  const onChangeRef = useRef(onChange);
  onRunRef.current = onRun;
  onChangeRef.current = onChange;

  useImperativeHandle(ref, () => ({
    setText: (v: string) => {
      const view = viewRef.current;
      if (!view) return;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } });
      onChangeRef.current(v);
    },
  }));

  useEffect(() => {
    if (!host.current) return;
    const view = new EditorView({
      doc: initial,
      parent: host.current,
      extensions: [
        basicSetup,
        sql({ dialect: PostgreSQL }),
        keymap.of([
          {
            key: "Mod-Enter",
            run: () => {
              onRunRef.current();
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
    // create once; editor is uncontrolled after mount
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <div ref={host} className="h-full overflow-auto" />;
});
