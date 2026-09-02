import {
  ArrowsInSimpleIcon,
  ArrowsOutSimpleIcon,
  CaretDownIcon,
  CaretUpIcon,
  DotsThreeIcon,
  FileIcon,
  PlayIcon,
  PlusIcon,
  XIcon,
} from "@phosphor-icons/react";
import * as Tabs from "@radix-ui/react-tabs";
import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import { ContextMenu } from "../../components/ui";
import type { ProjectCatalog } from "../../lib/commands";
import { QueryFlow } from "../query-flow/QueryFlow";
import { mapPlanNodeToSql, type SqlRange } from "../query-flow/sqlMapping";
import { useQueryPlan } from "../query-flow/useQueryPlan";
import { ResultGrid } from "../results/ResultGrid";
import { useQueryExecution, type TabExecution } from "../results/useQueryExecution";
import { SqlEditor } from "./SqlEditor";
import type { SqlTable } from "./sqlCompletion";
import { countSqlStatements, isClearlyReadOnlySql } from "./sqlText";
import { useQueryTabs } from "./useQueryTabs";

type Panel = "results" | "flow" | "profile";

export interface QueryWorkspaceHandle {
  insertSql(text: string): void;
  openPreview(sql: string): void;
}

interface QueryWorkspaceProps {
  projectId: string;
  catalog: ProjectCatalog;
  activePanel: Panel;
  bottomOpen: boolean;
  bottomPanelHeight: number;
  onQuerySucceeded?: () => void | Promise<void>;
  onSetBottomHeight: (height: number) => void;
  onToggleBottom: () => void;
  onUpdatePanel: (panel: Panel) => void;
}

