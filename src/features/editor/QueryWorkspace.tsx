import {
  CaretDownIcon,
  CaretUpIcon,
  DotsThreeIcon,
  FileIcon,
  PlayIcon,
  PlusIcon,
  XIcon,
} from "@phosphor-icons/react";
import * as Tabs from "@radix-ui/react-tabs";
import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { ContextMenu } from "../../components/ui";
import type { ProjectCatalog } from "../../lib/commands";
import { SqlEditor } from "./SqlEditor";
import type { SqlTable } from "./sqlCompletion";
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
              closeTab(tab.id);
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
            <button className="run-button" type="button">
              <PlayIcon aria-hidden="true" size={14} weight="fill" /> Run query <kbd>Ctrl</kbd>
              <kbd>Enter</kbd>
            </button>
            <button className="toolbar-button" type="button">
              Explain
            </button>
          </div>
          <div className="toolbar-group toolbar-group-right">
            {saveError && (
              <span className="draft-save-error" role="alert" title={saveError}>
                Draft not saved
              </span>
            )}
            <span className="selection-note">Statement 1 of 1</span>
            <button aria-label="More query actions" className="icon-button" type="button">
              <DotsThreeIcon aria-hidden="true" size={18} weight="bold" />
            </button>
          </div>
        </div>

        {tabs.map((tab) =>
          activeTabId === tab.id ? (
            <SqlEditor
              key={tab.id}
              onChange={(sql) => editSql(tab.id, sql)}
              onRun={() => undefined}
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
            <Tabs.Root onValueChange={(value) => onUpdatePanel(value as Panel)} value={activePanel}>
              <Tabs.List aria-label="Query output" className="result-tabs">
                <Tabs.Trigger
                  className="result-tab"
                  onClick={() => onUpdatePanel("results")}
                  value="results"
                >
                  Results <span className="tab-count">24,318</span>
                </Tabs.Trigger>
                <Tabs.Trigger
                  className="result-tab"
                  onClick={() => onUpdatePanel("flow")}
                  value="flow"
                >
                  Flow
                </Tabs.Trigger>
                <Tabs.Trigger
                  className="result-tab"
                  onClick={() => onUpdatePanel("profile")}
                  value="profile"
                >
                  Profile
                </Tabs.Trigger>
              </Tabs.List>
            </Tabs.Root>
            <div className="results-actions">
              <span className="result-duration">Completed in 1.82s</span>
              <button className="toolbar-button" type="button">
                Export
              </button>
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

          {bottomOpen && activePanel === "results" && (
            <div className="result-surface">
              <div className="result-toolbar">
                <span>Rows 1-500 of 24,318</span>
                <button className="subtle-button" type="button">
                  Copy visible rows
                </button>
              </div>
              <div className="result-scroll" tabIndex={0}>
                <table>
                  <thead>
                    <tr>
                      <th>country</th>
                      <th>orders</th>
                      <th>revenue</th>
                      <th>share</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr>
                      <td>Singapore</td>
                      <td>6,842</td>
                      <td>$2,431,900.00</td>
                      <td>38.4%</td>
                    </tr>
                    <tr>
                      <td>Indonesia</td>
                      <td>11,204</td>
                      <td>$1,885,220.00</td>
                      <td>29.8%</td>
                    </tr>
                    <tr>
                      <td>Malaysia</td>
                      <td>4,927</td>
                      <td>$1,192,410.00</td>
                      <td>18.9%</td>
                    </tr>
                    <tr>
                      <td>Thailand</td>
                      <td>1,345</td>
                      <td>$512,780.00</td>
                      <td>8.1%</td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>
          )}
          {bottomOpen && activePanel !== "results" && (
            <div className="panel-placeholder">
              <strong>
                {activePanel === "flow"
                  ? "Query flow is ready"
                  : "Profile is ready after execution"}
              </strong>
              <span>
                {activePanel === "flow"
                  ? "Run the query to inspect how DuckDB connects each operation."
                  : "Execute this statement to see actual operator timing."}
              </span>
            </div>
          )}
        </div>
      </section>
    );
  },
);
