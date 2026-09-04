import { autocompletion, completionKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { keywordCompletionSource, sql, StandardSQL } from "@codemirror/lang-sql";
import {
  bracketMatching,
  defaultHighlightStyle,
  HighlightStyle,
  syntaxHighlighting,
} from "@codemirror/language";
import {
  type Diagnostic as CodeMirrorDiagnostic,
  lintGutter,
  lintKeymap,
  setDiagnostics,
  setDiagnosticsEffect,
} from "@codemirror/lint";
import { searchKeymap } from "@codemirror/search";
import { Compartment, EditorState } from "@codemirror/state";
import { tags } from "@lezer/highlight";
import {
  EditorView,
  highlightActiveLine,
  keymap,
  lineNumbers,
  placeholder as viewPlaceholder,
} from "@codemirror/view";
import { useEffect, useRef } from "react";
import type { EffectiveTheme } from "../../app/preferences";
import type { SqlDiagnostic } from "../../lib/commands";
import { createCatalogCompletionSource, type SqlTable } from "./sqlCompletion";

const draculaHighlightStyle = HighlightStyle.define([
  { tag: [tags.keyword, tags.controlKeyword, tags.operatorKeyword], color: "#ff79c6" },
  { tag: [tags.name, tags.variableName, tags.propertyName], color: "#f8f8f2" },
  { tag: [tags.typeName, tags.className, tags.standard(tags.name)], color: "#8be9fd" },
  { tag: [tags.string, tags.special(tags.string)], color: "#f1fa8c" },
  { tag: [tags.number, tags.bool, tags.null], color: "#bd93f9" },
  {
    tag: [tags.comment, tags.lineComment, tags.blockComment],
    color: "#6272a4",
    fontStyle: "italic",
  },
  { tag: [tags.function(tags.name), tags.definition(tags.name)], color: "#50fa7b" },
  { tag: [tags.operator, tags.punctuation, tags.separator], color: "#ff79c6" },
  { tag: tags.invalid, color: "#ff5555" },
]);

const draculaTheme = EditorView.theme(
  {
    "&": { backgroundColor: "#282a36", color: "#f8f8f2" },
    ".cm-content": { caretColor: "#f8f8f2" },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: "#f8f8f2" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
      backgroundColor: "#44475a",
    },
    ".cm-activeLine": { backgroundColor: "#30323f" },
    ".cm-gutters": { backgroundColor: "#282a36", color: "#6272a4", borderRightColor: "#44475a" },
    ".cm-activeLineGutter": { backgroundColor: "#30323f", color: "#f8f8f2" },
    ".cm-placeholder": { color: "#6272a4" },
  },
  { dark: true },
);

function editorThemeExtensions(theme: EffectiveTheme) {
  return theme === "dark"
    ? [draculaTheme, syntaxHighlighting(draculaHighlightStyle)]
    : [syntaxHighlighting(defaultHighlightStyle)];
}

export function SqlEditor({
  value,
  onChange,
  tables = [],
  onRun,
  diagnostics = [],
  effectiveTheme = "light",
}: {
  value: string;
  onChange: (value: string) => void;
  tables?: SqlTable[];
  onRun?: (sql: string) => void;
  diagnostics?: SqlDiagnostic[];
  effectiveTheme?: EffectiveTheme;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const completionCompartment = useRef(new Compartment());
  const themeCompartment = useRef(new Compartment());
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
          lintGutter(),
          highlightActiveLine(),
          history(),
          bracketMatching(),
          themeCompartment.current.of(editorThemeExtensions(effectiveTheme)),
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
            ...lintKeymap,
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
          EditorState.transactionExtender.of((transaction) =>
            transaction.docChanged ? { effects: setDiagnosticsEffect.of([]) } : null,
          ),
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
      effects: themeCompartment.current.reconfigure(editorThemeExtensions(effectiveTheme)),
    });
  }, [effectiveTheme]);

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
    const safe: CodeMirrorDiagnostic[] = diagnostics
      .filter(
        (diagnostic) =>
          diagnostic.from != null &&
          diagnostic.to != null &&
          diagnostic.from >= 0 &&
          diagnostic.to <= view.state.doc.length &&
          diagnostic.from < diagnostic.to,
      )
      .map((diagnostic) => ({
        from: diagnostic.from!,
        to: diagnostic.to!,
        severity: diagnostic.severity,
        message: diagnostic.message,
        source: "DuckDB",
      }));
    view.dispatch(setDiagnostics(view.state, safe));
  }, [diagnostics]);

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
