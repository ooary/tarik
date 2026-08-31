import { DatabaseIcon, ListIcon, PlusIcon, TableIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import "./App.css";
import { ContextMenu, Dialog } from "./components/ui";
import { QueryWorkspace, type QueryWorkspaceHandle } from "./features/editor/QueryWorkspace";
import { previewTableSql, qualifiedSqlName } from "./features/editor/sqlText";
import { formatCompactCount } from "./features/sources/format";
import { ImportDialog, type SourceAction } from "./features/sources/ImportDialog";
import {
  createWorkbenchPreferencesRepository,
  defaultWorkbenchPreferences,
  type WorkbenchPreferences,
} from "./app/preferences";
import {
  cancelSourceOperation,
  chooseDuckDbFile,
  chooseParquetFile,
  chooseSourceFile,
  closeProject,
  createProject,
  dropCatalogObject,
  releaseAllResults,
  getActiveProject,
  getRuntimeInfo,
  getWorkbenchPreferences,
  importSourceTable,
  inspectProjectCatalog,
  inspectSourceFile,
  linkParquetSource,
  listRecentProjects,
  listSources,
  openProject,
  removeLinkedSource,
  removeProject,
  repairLinkedSource,
  renameProject,
  reopenRecentProject,
  setWorkbenchPreferences,
  type ActiveProject,
  type CsvOptions,
  type ImportOptions,
  type ProjectCatalog,
  type RecentProject,
  type SourceInspection,
  type SourceRecord,
  type RuntimeInfo,
} from "./lib/commands";

type RuntimeState =
  | { kind: "loading" }
  | { kind: "ready"; info: RuntimeInfo }
  | { kind: "unavailable" };

type PreviewTheme = "system" | "light" | "dark";

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
  const queryWorkspaceActionsRef = useRef<QueryWorkspaceHandle>(null);
  const [runtime, setRuntime] = useState<RuntimeState>({ kind: "loading" });
  const [project, setProject] = useState<ActiveProject | null>(null);
  const [catalog, setCatalog] = useState<ProjectCatalog>({ objects: [], columns: [] });
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);
  const [sources, setSources] = useState<SourceRecord[]>([]);
  const [sourceInspection, setSourceInspection] = useState<SourceInspection | null>(null);
  const [sourceBusy, setSourceBusy] = useState(false);
  const [sourceError, setSourceError] = useState<string | null>(null);
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
        return Promise.all([inspectProjectCatalog(), listSources(activeProject.id)]).then(
          ([projectCatalog, projectSources]) => {
            if (active) {
              setCatalog(projectCatalog);
              setSources(projectSources);
            }
          },
        );
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
    workspaceRef.current?.style.setProperty("--sidebar-width", `${preferences.sidebarWidth}px`);
  }, [preferences.sidebarWidth]);

  async function refreshProjectData(activeProject: ActiveProject) {
    const [projectCatalog, projectSources] = await Promise.all([
      inspectProjectCatalog(),
      listSources(activeProject.id),
    ]);
    setCatalog(projectCatalog);
    setSources(projectSources);
  }

  async function createLocalProject() {
    const name = window.prompt("Project name", "Local analysis")?.trim();
    if (!name) return;
    setProjectError(null);
    try {
      const activeProject = await createProject(name);
      setProject(activeProject);
      await refreshProjectData(activeProject);
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
      await refreshProjectData(activeProject);
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
      await refreshProjectData(activeProject);
      setRecentProjects(await listRecentProjects());
    } catch (error) {
      setProjectError(String(error));
    }
  }

  async function closeLocalProject() {
    setProjectError(null);
    try {
      await Promise.resolve(releaseAllResults()).catch(() => undefined);
      await closeProject();
      setProject(null);
      setCatalog({ objects: [], columns: [] });
      setSources([]);
      setRecentProjects(await listRecentProjects());
    } catch (error) {
      setProjectError(String(error));
    }
  }

  async function beginSourceImport() {
    if (!project) return;
    const path = await chooseSourceFile();
    if (!path) return;
    setSourceError(null);
    try {
      setSourceInspection(await inspectSourceFile(path));
    } catch (error) {
      setSourceError(String(error));
    }
  }

  async function reinspectCsv(options: CsvOptions) {
    if (!sourceInspection) return;
    try {
      setSourceInspection(await inspectSourceFile(sourceInspection.path, options));
      setSourceError(null);
    } catch (error) {
      setSourceError(String(error));
    }
  }

  async function submitSource(action: SourceAction, options: ImportOptions) {
    if (!project || !sourceInspection) return;
    setSourceBusy(true);
    setSourceError(null);
    try {
      if (action === "link") {
        await linkParquetSource(sourceInspection.path, options.tableName);
      } else {
        await importSourceTable(sourceInspection.path, options);
      }
      await refreshProjectData(project);
      setSourceInspection(null);
    } catch (error) {
      setSourceError(String(error));
    } finally {
      setSourceBusy(false);
    }
  }

  async function repairSource(source: SourceRecord) {
    if (!project) return;
    const replacement = await chooseParquetFile();
    if (!replacement) return;
    setSourceError(null);
    try {
      await repairLinkedSource(source.id, replacement);
      await refreshProjectData(project);
    } catch (error) {
      setSourceError(String(error));
    }
  }

  async function removeSource(source: SourceRecord) {
    if (!project) return;
    if (!window.confirm(`Remove linked source "${source.displayName}"?\n\nThe Parquet file is preserved.`)) return;
    setSourceError(null);
    try {
      await removeLinkedSource(source.id);
      await refreshProjectData(project);
    } catch (error) {
      setSourceError(String(error));
    }
  }

  async function removeCatalogObject(object: ProjectCatalog["objects"][number]) {
    if (!project) return;
    const sourceMetadata = sources.find((source) => source.duckdbName === object.name);
    if (object.kind === "view" && sourceMetadata?.kind === "linked_parquet") {
      await removeSource(sourceMetadata);
      return;
    }
    const label = object.kind === "table" ? "Delete table" : "Delete view";
    const fileNote = sourceMetadata?.sourcePath
      ? `\n\nThe original source file is preserved:\n${sourceMetadata.sourcePath}`
      : "";
    if (
      !window.confirm(
        `${label} "${object.name}"?\n\nThis permanently removes it from the active DuckDB project.${fileNote}`,
      )
    ) {
      return;
    }
    setSourceError(null);
    try {
      await dropCatalogObject(
        project.id,
        object.database,
        object.schema,
        object.name,
        object.kind,
      );
      await refreshProjectData(project);
    } catch (error) {
      setSourceError(String(error));
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
      setSources([]);
      setRecentProjects(await listRecentProjects());
    } catch (error) {
      setProjectError(String(error));
    }
  }

  async function removeLocalProject(recent: RecentProject) {
    const action = recent.ownership === "managed" ? "delete" : "forget";
    const detail =
      recent.ownership === "managed"
        ? `This permanently deletes Tarik-managed project "${recent.name}" and its directory:\n${recent.duckdbPath}`
        : `This forgets "${recent.name}" from Tarik. The external DuckDB file is preserved:\n${recent.duckdbPath}`;
    if (!window.confirm(`${action[0].toUpperCase() + action.slice(1)} project?\n\n${detail}`)) return;
    setProjectError(null);
    try {
      await removeProject(recent.id);
      setProject(null);
      setCatalog({ objects: [], columns: [] });
      setSources([]);
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
                    const sourceMetadata = sources.find(
                      (source) => source.duckdbName === object.name,
                    );
                    const cachedRows =
                      typeof sourceMetadata?.options.rowCount === "number"
                        ? sourceMetadata.options.rowCount
                        : null;
                    const cachedExact = sourceMetadata?.options.rowCountExact === true;
                    const rowCount = cachedRows ?? object.estimatedRowCount;
                    const rowLabel =
                      rowCount === null
                        ? null
                        : `${cachedExact ? "" : "~"}${formatCompactCount(rowCount)} rows`;
                    const exactTitle =
                      rowCount === null
                        ? undefined
                        : `${cachedExact ? "Exact cached" : "Estimated"}: ${rowCount.toLocaleString()} rows`;
                    const qualifiedName = qualifiedSqlName(object.schema, object.name);
                    return (
                      <ContextMenu
                        items={[
                          {
                            label: "Insert name",
                            onSelect: () => queryWorkspaceActionsRef.current?.insertSql(qualifiedName),
                          },
                          {
                            label: "Preview rows",
                            onSelect: () =>
                              queryWorkspaceActionsRef.current?.openPreview(
                                previewTableSql(object.schema, object.name),
                              ),
                          },
                          {
                            label: "Copy qualified name",
                            onSelect: () => void navigator.clipboard?.writeText(qualifiedName),
                          },
                          {
                            danger: object.kind === "table",
                            label:
                              object.kind === "view" && sourceMetadata?.kind === "linked_parquet"
                                ? "Remove link"
                                : object.kind === "table"
                                  ? "Delete table"
                                  : "Delete view",
                            onSelect: () => removeCatalogObject(object),
                          },
                        ]}
                        key={`${object.database}.${object.schema}.${object.name}`}
                        label={`${object.name} table actions`}
                      >
                        <button
                          className="tree-row tree-row-child catalog-object-row"
                          type="button"
                        >
                          <SourceIcon kind="table" />
                          <span title={`${object.schema}.${object.name}`}>{object.name}</span>
                          <span className="row-meta-group">
                            <span>{object.kind === "view" ? "view" : `${count} cols`}</span>
                            {rowLabel && <span title={exactTitle}>{rowLabel}</span>}
                          </span>
                        </button>
                      </ContextMenu>
                    );
                  })
                )}
                {sources.filter((source) => source.kind === "linked_parquet").length > 0 && (
                  <>
                    <div className="tree-section-title tree-section-spaced">Linked sources</div>
                    {sources
                      .filter((source) => source.kind === "linked_parquet")
                      .map((source) => (
                        <ContextMenu
                          items={[
                            {
                              disabled: source.state !== "missing",
                              label: "Locate replacement",
                              onSelect: () => repairSource(source),
                            },
                            {
                              danger: false,
                              label: "Remove link",
                              onSelect: () => removeSource(source),
                            },
                          ]}
                          key={source.id}
                          label={`${source.displayName} source actions`}
                        >
                          <button
                            className={`tree-row ${source.state === "missing" ? "source-row-missing" : ""}`}
                            onClick={() => source.state === "missing" && repairSource(source)}
                            title={source.sourcePath ?? source.displayName}
                            type="button"
                          >
                            <SourceIcon kind="table" />
                            <span>{source.displayName}</span>
                            <span className="row-meta">
                              {source.state === "missing" ? "Missing" : "Linked"}
                            </span>
                          </button>
                        </ContextMenu>
                      ))}
                  </>
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
                      <ContextMenu
                        items={[
                          { label: "Open project", onSelect: () => reopenLocalProject(recent) },
                          { label: "Rename", onSelect: () => renameLocalProject(recent) },
                          {
                            danger: recent.ownership === "managed",
                            label: recent.ownership === "managed" ? "Delete project" : "Forget project",
                            onSelect: () => removeLocalProject(recent),
                          },
                        ]}
                        key={recent.id}
                        label={`${recent.name} actions`}
                      >
                        <button
                          className="tree-row recent-project-row"
                          onClick={() => reopenLocalProject(recent)}
                          title={`${recent.duckdbPath}\nRight-click for project actions`}
                          type="button"
                        >
                          <SourceIcon kind="database" />
                          <span>{recent.name}</span>
                          <span className="row-meta">
                            {recent.ownership === "managed" ? "Managed" : "External"}
                          </span>
                        </button>
                      </ContextMenu>
                    ))}
                  </>
                )}
              </>
            )}
            {projectError && <p className="catalog-error" role="alert">{projectError}</p>}
            {sourceError && !sourceInspection && (
              <p className="catalog-error" role="alert">{sourceError}</p>
            )}
          </div>

          <div className="explorer-footer">
            <button className="footer-action" disabled={!project} onClick={beginSourceImport} type="button">
              Import file
            </button>
            <button className="footer-action" disabled={!project} type="button">New table</button>
          </div>
        </aside>

        <div aria-hidden="true" className="sidebar-resize-handle" onPointerDown={resizeSidebar} />
        <QueryWorkspace
          activePanel={activePanel}
          bottomOpen={bottomOpen}
          bottomPanelHeight={preferences.bottomPanelHeight}
          catalog={catalog}
          key={project?.id ?? "no-project"}
          onQuerySucceeded={() => (project ? refreshProjectData(project) : undefined)}
          onSetBottomHeight={(height) => updatePreferences({ bottomPanelHeight: height })}
          onToggleBottom={() => updatePreferences({ bottomPanelOpen: !bottomOpen })}
          onUpdatePanel={(panel) => updatePreferences({ activeOutputPanel: panel })}
          projectId={project?.id ?? ""}
          ref={queryWorkspaceActionsRef}
        />
      </div>

      {sourceInspection && (
        <ImportDialog
          key={`${sourceInspection.path}:${sourceInspection.csvOptions?.delimiter ?? "parquet"}:${sourceInspection.csvOptions?.hasHeader ?? true}:${sourceInspection.csvOptions?.allVarchar ?? false}:${sourceInspection.csvOptions?.nullValue ?? ""}`}
          busy={sourceBusy}
          error={sourceError}
          inspection={sourceInspection}
          onCancel={() => cancelSourceOperation()}
          onClose={() => {
            setSourceInspection(null);
            setSourceError(null);
          }}
          onInspectCsv={reinspectCsv}
          onSubmit={submitSource}
        />
      )}

      <footer className="status-bar">
        <span className="status-bar-left"><span className={`status-mark ${project ? "" : "status-mark-idle"}`} aria-hidden="true" /> {project ? `Connected to ${project.name}` : runtime.kind === "ready" ? "No DuckDB project open" : "Starting Tarik"}</span>
        <span className="status-bar-right"><span>Memory limit: Balanced</span><span>2 threads</span><span>UTF-8</span></span>
      </footer>
    </main>
  );
}

export default App;
