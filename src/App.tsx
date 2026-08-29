import * as Tabs from "@radix-ui/react-tabs";
import { CaretDownIcon, CaretUpIcon, DatabaseIcon, DotsThreeIcon, FileIcon, FolderOpenIcon, ListIcon, PlusIcon, PlayIcon, TableIcon, XIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import "./App.css";
import { getRuntimeInfo, type RuntimeInfo } from "./lib/commands";

type RuntimeState =
  | { kind: "loading" }
  | { kind: "ready"; info: RuntimeInfo }
  | { kind: "unavailable" };

type Panel = "results" | "flow" | "profile";

const sqlPreview = `SELECT
  c.country,
  COUNT(*) AS orders,
  SUM(o.amount) AS revenue
FROM orders AS o
JOIN customers AS c
  ON o.customer_id = c.customer_id
GROUP BY c.country
ORDER BY revenue DESC;`;

function SourceIcon({ kind }: { kind: "database" | "folder" | "table" }) {
  const Icon = kind === "database" ? DatabaseIcon : kind === "folder" ? FolderOpenIcon : TableIcon;
  return <Icon aria-hidden="true" className="source-icon" size={15} weight="regular" />;
}

function App() {
  const workspaceRef = useRef<HTMLDivElement>(null);
  const queryWorkspaceRef = useRef<HTMLElement>(null);
  const [runtime, setRuntime] = useState<RuntimeState>({ kind: "loading" });
  const [activePanel, setActivePanel] = useState<Panel>("results");
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [bottomOpen, setBottomOpen] = useState(true);

  function resizeSidebar(event: React.PointerEvent<HTMLDivElement>) {
    if (!workspaceRef.current) return;
    const workspace = workspaceRef.current;
    const move = (moveEvent: PointerEvent) => {
      const width = Math.min(360, Math.max(220, moveEvent.clientX));
      workspace.style.setProperty("--sidebar-width", `${width}px`);
    };
    const stop = () => {
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", stop);
    };
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", stop, { once: true });
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  function resizeBottom(event: React.PointerEvent<HTMLDivElement>) {
    if (!queryWorkspaceRef.current) return;
    const queryWorkspace = queryWorkspaceRef.current;
    const move = (moveEvent: PointerEvent) => {
      const height = Math.min(560, Math.max(180, window.innerHeight - moveEvent.clientY));
      queryWorkspace.style.setProperty("--bottom-height", `${height}px`);
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
    let active = true;

    getRuntimeInfo()
      .then((info) => {
        if (active) setRuntime({ kind: "ready", info });
      })
      .catch(() => {
        if (active) setRuntime({ kind: "unavailable" });
      });

    return () => {
      active = false;
    };
  }, []);

  return (
    <main className="app-shell">
      <header className="app-header">
        <div className="brand-lockup">
          <button
            aria-label={sidebarOpen ? "Collapse source explorer" : "Expand source explorer"}
            className="icon-button header-menu"
            onClick={() => setSidebarOpen((open) => !open)}
            type="button"
          >
            <ListIcon aria-hidden="true" size={17} weight="regular" />
          </button>
          <span className="brand-mark" aria-hidden="true">T</span>
          <span className="brand-name">Tarik</span>
        </div>

        <div className="project-context">
          <span className="project-file">retail-analysis.duckdb</span>
          <span className="project-mode">Local project</span>
        </div>

        <div className="header-actions">
          <span className="engine-status"><span aria-hidden="true" className="status-mark" /> DuckDB ready</span>
          <button className="text-button" type="button">Settings</button>
        </div>
      </header>

      <div ref={workspaceRef} className={`workspace ${sidebarOpen ? "workspace-sidebar-open" : "workspace-sidebar-closed"}`}>
        <aside aria-label="Source explorer" className="source-explorer">
          <div className="panel-heading">
            <div>
              <p className="panel-kicker">Workspace</p>
              <h2>Explorer</h2>
            </div>
            <button aria-label="Add source" className="icon-button add-button" type="button"><PlusIcon aria-hidden="true" size={17} weight="bold" /></button>
          </div>

          <div className="source-tree">
            <div className="tree-section-title">Project tables</div>
            <button className="tree-row tree-row-selected" type="button">
              <SourceIcon kind="database" />
              <span>retail-analysis</span>
            </button>
            <button className="tree-row tree-row-child" type="button">
              <SourceIcon kind="table" />
              <span>customers</span>
              <span className="row-meta">12 cols</span>
            </button>
            <button className="tree-row tree-row-child" type="button">
              <SourceIcon kind="table" />
              <span>order_items</span>
              <span className="row-meta">8 cols</span>
            </button>
            <button className="tree-row tree-row-child" type="button">
              <SourceIcon kind="table" />
              <span>products</span>
              <span className="row-meta">14 cols</span>
            </button>

            <div className="tree-section-title tree-section-spaced">Linked files</div>
            <button className="tree-row" type="button">
              <SourceIcon kind="folder" />
              <span>orders.parquet</span>
            </button>
            <button className="tree-row" type="button">
              <SourceIcon kind="folder" />
              <span>monthly_targets.csv</span>
            </button>
          </div>

          <div className="explorer-footer">
            <button className="footer-action" type="button">Import file</button>
            <button className="footer-action" type="button">New table</button>
          </div>
        </aside>

        <div aria-hidden="true" className="sidebar-resize-handle" onPointerDown={resizeSidebar} />
        <section ref={queryWorkspaceRef} aria-label="SQL workspace" className="query-workspace">
          <div className="query-tabs" role="tablist" aria-label="Query tabs">
            <button aria-selected="true" className="query-tab query-tab-active" role="tab" type="button">
              <FileIcon aria-hidden="true" className="tab-file-icon" size={14} /> monthly-revenue.sql <XIcon aria-hidden="true" className="tab-close" size={14} />
            </button>
            <button aria-selected="false" className="query-tab" role="tab" type="button">
              <FileIcon aria-hidden="true" className="tab-file-icon" size={14} /> customer-retention.sql <XIcon aria-hidden="true" className="tab-close" size={14} />
            </button>
            <button aria-label="New query tab" className="icon-button tab-add" type="button"><PlusIcon aria-hidden="true" size={17} weight="bold" /></button>
          </div>

          <div className="editor-toolbar">
            <div className="toolbar-group">
              <button className="run-button" type="button"><PlayIcon aria-hidden="true" size={14} weight="fill" /> Run query <kbd>Ctrl</kbd><kbd>Enter</kbd></button>
              <button className="toolbar-button" type="button">Explain</button>
            </div>
            <div className="toolbar-group toolbar-group-right">
              <span className="selection-note">Statement 1 of 1</span>
              <button aria-label="More query actions" className="icon-button" type="button"><DotsThreeIcon aria-hidden="true" size={18} weight="bold" /></button>
            </div>
          </div>

          <div aria-label="SQL editor" className="sql-editor" role="textbox" tabIndex={0}>
            <div className="line-numbers" aria-hidden="true">{sqlPreview.split("\n").map((_, index) => <span key={index}>{index + 1}</span>)}</div>
            <pre className="sql-code"><code>{sqlPreview}</code></pre>
          </div>

          <div aria-hidden="true" className={`bottom-resize-handle ${bottomOpen ? "" : "bottom-resize-hidden"}`} onPointerDown={resizeBottom} />
          <div className={`bottom-panel ${bottomOpen ? "bottom-panel-open" : "bottom-panel-closed"}`}>
            <div className="results-heading">
              <Tabs.Root onValueChange={(value) => setActivePanel(value as Panel)} value={activePanel}>
                <Tabs.List aria-label="Query output" className="result-tabs">
                  <Tabs.Trigger className="result-tab" onClick={() => setActivePanel("results")} value="results">Results <span className="tab-count">24,318</span></Tabs.Trigger>
                  <Tabs.Trigger className="result-tab" onClick={() => setActivePanel("flow")} value="flow">Flow</Tabs.Trigger>
                  <Tabs.Trigger className="result-tab" onClick={() => setActivePanel("profile")} value="profile">Profile</Tabs.Trigger>
                </Tabs.List>
              </Tabs.Root>
              <div className="results-actions">
                <span className="result-duration">Completed in 1.82s</span>
                <button className="toolbar-button" type="button">Export</button>
                <button aria-label={bottomOpen ? "Collapse result panel" : "Expand result panel"} className="icon-button" onClick={() => setBottomOpen((open) => !open)} type="button">{bottomOpen ? <CaretDownIcon aria-hidden="true" size={16} /> : <CaretUpIcon aria-hidden="true" size={16} />}</button>
              </div>
            </div>

            {bottomOpen && activePanel === "results" && (
              <div className="result-surface">
                <div className="result-toolbar"><span>Rows 1-500 of 24,318</span><button className="subtle-button" type="button">Copy visible rows</button></div>
                <div className="result-scroll" tabIndex={0}>
                  <table>
                    <thead><tr><th>country</th><th>orders</th><th>revenue</th><th>share</th></tr></thead>
                    <tbody>
                      <tr><td>Singapore</td><td>6,842</td><td>$2,431,900.00</td><td>38.4%</td></tr>
                      <tr><td>Indonesia</td><td>11,204</td><td>$1,885,220.00</td><td>29.8%</td></tr>
                      <tr><td>Malaysia</td><td>4,927</td><td>$1,192,410.00</td><td>18.9%</td></tr>
                      <tr><td>Thailand</td><td>1,345</td><td>$512,780.00</td><td>8.1%</td></tr>
                    </tbody>
                  </table>
                </div>
              </div>
            )}
            {bottomOpen && activePanel !== "results" && (
              <div className="panel-placeholder"><strong>{activePanel === "flow" ? "Query flow is ready" : "Profile is ready after execution"}</strong><span>{activePanel === "flow" ? "Run the query to inspect how DuckDB connects each operation." : "Execute this statement to see actual operator timing."}</span></div>
            )}
          </div>
        </section>
      </div>

      <footer className="status-bar">
        <span className="status-bar-left"><span className="status-mark" aria-hidden="true" /> {runtime.kind === "ready" ? "Connected to local DuckDB" : "Connecting to local DuckDB"}</span>
        <span className="status-bar-right"><span>Memory limit: Balanced</span><span>2 threads</span><span>UTF-8</span></span>
      </footer>
    </main>
  );
}

export default App;
