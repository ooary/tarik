import { ArrowClockwiseIcon, XIcon } from "@phosphor-icons/react";
import * as Dialog from "@radix-ui/react-dialog";
import { useMemo, useState } from "react";
import type { PlanMode, PlanNode } from "../../lib/commands";
import { NodeInspector } from "./NodeInspector";
import { QueryFlow } from "./QueryFlow";
import { mapPlanNodeToSql, type SqlRange } from "./sqlMapping";
import type { PlanViewState } from "./useQueryPlan";

export function QueryAnalysisWorkspace({
  currentSql,
  mode,
  onClose,
  onRun,
  open,
  state,
}: {
  currentSql: string;
  mode: PlanMode;
  onClose: () => void;
  onRun: () => void;
  open: boolean;
  state: PlanViewState;
}) {
  const [selected, setSelected] = useState<PlanNode | null>(null);
  const snapshotSql = state.sql ?? currentSql;
  const isActual = mode === "profile";
  const title = isActual ? "Actual Flow" : "Estimate";
  const runLabel = isActual ? "Run current SQL" : "Build current SQL";
  const runCurrent = () => {
    setSelected(null);
    onRun();
  };
  const isStale = Boolean(state.sql && state.sql !== currentSql);
  const range = useMemo(
    () => (selected && snapshotSql ? mapPlanNodeToSql(selected, snapshotSql) : null),
    [snapshotSql, selected],
  );

  return (
    <Dialog.Root onOpenChange={(next) => !next && onClose()} open={open}>
      <Dialog.Portal>
        <Dialog.Overlay className="analysis-workspace-overlay" />
        <Dialog.Content className="query-analysis-workspace">
          <header className="analysis-workspace-header">
            <div>
              <Dialog.Title>{title}</Dialog.Title>
              <Dialog.Description>
                {isActual
                  ? "DuckDB Profile executes this SQL and measures rows, rows scanned, and operator time."
                  : "DuckDB Explain builds a plan and row-count estimates without executing this SQL."}
              </Dialog.Description>
            </div>
            <div className="analysis-workspace-header-actions">
              {isStale && <span className="analysis-stale-label">Editor SQL changed</span>}
              <button
                className="toolbar-button"
                disabled={!currentSql.trim() || state.status === "loading"}
                onClick={runCurrent}
                type="button"
              >
                <ArrowClockwiseIcon aria-hidden="true" size={14} /> {runLabel}
              </button>
              <Dialog.Close aria-label={`Close ${title}`} className="icon-button">
                <XIcon aria-hidden="true" size={17} weight="bold" />
              </Dialog.Close>
            </div>
          </header>
          <div className="analysis-workspace-body">
            <SqlSnapshotPanel mode={mode} range={range} selected={selected} sql={snapshotSql} />
            <main
              className="analysis-workspace-graph"
              aria-label={isActual ? "Actual execution graph" : "Estimated query graph"}
            >
              {state.status === "loading" ? (
                <div className="flow-loading" role="status">
                  <span />
                  <span />
                  <span />
                  <strong>{isActual ? "Measuring actual flow" : "Building estimate"}</strong>
                </div>
              ) : state.status === "error" ? (
                <div className="result-state">
                  <div className="ui-inline-error" role="alert">
                    <strong>{title} failed</strong>
                    <span>{state.error}</span>
                  </div>
                  <button className="toolbar-button" onClick={runCurrent} type="button">
                    Try current SQL again
                  </button>
                </div>
              ) : state.status === "ready" ? (
                <QueryFlow
                  key={state.sql}
                  onSelectNode={setSelected}
                  plan={state.plan}
                  selectedNodeId={selected?.id ?? null}
                  showInspector={false}
                />
              ) : (
                <div className="panel-placeholder">
                  <strong>{isActual ? "No actual flow yet" : "No estimate yet"}</strong>
                  <span>Use {runLabel} above to analyze the current editor SQL.</span>
                </div>
              )}
            </main>
            <NodeInspector mode={mode} node={selected} />
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function SqlSnapshotPanel({
  mode,
  range,
  selected,
  sql,
}: {
  mode: PlanMode;
  range: SqlRange | null;
  selected: PlanNode | null;
  sql: string;
}) {
  const isActual = mode === "profile";
  return (
    <aside className="analysis-sql-panel" aria-label={isActual ? "Profiled SQL" : "Planned SQL"}>
      <header>
        <strong>{isActual ? "Profiled SQL" : "Planned SQL"}</strong>
        <span>Immutable {isActual ? "execution" : "plan"} snapshot</span>
      </header>
      <pre>
        <code>
          {range ? (
            <>
              {sql.slice(0, range.from)}
              <mark>{sql.slice(range.from, range.to)}</mark>
              {sql.slice(range.to)}
            </>
          ) : (
            sql
          )}
        </code>
      </pre>
      <footer>
        {selected
          ? range
            ? "Highlighted for the selected operation"
            : "SQL location unavailable for this operation"
          : "Select a graph node to highlight reliable SQL"}
      </footer>
    </aside>
  );
}
