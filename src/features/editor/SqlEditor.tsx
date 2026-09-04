import { autocompletion, completionKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { keywordCompletionSource, sql, StandardSQL } from "@codemirror/lang-sql";
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
import { createCatalogCompletionSource, type SqlTable } from "./sqlCompletion";

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
  const completionCompartment = useRef(new Compartment());
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
          sql({ upperCaseKeywords: true }),
          completionCompartment.current.of(
            autocompletion({
              override: [
                createCatalogCompletionSource(tables),
                keywordCompletionSource(StandardSQL, true),
              ],
            }),
          ),
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
      effects: completionCompartment.current.reconfigure(
        autocompletion({
          override: [
            createCatalogCompletionSource(tables),
            keywordCompletionSource(StandardSQL, true),
          ],
        }),
      ),
    });
  }, [tables]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== value) {
      view.dispatch({ changes: { from: 0, to: current.length, insert: value } });
    }
  }, [value]);

  return <div ref={containerRef} className="sql-codemirror" />;
}
