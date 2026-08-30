import { autocompletion, completionKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { sql } from "@codemirror/lang-sql";
import { bracketMatching, defaultHighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { searchKeymap } from "@codemirror/search";
import { Compartment, EditorState } from "@codemirror/state";
import {
  EditorView,
  highlightActiveLine,
  keymap,
  lineNumbers,
  placeholder as viewPlaceholder,
} from "@codemirror/view";
import { useEffect, useRef } from "react";
import { buildSqlCompletionSchema, type SqlTable } from "./sqlCompletion";

export function SqlEditor({
  value,
  onChange,
  tables = [],
  onRun,
}: {
  value: string;
  onChange: (value: string) => void;
  tables?: SqlTable[];
  onRun?: (sql: string) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const schemaCompartment = useRef(new Compartment());
  const onChangeRef = useRef(onChange);
  const onRunRef = useRef(onRun);
  useEffect(() => {
    onChangeRef.current = onChange;
    onRunRef.current = onRun;
  }, [onChange, onRun]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container || viewRef.current) return;

    const view = new EditorView({
      parent: container,
      state: EditorState.create({
        doc: value,
        extensions: [
          lineNumbers(),
          highlightActiveLine(),
          history(),
          bracketMatching(),
          syntaxHighlighting(defaultHighlightStyle),
          autocompletion(),
          keymap.of([
            {
              key: "Mod-Enter",
              run: (view) => {
                onRunRef.current?.(view.state.doc.toString());
                return true;
              },
            },
            ...defaultKeymap,
            ...completionKeymap,
            ...historyKeymap,
            ...searchKeymap,
            indentWithTab,
          ]),
          schemaCompartment.current.of(sql({ schema: buildSqlCompletionSchema(tables) })),
          viewPlaceholder("SELECT * FROM ..."),
          EditorState.tabSize.of(2),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) onChangeRef.current(update.state.doc.toString());
          }),
        ],
      }),
    });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: schemaCompartment.current.reconfigure(
        sql({ schema: buildSqlCompletionSchema(tables) }),
      ),
    });
  }, [tables]);

  return <div ref={containerRef} className="sql-codemirror" />;
}
