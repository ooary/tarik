import {
  CaretDownIcon,
  CaretUpIcon,
  DotsThreeIcon,
  FileIcon,
  PlayIcon,
  PlusIcon,
  XIcon,
} from "@phosphor-icons/react";
import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import type { EffectiveTheme } from "../../app/preferences";
import { ContextMenu } from "../../components/ui";
import type { ProjectCatalog } from "../../lib/commands";
import { ExportDialog } from "../export/ExportDialog";
import { QueryAnalysisWorkspace } from "../query-flow/QueryAnalysisWorkspace";
import { useQueryPlan } from "../query-flow/useQueryPlan";
import { ResultGrid } from "../results/ResultGrid";
import { SavedQueryLibrary } from "../saved-queries/SavedQueryLibrary";
import { useQueryExecution, type TabExecution } from "../results/useQueryExecution";
import { SqlEditor } from "./SqlEditor";
import type { SqlTable } from "./sqlCompletion";
import { countSqlStatements, isClearlyReadOnlySql } from "./sqlText";
import { useQueryTabs } from "./useQueryTabs";
import { useSqlValidation } from "./useSqlValidation";

type AnalysisMode = "explain" | "profile";

export interface QueryWorkspaceHandle {
  insertSql(text: string): void;
  openPreview(sql: string): void;
  flushDraft(): Promise<void>;
}

interface QueryWorkspaceProps {
  projectId: string;
  effectiveTheme?: EffectiveTheme;
  catalog: ProjectCatalog;
  bottomOpen: boolean;
  bottomPanelHeight: number;
  onQuerySucceeded?: () => void | Promise<void>;
  onSetBottomHeight: (height: number) => void;
  onToggleBottom: () => void;
}