export const QueryWorkspace = forwardRef<QueryWorkspaceHandle, QueryWorkspaceProps>(
  function QueryWorkspace(
    {
      projectId,
      catalog,
      activePanel,
      bottomOpen,
      bottomPanelHeight,
      onQuerySucceeded,
      onSetBottomHeight,
      onToggleBottom,
      onUpdatePanel,
    },
    ref,
  ) {
    const {
      tabs,
      activeTabId,
      saveError,
      selectTab,
      addTab,
      closeTab,
      renameTab,
      duplicateTab,
      moveTab,
      editSql,
    } = useQueryTabs(projectId);
    const sectionRef = useRef<HTMLElement>(null);
    const outputPanelRef = useRef<HTMLDivElement>(null);
    const fullscreenButtonRef = useRef<HTMLButtonElement>(null);
    const { executions, run, cancel, forget } = useQueryExecution(projectId, onQuerySucceeded);
    const [runError, setRunError] = useState<string | null>(null);
    const [autoRunPending, setAutoRunPending] = useState(false);
    const [flowFullscreen, setFlowFullscreen] = useState(false);
    const [planHighlight, setPlanHighlight] = useState<{ tabId: string; range: SqlRange } | null>(
      null,
    );
    const { states: planStates, runPlan, clearPlan } = useQueryPlan(projectId);

    const activeTab = tabs.find((tab) => tab.id === activeTabId);
    const activeExecution: TabExecution | undefined = activeTab
      ? executions[activeTab.id]
      : undefined;
    const executionActive =
      activeExecution?.state === "queued" || activeExecution?.state === "running";

    useEffect(() => {
      if (!flowFullscreen) return;
      fullscreenButtonRef.current?.focus();
      const containFullscreenFocus = (event: KeyboardEvent) => {
        if (event.key === "Escape") {
          setFlowFullscreen(false);
          window.setTimeout(() => fullscreenButtonRef.current?.focus(), 0);
          return;
        }
        if (event.key !== "Tab") return;
        const controls = outputPanelRef.current?.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [href], summary, [tabindex]:not([tabindex="-1"])',
        );
        if (!controls?.length) return;
        const first = controls[0];
        const last = controls[controls.length - 1];
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      };
      document.addEventListener("keydown", containFullscreenFocus);
      return () => document.removeEventListener("keydown", containFullscreenFocus);
    }, [flowFullscreen]);

    const updateOutputPanel = (panel: Panel) => {
      if (panel === "results") setFlowFullscreen(false);
      onUpdatePanel(panel);
    };

    const runActiveTab = () => {
      if (!projectId || !activeTab || executionActive || autoRunPending) return;
      const { id: tabId, sql } = activeTab;
      setRunError(null);
      setPlanHighlight(null);
      setAutoRunPending(true);
      updateOutputPanel("flow");
      if (!bottomOpen) onToggleBottom();
      // Explain is non-executing and deliberately completes before the real
      // submission. This gives Run an estimated path animation without ever
      // profiling or executing DDL/DML twice.
      void runPlan(sql, "explain")
        .then(() => run(tabId, sql))
        .catch((error: unknown) => {
          setRunError(error instanceof Error ? error.message : String(error));
        })
        .finally(() => setAutoRunPending(false));
    };

    const runActivePlan = (mode: "explain" | "profile") => {
      if (!projectId || !activeTab || !activeTab.sql.trim()) return;
      if (
        mode === "profile" &&
        !isClearlyReadOnlySql(activeTab.sql) &&
        !window.confirm(
          "Run Actual Flow?\n\nActual Flow executes this SQL to collect operator metrics. INSERT, UPDATE, DELETE, CREATE, ALTER, and DROP may modify your project.",
        )
      ) {
        return;
      }
      setPlanHighlight(null);
      updateOutputPanel(mode === "explain" ? "flow" : "profile");
      if (!bottomOpen) onToggleBottom();
      void runPlan(activeTab.sql, mode);
    };

    const closeTabAndForget = (tabId: string) => {
      forget(tabId);
      clearPlan();
      closeTab(tabId);
    };

    const tables = catalog.objects.map((object): SqlTable => ({
      schema: object.schema,
      label: object.name,
      type: object.kind,
      columns: catalog.columns
        .filter(
          (column) =>
            column.database === object.database &&
            column.schema === object.schema &&
            column.object === object.name,
        )
        .map((column) => column.name),
    }));

    useImperativeHandle(
      ref,
      () => ({
        insertSql(text: string) {
          const active = tabs.find((tab) => tab.id === activeTabId);
          if (!active) return;
          const separator = active.sql.length > 0 && !active.sql.endsWith(" ") ? " " : "";
          editSql(active.id, `${active.sql}${separator}${text}`);
        },
        openPreview(sql: string) {
          addTab(sql);
        },
      }),
      [tabs, activeTabId, editSql, addTab],
    );

    function resizeBottom(event: React.PointerEvent<HTMLDivElement>) {
      if (!sectionRef.current) return;
      const section = sectionRef.current;
      const move = (moveEvent: PointerEvent) => {
        const height = Math.min(560, Math.max(180, window.innerHeight - moveEvent.clientY));
        section.style.setProperty("--bottom-height", `${height}px`);
        onSetBottomHeight(height);
      };
      const stop = () => {
        document.removeEventListener("pointermove", move);
        document.removeEventListener("pointerup", stop);
      };
      document.addEventListener("pointermove", move);
      document.addEventListener("pointerup", stop, { once: true });
      event.currentTarget.setPointerCapture(event.pointerId);
    }

    useEffect(() => {
      sectionRef.current?.style.setProperty("--bottom-height", `${bottomPanelHeight}px`);
    }, [bottomPanelHeight]);

    return (
      <section ref={sectionRef} aria-label="SQL workspace" className="query-workspace">
        <div className="query-tabs" role="tablist" aria-label="Query tabs">
          {tabs.map((tab, index) => {
            const close = () => {
              if (
                tab.dirty &&
                saveError &&
                !window.confirm(`Close "${tab.title}"?\n\nThe latest draft has not been saved.`)
              ) {
                return;
              }
              closeTabAndForget(tab.id);
            };
            const rename = () => {
              const title = window.prompt("Query tab name", tab.title);
              if (title) renameTab(tab.id, title);
            };
            return (
              <ContextMenu
                items={[
                  { label: "Rename", onSelect: rename },
                  { label: "Duplicate", onSelect: () => duplicateTab(tab.id) },
                  {
                    disabled: index === 0,
                    label: "Move left",
                    onSelect: () => moveTab(tab.id, -1),
                  },
                  {
                    disabled: index === tabs.length - 1,
                    label: "Move right",
                    onSelect: () => moveTab(tab.id, 1),
                  },
                  { danger: Boolean(tab.dirty && saveError), label: "Close", onSelect: close },
                ]}
                key={tab.id}
                label={`${tab.title} tab actions`}
              >
                <button
                  aria-selected={activeTabId === tab.id}
                  className={`query-tab ${activeTabId === tab.id ? "query-tab-active" : ""}`}
                  onClick={() => selectTab(tab.id)}
                  onDoubleClick={rename}
                  role="tab"
                  type="button"
                >
                  <FileIcon aria-hidden="true" className="tab-file-icon" size={14} />
                  <span className="query-tab-title" title={tab.title}>
                    {tab.title}
                    {tab.dirty ? " •" : ""}
                  </span>
                  <XIcon
                    aria-label={`Close ${tab.title}`}
                    className="tab-close"
                    onClick={(event) => {
                      event.stopPropagation();
                      close();
                    }}
                    role="button"
                    size={14}
                  />
                </button>
              </ContextMenu>
            );
          })}
          <button
            aria-label="New query tab"
            className="icon-button tab-add"
            onClick={() => addTab()}
            type="button"
          >
            <PlusIcon aria-hidden="true" size={17} weight="bold" />
          </button>
        </div>

        <div className="editor-toolbar">
          <div className="toolbar-group">
            <button
              aria-label={autoRunPending ? "Preparing query flow" : "Run query"}
              className="run-button"
              disabled={executionActive || autoRunPending || !activeTab || !projectId}
              onClick={runActiveTab}
              type="button"
            >
              <PlayIcon aria-hidden="true" size={14} weight="fill" />
              {autoRunPending ? "Preparing flow" : "Run query"} <kbd>Ctrl</kbd>
              <kbd>Enter</kbd>
            </button>
            <button
              className="toolbar-button"
              disabled={!activeTab?.sql.trim() || planStates.explain.status === "loading"}
              onClick={() => runActivePlan("explain")}
              type="button"
            >
              Explain
            </button>
          </div>
          <div className="toolbar-group toolbar-group-right">
            {saveError && (
              <span className="draft-save-error" role="alert" title={saveError}>
                Draft not saved
              </span>
            )}
            {runError && (
              <span className="draft-save-error" role="alert" title={runError}>
                Query could not start
              </span>
            )}
            <span className="selection-note">
              {activeTab ? `${countSqlStatements(activeTab.sql)} statement(s)` : "No query tab"}
            </span>
            <button aria-label="More query actions" className="icon-button" type="button">
              <DotsThreeIcon aria-hidden="true" size={18} weight="bold" />
            </button>
          </div>
        </div>

        {tabs.map((tab) =>
          activeTabId === tab.id ? (
            <SqlEditor
              highlightRange={planHighlight?.tabId === tab.id ? planHighlight.range : null}
              key={tab.id}
              onChange={(sql) => {
                setPlanHighlight(null);
                editSql(tab.id, sql);
              }}
              onRun={runActiveTab}
              tables={tables}
              value={tab.sql}
            />
          ) : null,
        )}

        <div
          aria-hidden="true"
          className={`bottom-resize-handle ${bottomOpen && !flowFullscreen ? "" : "bottom-resize-hidden"}`}
          onPointerDown={resizeBottom}
        />
        <div
          aria-label={flowFullscreen ? "Fullscreen query flow" : undefined}
          aria-modal={flowFullscreen ? true : undefined}
          className={`bottom-panel ${bottomOpen ? "bottom-panel-open" : "bottom-panel-closed"} ${flowFullscreen ? "bottom-panel-flow-fullscreen" : ""}`}
          ref={outputPanelRef}
          role={flowFullscreen ? "dialog" : undefined}
        >
          <div className="results-heading">
            <Tabs.Root
              onValueChange={(value) => updateOutputPanel(value as Panel)}
              value={activePanel}
            >
              <Tabs.List aria-label="Query output" className="result-tabs">
                <Tabs.Trigger
                  className="result-tab"
                  onClick={() => updateOutputPanel("results")}
                  value="results"
                >
                  Results
                  {activeExecution?.rowsProduced != null && (
                    <span className="tab-count">
                      {activeExecution.rowsProduced.toLocaleString("en-US")}
                    </span>
                  )}
                </Tabs.Trigger>
                <Tabs.Trigger
                  className="result-tab"
                  onClick={() => updateOutputPanel("flow")}
                  value="flow"
                >
                  Estimate
                </Tabs.Trigger>
                <Tabs.Trigger
                  className="result-tab"
                  onClick={() => updateOutputPanel("profile")}
                  value="profile"
                >
                  Actual Flow
                </Tabs.Trigger>
              </Tabs.List>
            </Tabs.Root>
            <div className="results-actions">
              <span className="result-duration">{resultDuration(activeExecution)}</span>
              {activePanel !== "results" && bottomOpen && (
                <button
                  aria-label={flowFullscreen ? "Exit fullscreen flow" : "Open fullscreen flow"}
                  className="icon-button"
                  onClick={() => setFlowFullscreen((current) => !current)}
                  ref={fullscreenButtonRef}
                  title={flowFullscreen ? "Exit fullscreen (Esc)" : "Open fullscreen"}
                  type="button"
                >
                  {flowFullscreen ? (
                    <ArrowsInSimpleIcon aria-hidden="true" size={16} />
                  ) : (
                    <ArrowsOutSimpleIcon aria-hidden="true" size={16} />
                  )}
                </button>
              )}
              {executionActive && (
                <button
                  className="toolbar-button"
                  onClick={() => void cancel(activeTabId ?? "")}
                  type="button"
                >
                  Cancel
                </button>
              )}
              {!flowFullscreen && (
                <button
                  aria-label={bottomOpen ? "Collapse result panel" : "Expand result panel"}
                  className="icon-button"
                  onClick={onToggleBottom}
                  type="button"
                >
                  {bottomOpen ? (
                    <CaretDownIcon aria-hidden="true" size={16} />
                  ) : (
                    <CaretUpIcon aria-hidden="true" size={16} />
                  )}
                </button>
              )}
            </div>
          </div>

          {bottomOpen && activePanel === "results" && (
            <ResultPanel execution={activeExecution} runError={runError} />
          )}
          {bottomOpen && activePanel === "flow" && (
            <PlanPanel
              key={`explain-${flowFullscreen ? "fullscreen" : "panel"}`}
              mode="explain"
              onRun={() => runActivePlan("explain")}
              onSelectNode={(node) => {
                if (
                  !node ||
                  !planStates.explain.sql ||
                  !activeTab ||
                  activeTab.sql !== planStates.explain.sql
                ) {
                  setPlanHighlight(null);
                  return;
                }
                const range = mapPlanNodeToSql(node, planStates.explain.sql);
                setPlanHighlight(range ? { tabId: activeTab.id, range } : null);
              }}
              state={planStates.explain}
            />
          )}
          {bottomOpen && activePanel === "profile" && (
            <PlanPanel
              key={`profile-${flowFullscreen ? "fullscreen" : "panel"}`}
              mode="profile"
              onRun={() => runActivePlan("profile")}
              onSelectNode={(node) => {
                if (
                  !node ||
                  !planStates.profile.sql ||
                  !activeTab ||
                  activeTab.sql !== planStates.profile.sql
                ) {
                  setPlanHighlight(null);
                  return;
                }
                const range = mapPlanNodeToSql(node, planStates.profile.sql);
                setPlanHighlight(range ? { tabId: activeTab.id, range } : null);
              }}
              state={planStates.profile}
            />
          )}
        </div>
      </section>
    );
  },
);

