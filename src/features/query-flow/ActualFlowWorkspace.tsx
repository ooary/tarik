import { ArrowClockwiseIcon, XIcon } from "@phosphor-icons/react";
import * as Dialog from "@radix-ui/react-dialog";
import { useMemo, useState } from "react";
import type { PlanNode } from "../../lib/commands";
import { NodeInspector } from "./NodeInspector";
import { QueryFlow } from "./QueryFlow";
import { mapPlanNodeToSql, type SqlRange } from "./sqlMapping";
import type { PlanViewState } from "./useQueryPlan";

export function ActualFlowWorkspace({
  currentSql,
  onClose,
  onRun,
  open,
  state,
}: {
  currentSql: string;
  onClose: () => void;
  onRun: () => void;
  open: boolean;
  state: PlanViewState;
}) {
  const [selected, setSelected] = useState<PlanNode | null>(null);
  const profiledSql = state.sql ?? currentSql;
  const runCurrent = () => {
    setSelected(null);
    onRun();
  };
  const isStale = Boolean(state.sql && state.sql !== currentSql);
  const range = useMemo(
    () => (selected && profiledSql ? mapPlanNodeToSql(selected, profiledSql) : null),
    [profiledSql, selected],
  );

  return (
    <Dialog.Root onOpenChange={(next) => !next && onClose()} open={open}>
      <Dialog.Portal>
        <Dialog.Overlay className="actual-flow-overlay" />
        <Dialog.Content className="actual-flow-workspace">
          <header className="actual-flow-header">
            <div>
              <Dialog.Title>Actual Flow</Dialog.Title>
              <Dialog.Description>
                DuckDB Profile executes this SQL and measures rows, rows scanned, and operator time.
              </Dialog.Description>
            </div>
            <div className="actual-flow-header-actions">
              {isStale && <span className="analysis-stale-label">Editor SQL changed</span>}
              <button
                className="toolbar-button"
                disabled={!currentSql.trim() || state.status === "loading"}
                onClick={runCurrent}
                type="button"
              >
                <ArrowClockwiseIcon aria-hidden="true" size={14} /> Run current SQL
              </button>
              <Dialog.Close aria-label="Close Actual Flow" className="icon-button">
                <XIcon aria-hidden="true" size={17} weight="bold" />
              </Dialog.Close>
            </div>
          </header>
          <div className="actual-flow-body">
            <ProfiledSqlPanel range={range} selected={selected} sql={profiledSql} />
            <main className="actual-flow-graph" aria-label="Actual execution graph">
              {state.status === "loading" ? (
                <div className="flow-loading" role="status">
                  <span />
                  <span />
                  <span />
                  <strong>Measuring actual flow</strong>
                </div>
              ) : state.status === "error" ? (
                <div className="result-state">
                  <div className="ui-inline-error" role="alert">
                    <strong>Actual Flow failed</strong>
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
                  <strong>No actual flow yet</strong>
                  <span>Use Run current SQL above to collect measured operator metrics.</span>
                </div>
              )}
            </main>
            <NodeInspector mode="profile" node={selected} />
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function ProfiledSqlPanel({
  range,
  selected,
  sql,
}: {
  range: SqlRange | null;
  selected: PlanNode | null;
  sql: string;
}) {
  return (
    <aside className="profiled-sql-panel" aria-label="Profiled SQL">
      <header>
        <strong>Profiled SQL</strong>
        <span>Immutable execution snapshot</span>
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