export const QueryWorkspace = forwardRef<QueryWorkspaceHandle, QueryWorkspaceProps>(
  function QueryWorkspace(
    {
      projectId,
      effectiveTheme = "light",
      catalog,
      bottomOpen,
      bottomPanelHeight,
      onQuerySucceeded,
      onSetBottomHeight,
      onToggleBottom,
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
      flush,
    } = useQueryTabs(projectId);
    const sectionRef = useRef<HTMLElement>(null);
    const { executions, run, cancel, forget } = useQueryExecution(projectId, onQuerySucceeded);
    const [runError, setRunError] = useState<string | null>(null);
    const [analysisMode, setAnalysisMode] = useState<AnalysisMode | null>(null);
    const { states: planStates, runPlan, clearPlan } = useQueryPlan(projectId);

    const activeTab = tabs.find((tab) => tab.id === activeTabId);
    const activeExecution: TabExecution | undefined = activeTab
      ? executions[activeTab.id]
      : undefined;
    const executionActive =
      activeExecution?.state === "queued" || activeExecution?.state === "running";
    const validation = useSqlValidation(projectId, activeTab?.id, activeTab?.sql, catalog);

    const submitSql = (tabId: string, sql: string) => {
      if (!projectId || executionActive) return;
      if (
        sql.trim().length > 0 &&
        !isClearlyReadOnlySql(sql) &&
        !window.confirm(
          "Run query?\n\nThis SQL may modify your project. INSERT, UPDATE, DELETE, CREATE, ALTER, and DROP can change stored data or catalog objects.",
        )
      ) {
        return;
      }
      setRunError(null);
      if (!bottomOpen) onToggleBottom();
      run(tabId, sql).catch((error: unknown) => {
        setRunError(error instanceof Error ? error.message : String(error));
      });
    };

    const runActiveTab = () => {
      if (!activeTab) return;
      submitSql(activeTab.id, activeTab.sql);
    };

    const rerunExecution = (execution: TabExecution) => {
      if (!activeTab) return;
      submitSql(activeTab.id, execution.sql);
    };

    const runEstimate = () => {
      if (!projectId || !activeTab || !activeTab.sql.trim()) return;
      setAnalysisMode("explain");
      void runPlan(activeTab.sql, "explain");
    };

    const runActualFlow = () => {
      if (!projectId || !activeTab || !activeTab.sql.trim()) return;
      if (
        !isClearlyReadOnlySql(activeTab.sql) &&
        !window.confirm(
          "Run Actual Flow?\n\nActual Flow executes this SQL to collect operator metrics. INSERT, UPDATE, DELETE, CREATE, ALTER, and DROP may modify your project.",
        )
      ) {
        return;
      }
      setAnalysisMode("profile");
      void runPlan(activeTab.sql, "profile");
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
        flushDraft() {
          return flush();
        },
      }),
      [tabs, activeTabId, editSql, addTab, flush],
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
              aria-label="Run query"
              className="run-button"
              disabled={executionActive || !activeTab || !projectId}
              onClick={runActiveTab}
              type="button"
            >
              <PlayIcon aria-hidden="true" size={14} weight="fill" />
              Run query <kbd>Ctrl</kbd>
              <kbd>Enter</kbd>
            </button>
            <SavedQueryLibrary
              activeSql={activeTab?.sql ?? ""}
              activeTitle={activeTab?.title ?? "Untitled"}
              onOpenSql={(sql, title) => addTab(sql, title)}
              projectId={projectId}
            />
            <button
              className="toolbar-button"
              disabled={!activeTab?.sql.trim() || planStates.explain.status === "loading"}
              onClick={runEstimate}
              type="button"
            >
              Estimate
            </button>
            <button
              className="toolbar-button"
              disabled={!activeTab?.sql.trim() || planStates.profile.status === "loading"}
              onClick={runActualFlow}
              type="button"
            >
              Actual Flow
            </button>
            <ExportDialog
              projectId={projectId}
              sql={activeTab?.sql ?? ""}
              suggestedName={activeTab?.title ?? "query_export"}
            />
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
            <SqlValidationSummary state={validation} />
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
              diagnostics={validation.diagnostics}
              effectiveTheme={effectiveTheme}
              key={tab.id}
              onChange={(sql) => editSql(tab.id, sql)}
              onRun={runActiveTab}
              tables={tables}
              value={tab.sql}
            />
          ) : null,
        )}

        <div
          aria-hidden="true"
          className={`bottom-resize-handle ${bottomOpen ? "" : "bottom-resize-hidden"}`}
          onPointerDown={resizeBottom}
        />
        <div className={`bottom-panel ${bottomOpen ? "bottom-panel-open" : "bottom-panel-closed"}`}>
          <div className="results-heading">
            <strong className="results-title">Results</strong>
            <div className="results-actions">
              <span className="result-duration">{resultDuration(activeExecution)}</span>
              {executionActive && (
                <button
                  className="toolbar-button"
                  onClick={() => void cancel(activeTabId ?? "")}
                  type="button"
                >
                  Cancel
                </button>
              )}
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
            </div>
          </div>
          {bottomOpen && (
            <ResultPanel
              execution={activeExecution}
              onRunAgain={rerunExecution}
              runError={runError}
            />
          )}
        </div>
        {analysisMode && (
          <QueryAnalysisWorkspace
            currentSql={activeTab?.sql ?? ""}
            mode={analysisMode}
            onClose={() => setAnalysisMode(null)}
            onRun={analysisMode === "explain" ? runEstimate : runActualFlow}
            open
            state={planStates[analysisMode]}
          />
        )}
      </section>
    );
  },
);

function SqlValidationSummary({ state }: { state: ReturnType<typeof useSqlValidation> }) {
  switch (state.status) {
    case "idle":
    case "editing":
      return null;
    case "checking":
      return (
        <span className="sql-validation-summary" role="status">
          Checking SQL
        </span>
      );
    case "clean":
      return (
        <span className="sql-validation-summary" title="Runtime-only failures may still occur.">
          No problems detected before execution
        </span>
      );
    case "unavailable":
      return (
        <span className="sql-validation-summary sql-validation-unavailable" title={state.message}>
          SQL check unavailable
        </span>
      );
    case "problems": {
      const first = state.diagnostics[0];
      const count = state.diagnostics.length;
      return (
        <span
          className="sql-validation-summary sql-validation-problems"
          role="status"
          title={`${first.code}: ${first.message}`}
        >
          {count.toLocaleString("en-US")} {count === 1 ? "problem" : "problems"}: {first.message}
        </span>
      );
    }
  }
}

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
  onRunAgain,
  runError,
}: {
  execution: TabExecution | undefined;
  onRunAgain: (execution: TabExecution) => void;
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
            onRunAgain={() => onRunAgain(execution)}
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