function resultDuration(execution: TabExecution | undefined): string {
  if (!execution) return "";
  switch (execution.state) {
    case "queued":
      return "Queued";
    case "running":
      return `Running ${formatSeconds(execution.durationMs)}`;
    case "succeeded":
      return `Completed in ${formatSeconds(execution.durationMs)}`;
    case "failed":
      return "Failed";
    case "cancelled":
      return "Cancelled";
  }
}

function formatSeconds(durationMs: number): string {
  if (durationMs >= 10_000) return `${Math.round(durationMs / 1000)}s`;
  return `${(durationMs / 1000).toFixed(1)}s`;
}

function ResultPanel({
  execution,
  runError,
}: {
  execution: TabExecution | undefined;
  runError: string | null;
}) {
  if (runError) {
    return (
      <div className="result-state">
        <div className="ui-inline-error" role="alert">
          <strong>Query could not start</strong>
          <span>{runError}</span>
        </div>
      </div>
    );
  }
  if (!execution) {
    return (
      <div className="result-state">
        <div className="ui-empty-state">
          <strong>No results yet</strong>
          <span>Run a query to see results here.</span>
        </div>
      </div>
    );
  }
  switch (execution.state) {
    case "queued":
      return (
        <div className="result-state" role="status">
          <strong>Queued</strong>
          <span>The engine will start this query when the current one finishes.</span>
        </div>
      );
    case "running":
      return (
        <div className="result-state" role="status">
          <strong>Running</strong>
          <span>
            {execution.rowsProduced != null
              ? `${execution.rowsProduced.toLocaleString("en-US")} rows produced so far`
              : "Waiting for the first rows from DuckDB"}
          </span>
        </div>
      );
    case "failed":
      return (
        <div className="result-state">
          <div className="ui-inline-error" role="alert">
            <strong>{execution.error?.code ?? "query.failed"}</strong>
            <span>{execution.error?.message ?? "The query failed."}</span>
          </div>
        </div>
      );
    case "cancelled":
      return (
        <div className="result-state" role="status">
          <strong>Cancelled</strong>
          <span>The query was stopped before completion.</span>
        </div>
      );
    case "succeeded":
      if (execution.rowsProduced != null && execution.resultId) {
        return (
          <ResultGrid
            resultId={execution.resultId}
            rowTotal={execution.rowTotal ?? execution.rowsProduced}
          />
        );
      }
      return (
        <div className="result-state" role="status">
          <strong>Statement completed</strong>
          {execution.rowsAffected != null ? (
            <span>{execution.rowsAffected.toLocaleString("en-US")} rows affected.</span>
          ) : (
            <span>The statement finished without returning rows.</span>
          )}
        </div>
      );
  }
}

