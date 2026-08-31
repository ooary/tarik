import { autocompletion, completionKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { sql } from "@codemirror/lang-sql";
import { bracketMatching, defaultHighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, StateEffect, StateField } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  highlightActiveLine,
  type DecorationSet,
  keymap,
  lineNumbers,
  placeholder as viewPlaceholder,
} from "@codemirror/view";
import { useEffect, useRef } from "react";
import type { SqlRange } from "../query-flow/sqlMapping";
import { buildSqlCompletionSchema, type SqlTable } from "./sqlCompletion";

const setPlanHighlight = StateEffect.define<SqlRange | null>();
const planHighlightField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update: (decorations, transaction) => {
    decorations = decorations.map(transaction.changes);
    for (const effect of transaction.effects) {
      if (effect.is(setPlanHighlight)) {
        const range = effect.value;
        decorations = range
          ? Decoration.set([
              Decoration.mark({ class: "cm-plan-highlight" }).range(range.from, range.to),
            ])
          : Decoration.none;
      }
    }
    return decorations;
  },
  provide: (field) => EditorView.decorations.from(field),
});

export function SqlEditor({
  value,
  onChange,
  tables = [],
  onRun,
  highlightRange = null,
}: {
  value: string;
  onChange: (value: string) => void;
  tables?: SqlTable[];
  onRun?: (sql: string) => void;
  highlightRange?: SqlRange | null;
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
          planHighlightField,
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

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== value) {
      view.dispatch({ changes: { from: 0, to: current.length, insert: value } });
    }
  }, [value]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const safeRange =
      highlightRange && highlightRange.from >= 0 && highlightRange.to <= view.state.doc.length
        ? highlightRange
        : null;
    view.dispatch({ effects: setPlanHighlight.of(safeRange) });
    if (safeRange) view.dispatch({ selection: { anchor: safeRange.from, head: safeRange.to } });
  }, [highlightRange]);

  return <div ref={containerRef} className="sql-codemirror" />;
}
