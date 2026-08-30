import * as Tabs from "@radix-ui/react-tabs";
import { CaretDownIcon, CaretUpIcon, DatabaseIcon, DotsThreeIcon, FileIcon, ListIcon, PlusIcon, PlayIcon, TableIcon, XIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import "./App.css";
import { Dialog } from "./components/ui";
import {
  createWorkbenchPreferencesRepository,
  defaultWorkbenchPreferences,
  type WorkbenchPreferences,
} from "./app/preferences";
import {
  chooseDuckDbFile,
  closeProject,
  createProject,
  getActiveProject,
  getRuntimeInfo,
  getWorkbenchPreferences,
  inspectProjectCatalog,
  listRecentProjects,
  openProject,
  removeProject,
  renameProject,
  reopenRecentProject,
  setWorkbenchPreferences,
  type ActiveProject,
  type ProjectCatalog,
  type RecentProject,
  type RuntimeInfo,
} from "./lib/commands";

type RuntimeState =
  | { kind: "loading" }
  | { kind: "ready"; info: RuntimeInfo }
  | { kind: "unavailable" };

type Panel = "results" | "flow" | "profile";
type PreviewTheme = "system" | "light" | "dark";

const sqlPreview = `SELECT
  c.country,
  COUNT(*) AS orders,
  SUM(o.amount) AS revenue
FROM orders AS o
JOIN customers AS c
  ON o.customer_id = c.customer_id
GROUP BY c.country
ORDER BY revenue DESC;`;

function SourceIcon({ kind }: { kind: "database" | "table" }) {
  const Icon = kind === "database" ? DatabaseIcon : TableIcon;
  return <Icon aria-hidden="true" className="source-icon" size={15} weight="regular" />;
}

function previewTheme(): PreviewTheme {
  const value = new URLSearchParams(window.location.search).get("theme");
  return value === "light" || value === "dark" ? value : "system";
}

const preferencesRepository = createWorkbenchPreferencesRepository(
  getWorkbenchPreferences,
  setWorkbenchPreferences,
);

function App() {
  const workspaceRef = useRef<HTMLDivElement>(null);
  const queryWorkspaceRef = useRef<HTMLElement>(null);
  const [runtime, setRuntime] = useState<RuntimeState>({ kind: "loading" });
  const [project, setProject] = useState<ActiveProject | null>(null);
  const [catalog, setCatalog] = useState<ProjectCatalog>({ objects: [], columns: [] });
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);
  const [projectError, setProjectError] = useState<string | null>(null);
  const [preferencesReady, setPreferencesReady] = useState(false);
  const [preferences, setPreferences] = useState<WorkbenchPreferences>(defaultWorkbenchPreferences);
  const { activeOutputPanel: activePanel, bottomPanelOpen: bottomOpen, sidebarOpen } = preferences;

  function updatePreferences(patch: Partial<WorkbenchPreferences>) {
    setPreferences((current) => ({ ...current, ...patch }));
  }

  function resizeSidebar(event: React.PointerEvent<HTMLDivElement>) {
    if (!workspaceRef.current) return;
    const workspace = workspaceRef.current;
    const move = (moveEvent: PointerEvent) => {
      const width = Math.min(360, Math.max(220, moveEvent.clientX));
      workspace.style.setProperty("--sidebar-width", `${width}px`);
      updatePreferences({ sidebarWidth: width });
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
      updatePreferences({ bottomPanelHeight: height });
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

    preferencesRepository
      .load()
      .then((stored) => {
        if (active) setPreferences(stored);
      })
      .catch(() => undefined)
      .finally(() => {
        if (active) setPreferencesReady(true);
      });

    listRecentProjects()
      .then((recent) => {
        if (active) setRecentProjects(recent);
      })
      .catch(() => undefined);

    getActiveProject()
      .then((activeProject) => {
        if (!active || !activeProject) return;
        setProject(activeProject);
        return inspectProjectCatalog().then((projectCatalog) => {
          if (active) setCatalog(projectCatalog);
        });
      })
      .catch(() => undefined);

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

  useEffect(() => {
    if (!preferencesReady) return;
    const timeout = window.setTimeout(() => {
      preferencesRepository.save(preferences).catch(() => undefined);
    }, 250);
    return () => window.clearTimeout(timeout);
  }, [preferences, preferencesReady]);

  useEffect(() => {
    if (!workspaceRef.current || !queryWorkspaceRef.current) return;
    workspaceRef.current.style.setProperty("--sidebar-width", `${preferences.sidebarWidth}px`);
    queryWorkspaceRef.current.style.setProperty(
      "--bottom-height",
      `${preferences.bottomPanelHeight}px`,
    );
  }, [preferences.bottomPanelHeight, preferences.sidebarWidth]);

  async function createLocalProject() {
    const name = window.prompt("Project name", "Local analysis")?.trim();
    if (!name) return;
    setProjectError(null);
    try {
      const activeProject = await createProject(name);
      setProject(activeProject);
      setCatalog(await inspectProjectCatalog());
      setRecentProjects(await listRecentProjects());
    } catch (error) {
      setProjectError(String(error));
    }
  }

  async function openLocalProject() {
    const duckdbPath = await chooseDuckDbFile();
    if (!duckdbPath) return;
    const defaultName = duckdbPath.split(/[\\/]/).pop()?.replace(/\.duckdb$/i, "") || "Local project";
    const name = window.prompt("Project name", defaultName)?.trim();
    if (!name) return;
    setProjectError(null);
    try {
      const activeProject = await openProject(name, duckdbPath);
      setProject(activeProject);
      setCatalog(await inspectProjectCatalog());
      setRecentProjects(await listRecentProjects());
    } catch (error) {
      setProjectError(String(error));
    }
  }

  async function reopenLocalProject(recent: RecentProject) {
    setProjectError(null);
    try {
      const activeProject = await reopenRecentProject(recent.id);
      setProject(activeProject);
      setCatalog(await inspectProjectCatalog());
      setRecentProjects(await listRecentProjects());
    } catch (error) {
      setProjectError(String(error));
    }
  }

  async function closeLocalProject() {
    setProjectError(null);
    try {
      await closeProject();
      setProject(null);
      setCatalog({ objects: [], columns: [] });
      setRecentProjects(await listRecentProjects());
    } catch (error) {
      setProjectError(String(error));
    }
  }

  async function renameLocalProject(recent: RecentProject) {
    const newName = window.prompt("New project name", recent.name)?.trim();
    if (!newName || newName === recent.name) return;
    setProjectError(null);
    try {
      await renameProject(recent.id, newName);
      setProject(null);
      setCatalog({ objects: [], columns: [] });
      setRecentProjects(await listRecentProjects());
    } catch (error) {
      setProjectError(String(error));
    }
  }

  async function removeLocalProject(recent: RecentProject) {
    const action = recent.ownership === "managed" ? "delete" : "forget";
    const detail =
      recent.ownership === "managed"
        ? `This permanently deletes Tarik-managed project \"${recent.name}\" and its directory:\n${recent.duckdbPath}`
        : `This forgets \"${recent.name}\" from Tarik. The external DuckDB file is preserved:\n${recent.duckdbPath}`;
    if (!window.confirm(`${action[0].toUpperCase() + action.slice(1)} project?\n\n${detail}`)) return;
    setProjectError(null);
    try {
      await removeProject(recent.id);
      setProject(null);
      setCatalog({ objects: [], columns: [] });
      setRecentProjects(await listRecentProjects());
    } catch (error) {
      setProjectError(String(error));
    }
  }

  return (
    <main
      className="app-shell"
      data-theme={previewTheme() === "system" ? preferences.theme : previewTheme()}
    >
      <header className="app-header">
        <div className="brand-lockup">
          <button
            aria-label={sidebarOpen ? "Collapse source explorer" : "Expand source explorer"}
            className="icon-button header-menu"
            onClick={() => updatePreferences({ sidebarOpen: !sidebarOpen })}
            type="button"
          >
            <ListIcon aria-hidden="true" size={17} weight="regular" />
          </button>
          <span className="brand-mark" aria-hidden="true">T</span>
          <span className="brand-name">Tarik</span>
        </div>

        <div className="project-context">
          <span className="project-file">{project?.name ?? "No project open"}</span>
          <span className="project-mode">{project ? "Local DuckDB" : "Create or open a project"}</span>
        </div>

        <div className="header-actions">
          <span className="engine-status">
            <span aria-hidden="true" className={`status-mark ${project ? "" : "status-mark-idle"}`} />
            {project ? "DuckDB ready" : "DuckDB idle"}
          </span>
          {project ? (
            <button className="text-button" onClick={closeLocalProject} type="button">Close project</button>
          ) : (
            <>
              <button className="text-button" onClick={createLocalProject} type="button">New project</button>
              <button className="text-button" onClick={openLocalProject} type="button">Open</button>
            </>
          )}
          <Dialog
            description="Choose how Tarik appears on this device. This setting is stored locally."
            title="Appearance"
            trigger={<button className="text-button" type="button">Settings</button>}
          >
            <div aria-label="Theme" className="theme-choices" role="group">
              {(["system", "light", "dark"] as const).map((theme) => (
                <button
                  aria-pressed={preferences.theme === theme}
                  className="theme-choice"
                  key={theme}
                  onClick={() => updatePreferences({ theme })}
                  type="button"
                >
                  <span>{theme[0].toUpperCase() + theme.slice(1)}</span>
                  <small>
                    {theme === "system"
                      ? "Follow the operating system"
                      : `Always use the ${theme} theme`}
                  </small>
                </button>
              ))}
            </div>
          </Dialog>
        </div>
      </header>

      <div ref={workspaceRef} className={`workspace ${sidebarOpen ? "workspace-sidebar-open" : "workspace-sidebar-closed"}`}>
        <aside aria-label="Source explorer" className="source-explorer">
          <div className="panel-heading">
            <div>
              <p className="panel-kicker">Workspace</p>
              <h2>Explorer</h2>
            </div>
            <button aria-label="Refresh catalog" className="icon-button add-button" disabled={!project} onClick={async () => project && setCatalog(await inspectProjectCatalog())} type="button"><PlusIcon aria-hidden="true" size={17} weight="bold" /></button>
          </div>

          <div className="source-tree">
            <div className="tree-section-title">Project catalog</div>
            {project ? (
              <>
                <div className="tree-row tree-row-selected">
                  <SourceIcon kind="database" />
                  <span>{project.name}</span>
                </div>
                {catalog.objects.length === 0 ? (
                  <p className="tree-empty">No tables or views yet.</p>
                ) : (
                  catalog.objects.map((object) => {
                    const count = catalog.columns.filter(
                      (column) =>
                        column.database === object.database &&
                        column.schema === object.schema &&
                        column.object === object.name,
                    ).length;
                    return (
                      <button className="tree-row tree-row-child" key={`${object.database}.${object.schema}.${object.name}`} type="button">
                        <SourceIcon kind="table" />
                        <span title={`${object.schema}.${object.name}`}>{object.name}</span>
                        <span className="row-meta">{object.kind === "view" ? "view" : `${count} cols`}</span>
                      </button>
                    );
                  })
                )}
              </>
            ) : (
              <>
                <div className="catalog-empty">
                  <strong>No local project</strong>
                  <span>Create a project, choose a DuckDB file, or reopen a recent project.</span>
                </div>
                {recentProjects.length > 0 && (
                  <>
                    <div className="tree-section-title tree-section-spaced">Recent projects</div>
                    {recentProjects.map((recent) => (
                      <div className="recent-project" key={recent.id}>
                        <button
                          className="tree-row"
                          onClick={() => reopenLocalProject(recent)}
                          title={recent.duckdbPath}
                          type="button"
                        >
                          <SourceIcon kind="database" />
                          <span>{recent.name}</span>
                          <span className="row-meta">Open</span>
                        </button>
                        <div className="recent-action-row">
                          <span title={recent.duckdbPath}>
                            {recent.ownership === "managed" ? "Managed" : "External"}
                          </span>
                          <button onClick={() => renameLocalProject(recent)} type="button">
                            Rename
                          </button>
                          <button
                            className={recent.ownership === "managed" ? "danger-action" : ""}
                            onClick={() => removeLocalProject(recent)}
                            type="button"
                          >
                            {recent.ownership === "managed" ? "Delete" : "Forget"}
                          </button>
                        </div>
                      </div>
                    ))}
                  </>
                )}
              </>
            )}
            {projectError && <p className="catalog-error" role="alert">{projectError}</p>}
          </div>

          <div className="explorer-footer">
            <button className="footer-action" disabled={!project} type="button">Import file</button>
            <button className="footer-action" disabled={!project} type="button">New table</button>
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
              <Tabs.Root onValueChange={(value) => updatePreferences({ activeOutputPanel: value as Panel })} value={activePanel}>
                <Tabs.List aria-label="Query output" className="result-tabs">
                  <Tabs.Trigger className="result-tab" onClick={() => updatePreferences({ activeOutputPanel: "results" })} value="results">Results <span className="tab-count">24,318</span></Tabs.Trigger>
                  <Tabs.Trigger className="result-tab" onClick={() => updatePreferences({ activeOutputPanel: "flow" })} value="flow">Flow</Tabs.Trigger>
                  <Tabs.Trigger className="result-tab" onClick={() => updatePreferences({ activeOutputPanel: "profile" })} value="profile">Profile</Tabs.Trigger>
                </Tabs.List>
              </Tabs.Root>
              <div className="results-actions">
                <span className="result-duration">Completed in 1.82s</span>
                <button className="toolbar-button" type="button">Export</button>
                <button aria-label={bottomOpen ? "Collapse result panel" : "Expand result panel"} className="icon-button" onClick={() => updatePreferences({ bottomPanelOpen: !bottomOpen })} type="button">{bottomOpen ? <CaretDownIcon aria-hidden="true" size={16} /> : <CaretUpIcon aria-hidden="true" size={16} />}</button>
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
        <span className="status-bar-left"><span className={`status-mark ${project ? "" : "status-mark-idle"}`} aria-hidden="true" /> {project ? `Connected to ${project.name}` : runtime.kind === "ready" ? "No DuckDB project open" : "Starting Tarik"}</span>
        <span className="status-bar-right"><span>Memory limit: Balanced</span><span>2 threads</span><span>UTF-8</span></span>
      </footer>
    </main>
  );
}

export default App;