function PlanPanel({
  mode,
  onRun,
  onSelectNode,
  state,
}: {
  mode: "explain" | "profile";
  onRun: () => void;
  onSelectNode: (node: import("../../lib/commands").PlanNode | null) => void;
  state: ReturnType<typeof useQueryPlan>["states"]["explain"];
}) {
  if (state.status === "loading") {
    return (
      <div className="flow-loading" role="status">
        <span />
        <span />
        <span />
        <strong>{mode === "profile" ? "Measuring actual flow" : "Building estimate"}</strong>
      </div>
    );
  }
  if (state.status === "error") {
    return (
      <div className="result-state">
        <div className="ui-inline-error" role="alert">
          <strong>Plan failed</strong>
          <span>{state.error}</span>
        </div>
        <button className="toolbar-button" onClick={onRun} type="button">
          Try again
        </button>
      </div>
    );
  }
  if (state.status === "ready" && state.plan.mode === mode) {
    return <QueryFlow onSelectNode={onSelectNode} plan={state.plan} />;
  }
  return (
    <div className="panel-placeholder">
      <strong>{mode === "profile" ? "No actual flow yet" : "No estimate yet"}</strong>
      <span>
        {mode === "profile"
          ? "Actual Flow runs the SQL using DuckDB Profile and shows measured rows, rows scanned, and operator time."
          : "Estimate shows DuckDB's plan and row-count guesses without running the SQL."}
      </span>
      <button className="toolbar-button" onClick={onRun} type="button">
        {mode === "profile" ? "Run Actual Flow" : "Build Estimate"}
      </button>
    </div>
  );
}
