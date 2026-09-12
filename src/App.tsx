import {
  DatabaseIcon,
  DotsThreeIcon,
  FolderOpenIcon,
  ListChecksIcon,
  ListIcon,
  TableIcon,
} from "@phosphor-icons/react";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import tarikLogo from "./assets/tarik-logo.png";
import "./App.css";
import {
  ConfirmationDialog,
  ContextMenu,
  Dialog,
  Menu,
  TextEntryDialog,
} from "./components/ui";
import { QueryWorkspace, type QueryWorkspaceHandle } from "./features/editor/QueryWorkspace";
import { NewProjectDialog } from "./features/projects/NewProjectDialog";
import { previewTableSql, qualifiedSqlName } from "./features/editor/sqlText";
import { formatCompactCount } from "./features/sources/format";
import { ImportDialog, type SourceAction } from "./features/sources/ImportDialog";
import { NewTableDialog } from "./features/sources/NewTableDialog";
import { EngineResourcesDialog } from "./features/settings/EngineResourcesDialog";
import {
  ProfileWorkspace,
  type ProfileCheckPrefill,
} from "./features/profile/ProfileWorkspace";
import { ChecksWorkspace } from "./features/quality/ChecksWorkspace";
import { SupportIncidentNotice } from "./app/SupportIncidentNotice";
import { AgentAccessDialog } from "./features/agent-access/AgentAccessDialog";
import { StartupScreen, type StartupPhase } from "./app/StartupScreen";
import {
  createWorkbenchPreferencesRepository,
  defaultWorkbenchPreferences,
  type WorkbenchPreferences,
} from "./app/preferences";
import { useEffectiveTheme } from "./app/useEffectiveTheme";
import {
  cancelSourceOperation,
  clearCache,
  chooseDuckDbFile,
  chooseParquetFile,
  chooseSourceFile,
  closeProject,
  completeShutdown,
  createProject,
  dropCatalogObject,
  releaseAllResults,
  getActiveProject,
  getEngineStatus,
  getRuntimeInfo,
  getStartupStatus,
  getLastSupportIncident,
  getLogInfo,
  getWorkbenchPreferences,
  importSourceTable,
  inspectProjectCatalog,
  inspectSourceFile,
  linkParquetSource,
  listRecentProjects,
  listSources,
  openProject,
  removeLinkedSource,
  registerShutdownReady,
  removeProject,
  repairLinkedSource,
  renameProject,
  reopenRecentProject,
  revealLogDirectory,
  setWorkbenchPreferences,
  type ActiveProject,
  type CsvOptions,
  type ImportOptions,
  type ProjectCatalog,
  type RecentProject,
  type SourceInspection,
  type SourceRecord,
  type RuntimeInfo,
  type LogInfo,
  type SupportIncident,
} from "./lib/commands";

type RuntimeState =
  | { kind: "loading" }
  | { kind: "ready"; info: RuntimeInfo }
  | { kind: "unavailable" };

type PreviewTheme = "system" | "light" | "dark";
type EngineConnectionState =
  | "idle"
  | "connecting"
  | "standby"
  | "recovering"
  | "connected"
  | "failed";

type AppTextIntent =
  | { kind: "open-project"; duckdbPath: string; value: string }
  | { kind: "rename-project"; project: RecentProject; value: string };

type ProfileIntent = {
  projectId: string;
  object: ProjectCatalog["objects"][number];
  source: SourceRecord | null;
  catalogRevision: string;
};

type AppConfirmIntent =
  | { kind: "remove-source"; projectId: string; source: SourceRecord }
  | {
      kind: "drop-object";
      projectId: string;
      object: ProjectCatalog["objects"][number];
      source: SourceRecord | null;
    }
  | { kind: "remove-project"; project: RecentProject };

function SourceIcon({ kind }: { kind: "database" | "table" }) {
  const Icon = kind === "database" ? DatabaseIcon : TableIcon;
  return <Icon aria-hidden="true" className="source-icon" size={15} weight="regular" />;
}

function previewTheme(): PreviewTheme {
  const value = new URLSearchParams(window.location.search).get("theme");
  return value === "light" || value === "dark" ? value : "system";
}

function engineStatusLabel(state: EngineConnectionState): string {
  if (state === "connected") return "DuckDB ready";
  if (state === "connecting") return "Connecting to DuckDB";
  if (state === "standby") return "DuckDB standby";
  if (state === "recovering") return "DuckDB will reconnect";
  if (state === "failed") return "DuckDB unavailable";
  return "DuckDB stopped";
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
  const [engineState, setEngineState] = useState<EngineConnectionState>("idle");
  const [catalog, setCatalog] = useState<ProjectCatalog>({ objects: [], columns: [] });
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);
  const [sources, setSources] = useState<SourceRecord[]>([]);
  const [sourceInspection, setSourceInspection] = useState<SourceInspection | null>(null);
  const [sourceBusy, setSourceBusy] = useState(false);
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [projectError, setProjectError] = useState<string | null>(null);
  const [preferencesReady, setPreferencesReady] = useState(false);
  const [startupPhase, setStartupPhase] = useState<StartupPhase>("storage");
  const [startupReady, setStartupReady] = useState(false);
  const [logInfo, setLogInfo] = useState<LogInfo | null>(null);
  const [supportIncident, setSupportIncident] = useState<SupportIncident | null>(null);
  const [cacheStatus, setCacheStatus] = useState<string | null>(null);
  const [clearingCache, setClearingCache] = useState(false);
  const [shutdownError, setShutdownError] = useState<string | null>(null);
  const [textIntent, setTextIntent] = useState<AppTextIntent | null>(null);
  const [confirmIntent, setConfirmIntent] = useState<AppConfirmIntent | null>(null);
  const [interactionBusy, setInteractionBusy] = useState(false);
  const [interactionError, setInteractionError] = useState<string | null>(null);
  const [profileIntent, setProfileIntent] = useState<ProfileIntent | null>(null);
  const [checksOpen, setChecksOpen] = useState(false);
  const [profileHandoff, setProfileHandoff] = useState<ProfileCheckPrefill | null>(null);
  const profileTriggerRef = useRef<HTMLElement | null>(null);
  const shutdownInFlight = useRef(false);
  const [preferences, setPreferences] = useState<WorkbenchPreferences>(defaultWorkbenchPreferences);
  const { bottomPanelOpen: bottomOpen, sidebarOpen } = preferences;
  const selectedTheme = previewTheme() === "system" ? preferences.theme : previewTheme();
  const effectiveTheme = useEffectiveTheme(selectedTheme);
  const resourceStatusKey = `${engineState}:${project?.id ?? "none"}`;

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
    const suppressNativeContextMenu = (event: MouseEvent) => event.preventDefault();
    document.addEventListener("contextmenu", suppressNativeContextMenu);
    return () => document.removeEventListener("contextmenu", suppressNativeContextMenu);
  }, []);

  useEffect(() => {
    let active = true;

    async function initialize() {
      setStartupPhase("storage");
      const storageTask = waitForStartupStorage(() => active);
      const runtimeTask = getRuntimeInfo()
        .then((info) => {
          if (active) setRuntime({ kind: "ready", info });
        })
        .catch(() => {
          if (active) setRuntime({ kind: "unavailable" });
        });
      const diagnosticsTask = Promise.allSettled([
        getLogInfo().then((info) => {
          if (active) setLogInfo(info);
        }),
        getLastSupportIncident().then((incident) => {
          if (active && incident) setSupportIncident(incident);
        }),
      ]);

      await storageTask;
      if (!active) return;

      setStartupPhase("preferences");
      const preferencesTask = preferencesRepository
        .load()
        .then((stored) => {
          if (active) setPreferences(stored);
        })
        .catch(() => undefined)
        .finally(() => {
          if (active) setPreferencesReady(true);
        });

      await preferencesTask;
      if (!active) return;

      setStartupPhase("projects");
      const recentTask = listRecentProjects()
        .then((recent) => {
          if (active) setRecentProjects(recent);
        })
        .catch(() => undefined);
      const activeProjectTask = getActiveProject()
        .then(async (activeProject) => {
          if (!active || !activeProject) return;
          setProject(activeProject);
          setEngineState("connected");
          const [projectCatalog, projectSources] = await Promise.all([
            inspectProjectCatalog(),
            listSources(activeProject.id),
          ]);
          if (active) {
            setCatalog(projectCatalog);
            setSources(projectSources);
          }
        })
        .catch(() => undefined);

      await Promise.allSettled([recentTask, activeProjectTask]);
      if (!active) return;

      setStartupPhase("workspace");
      await Promise.allSettled([runtimeTask, diagnosticsTask]);
      if (!active) return;
      setStartupPhase("ready");
      setStartupReady(true);
    }

    void initialize();
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen<SupportIncident>("support-incident", (event) => {
      if (!disposed) setSupportIncident(event.payload);
    })
      .then((remove) => {
        if (disposed) remove();
        else unlisten = remove;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    const refreshEngineStatus = () => {
      getEngineStatus()
        .then((status) => {
          if (disposed) return;
          setEngineState((current) => {
            if (status.state === "connected") return "connected";
            if (status.state === "standby") return "standby";
            if (status.state === "failed") return "failed";
            if (project) return "recovering";
            if (current === "connecting" || current === "failed") return current;
            return "idle";
          });
        })
        .catch(() => {
          if (!disposed && project) setEngineState("recovering");
        });
    };
    refreshEngineStatus();
    const interval = window.setInterval(refreshEngineStatus, 1_000);
    return () => {
      disposed = true;
      window.clearInterval(interval);
    };
  }, [project]);

  const finishShutdown = useCallback(
    async (skipDraft = false) => {
      if (shutdownInFlight.current) return;
      shutdownInFlight.current = true;
      setShutdownError(null);
      try {
        if (!skipDraft) {
          await queryWorkspaceActionsRef.current?.flushDraft();
          await preferencesRepository.save(preferences);
        }
        await completeShutdown(skipDraft);
      } catch (error) {
        setShutdownError(String(error));
        shutdownInFlight.current = false;
      }
    },
    [preferences],
  );

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    registerShutdownReady()
      .then(() => listen("shutdown-requested", () => void finishShutdown(false)))
      .then((remove) => {
        if (disposed) remove();
        else unlisten = remove;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [finishShutdown]);

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

  async function clearTemporaryCache() {
    setClearingCache(true);
    setCacheStatus(null);
    try {
      const summary = await clearCache();
      const removed = summary.artifactsRemoved.toLocaleString("en-US");
      setCacheStatus(
        summary.warnings.length > 0
          ? `Removed ${removed} temporary artifacts with ${summary.warnings.length.toLocaleString("en-US")} warning(s).`
          : `Removed ${removed} temporary artifacts.`,
      );
    } catch (error) {
      setCacheStatus(`Cache cleanup failed: ${String(error)}`);
    } finally {
      setClearingCache(false);
    }
  }

  async function createLocalProject(name: string) {
    setProjectError(null);
    setEngineState("connecting");
    try {
      const activeProject = await createProject(name);
      setProject(activeProject);
      setEngineState("connected");
      await refreshProjectData(activeProject);
      setRecentProjects(await listRecentProjects());
    } catch (error) {
      setEngineState("failed");
      setProjectError(String(error));
      throw error;
    }
  }

  async function openLocalProject() {
    const duckdbPath = await chooseDuckDbFile();
    if (!duckdbPath) return;
    const defaultName =
      duckdbPath.split(/[\\/]/).pop()?.replace(/\.duckdb$/i, "") || "Local project";
    setInteractionError(null);
    setTextIntent({ kind: "open-project", duckdbPath, value: defaultName });
  }

  async function submitTextIntent() {
    if (!textIntent || interactionBusy) return;
    const intent = textIntent;
    const value = intent.value.trim();
    if (!value) return;
    setInteractionBusy(true);
    setInteractionError(null);
    try {
      if (intent.kind === "open-project") {
        setProjectError(null);
        setEngineState("connecting");
        const activeProject = await openProject(value, intent.duckdbPath);
        setProject(activeProject);
        setEngineState("connected");
        await refreshProjectData(activeProject);
        setRecentProjects(await listRecentProjects());
      } else {
        if (!recentProjects.some((recent) => recent.id === intent.project.id)) {
          throw new Error("project.stale: This recent project is no longer available.");
        }
        if (value === intent.project.name) {
          setTextIntent(null);
          return;
        }
        setProjectError(null);
        await renameProject(intent.project.id, value);
        setProject(null);
        setCatalog({ objects: [], columns: [] });
        setSources([]);
        setRecentProjects(await listRecentProjects());
      }
      setTextIntent(null);
    } catch (error) {
      if (intent.kind === "open-project") setEngineState("failed");
      setInteractionError(String(error));
    } finally {
      setInteractionBusy(false);
    }
  }

  async function reopenLocalProject(recent: RecentProject) {
    setProjectError(null);
    setEngineState("connecting");
    try {
      const activeProject = await reopenRecentProject(recent.id);
      setProject(activeProject);
      setEngineState("connected");
      await refreshProjectData(activeProject);
      setRecentProjects(await listRecentProjects());
    } catch (error) {
      setEngineState("failed");
      setProjectError(String(error));
    }
  }

  async function closeLocalProject() {
    setProjectError(null);
    try {
      await Promise.resolve(releaseAllResults()).catch(() => undefined);
      await closeProject();
      setProject(null);
      setEngineState("idle");
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

  function removeSource(source: SourceRecord) {
    if (!project) return;
    setInteractionError(null);
    setConfirmIntent({ kind: "remove-source", projectId: project.id, source });
  }

  function openProfile(
    object: ProjectCatalog["objects"][number],
    trigger: HTMLElement | null = null,
  ) {
    if (!project || !catalog.revision) return;
    const source = sourceForCatalogObject(sources, catalog.objects, object);
    const rowTrigger = trigger?.closest<HTMLElement>(".catalog-object-row");
    if (rowTrigger) profileTriggerRef.current = rowTrigger;
    setProfileHandoff(null);
    setChecksOpen(false);
    setProfileIntent({
      projectId: project.id,
      object: { ...object },
      source: source ? { ...source, options: { ...source.options } } : null,
      catalogRevision: catalog.revision,
    });
  }

  function closeProfile() {
    setProfileIntent(null);
    setProfileHandoff(null);
    window.requestAnimationFrame(() => profileTriggerRef.current?.focus());
  }

  async function refreshProfileSetup() {
    if (!project || !profileIntent) return;
    const [nextCatalog, nextSources] = await Promise.all([
      inspectProjectCatalog(),
      listSources(project.id),
    ]);
    const nextObject = nextCatalog.objects.find(
      (candidate) =>
        candidate.database === profileIntent.object.database &&
        candidate.schema === profileIntent.object.schema &&
        candidate.name === profileIntent.object.name &&
        candidate.kind === profileIntent.object.kind,
    );
    setCatalog(nextCatalog);
    setSources(nextSources);
    if (!nextObject || !nextCatalog.revision) {
      throw new Error("catalog.stale: This table or view is no longer available.");
    }
    const nextSource = sourceForCatalogObject(nextSources, nextCatalog.objects, nextObject);
    setProfileIntent({
      projectId: project.id,
      object: { ...nextObject },
      source: nextSource ? { ...nextSource, options: { ...nextSource.options } } : null,
      catalogRevision: nextCatalog.revision,
    });
  }

  function beginProfileCheck(prefill: ProfileCheckPrefill) {
    setProfileHandoff(prefill);
    setProfileIntent(null);
    setChecksOpen(true);
  }

  function openProfileSql(sql: string, title: string) {
    setProfileIntent(null);
    setProfileHandoff(null);
    queryWorkspaceActionsRef.current?.openPreview(sql, title);
  }

  function closeChecks() {
    setChecksOpen(false);
    setProfileHandoff(null);
  }

  function profileFromChecks(target: {
    database: string;
    schema: string;
    object: string;
  }) {
    const object = catalog.objects.find(
      (candidate) =>
        candidate.database === target.database &&
        candidate.schema === target.schema &&
        candidate.name === target.object,
    );
    if (!object) return;
    setChecksOpen(false);
    openProfile(object);
  }

  async function repairSourceFromChecks(target: {
    database: string;
    schema: string;
    object: string;
  }) {
    const object = catalog.objects.find(
      (candidate) =>
        candidate.database === target.database &&
        candidate.schema === target.schema &&
        candidate.name === target.object,
    );
    if (!object) {
      throw new Error("quality.catalog_stale: The target is no longer in the catalog.");
    }
    const source = sourceForCatalogObject(sources, catalog.objects, object);
    if (!source || source.kind !== "linked_parquet") {
      throw new Error("quality.source_not_repairable: No linked Parquet source is recorded for this target.");
    }
    await repairSource(source);
  }

  function openGeneratedSql(sql: string, title: string) {
    setChecksOpen(false);
    setProfileHandoff(null);
    queryWorkspaceActionsRef.current?.openPreview(sql, title);
  }

  function removeCatalogObject(object: ProjectCatalog["objects"][number]) {
    if (!project) return;
    const sourceMetadata = sourceForCatalogObject(sources, catalog.objects, object);
    if (object.kind === "view" && sourceMetadata?.kind === "linked_parquet") {
      removeSource(sourceMetadata);
      return;
    }
    setInteractionError(null);
    setConfirmIntent({
      kind: "drop-object",
      projectId: project.id,
      object,
      source: sourceMetadata,
    });
  }

  function renameLocalProject(recent: RecentProject) {
    setInteractionError(null);
    setTextIntent({ kind: "rename-project", project: recent, value: recent.name });
  }

  function removeLocalProject(recent: RecentProject) {
    setInteractionError(null);
    setConfirmIntent({ kind: "remove-project", project: recent });
  }

  async function submitConfirmation() {
    if (!confirmIntent || interactionBusy) return;
    const intent = confirmIntent;
    setInteractionBusy(true);
    setInteractionError(null);
    try {
      if (intent.kind === "remove-source") {
        if (
          project?.id !== intent.projectId ||
          !sources.some((source) => source.id === intent.source.id)
        ) {
          throw new Error("source.stale: This linked source is no longer available.");
        }
        await removeLinkedSource(intent.source.id);
        await refreshProjectData(project);
      } else if (intent.kind === "drop-object") {
        if (
          project?.id !== intent.projectId ||
          !catalog.objects.some(
            (object) =>
              object.database === intent.object.database &&
              object.schema === intent.object.schema &&
              object.name === intent.object.name &&
              object.kind === intent.object.kind,
          )
        ) {
          throw new Error("catalog.stale: This catalog object is no longer available.");
        }
        await dropCatalogObject(
          project.id,
          intent.object.database,
          intent.object.schema,
          intent.object.name,
          intent.object.kind,
        );
        await refreshProjectData(project);
      } else {
        if (!recentProjects.some((recent) => recent.id === intent.project.id)) {
          throw new Error("project.stale: This recent project is no longer available.");
        }
        setProjectError(null);
        await removeProject(intent.project.id);
        setProject(null);
        setCatalog({ objects: [], columns: [] });
        setSources([]);
        setRecentProjects(await listRecentProjects());
      }
      setConfirmIntent(null);
    } catch (error) {
      setInteractionError(String(error));
    } finally {
      setInteractionBusy(false);
    }
  }

  return (
    <>
      {!startupReady && <StartupScreen phase={startupPhase} />}
      <main
      className="app-shell"
      data-effective-theme={effectiveTheme}
      data-theme={selectedTheme}
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
          <img alt="" aria-hidden="true" className="brand-logo" src={tarikLogo} />
          <span className="brand-name">Tarik</span>
        </div>

        <div className="project-context">
          <span className="project-file">{project?.name ?? "No project open"}</span>
          <span className="project-mode">{project ? "Local DuckDB" : "Create or open a project"}</span>
        </div>

        <div className="header-actions">
          <span className="engine-status">
            <span aria-hidden="true" className={`status-mark status-mark-${engineState}`} />
            {engineStatusLabel(engineState)}
          </span>
          {project ? (
            <button className="text-button project-close-button" onClick={closeLocalProject} type="button">Close project</button>
          ) : (
            <>
              <NewProjectDialog
                existingNames={recentProjects.map((recent) => recent.name)}
                onCreate={createLocalProject}
              />
              <button className="text-button" onClick={openLocalProject} type="button">Open</button>
            </>
          )}
          <AgentAccessDialog project={project} />
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
            <section className="settings-diagnostics" aria-label="Support diagnostics">
              <div>
                <strong>Support logs</strong>
                <span>
                  {logInfo
                    ? `${logInfo.retainedFiles} files, up to ${Math.round(logInfo.maxFileBytes / 1024 / 1024)} MiB each`
                    : "Log location unavailable"}
                </span>
              </div>
              <button
                className="text-button"
                disabled={!logInfo}
                onClick={() => logInfo && void revealLogDirectory()}
                type="button"
              >
                <FolderOpenIcon aria-hidden="true" size={14} />
                Reveal logs
              </button>
            </section>
            <section className="settings-diagnostics" aria-label="Temporary cache">
              <div>
                <strong>Temporary result cache</strong>
                <span>Clears result pages only; completed exports are preserved.</span>
                {cacheStatus && <span role="status">{cacheStatus}</span>}
              </div>
              <button
                className="text-button"
                disabled={clearingCache}
                onClick={() => void clearTemporaryCache()}
                type="button"
              >
                {clearingCache ? "Clearing" : "Clear cache"}
              </button>
            </section>
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

          </div>

          <div className="source-tree" role="tree">
            <div className="tree-section-title">Project catalog</div>
            {project ? (
              <>
                <div className="tree-row tree-row-selected">
                  <SourceIcon kind="database" />
                  <span>{project.name}</span>
                </div>
                <button
                  className={`tree-row project-tool-row ${checksOpen ? "project-tool-row-active" : ""}`}
                  onClick={() => {
                    setProfileIntent(null);
                    setChecksOpen(true);
                  }}
                  type="button"
                >
                  <ListChecksIcon aria-hidden="true" className="source-icon" size={15} />
                  <span>Quality checks</span>
                  <span className="row-meta">Define and run</span>
                </button>
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
                    const sourceMetadata = sourceForCatalogObject(
                      sources,
                      catalog.objects,
                      object,
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
                        : `${cachedExact ? "Exact cached" : "Estimated"}: ${rowCount.toLocaleString("en-US")} rows`;
                    const qualifiedName = qualifiedSqlName(object.schema, object.name);
                    const items = [
                      {
                        label: "Insert name",
                        onSelect: () => queryWorkspaceActionsRef.current?.insertSql(qualifiedName),
                      },
                      {
                        label: "Profile data",
                        disabled: !canProfileCatalogObject(catalog.revision, sourceMetadata),
                        onSelect: () => openProfile(object),
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
                        label:
                          object.kind === "view" && sourceMetadata?.kind === "linked_parquet"
                            ? "Remove link"
                            : object.kind === "table"
                              ? "Delete table"
                              : "Delete view",
                        onSelect: () => removeCatalogObject(object),
                      },
                    ];
                    return (
                      <ContextMenu
                        items={items}
                        key={`${object.database}.${object.schema}.${object.name}`}
                        label={`${object.name} catalog actions`}
                      >
                        <div
                          className="tree-row tree-row-child catalog-object-row"
                          onContextMenu={(event) => {
                            profileTriggerRef.current = event.currentTarget;
                          }}
                          onFocusCapture={(event) => {
                            profileTriggerRef.current = event.currentTarget;
                          }}
                          onKeyDown={(event) => {
                            if (event.altKey && event.key.toLowerCase() === "p") {
                              event.preventDefault();
                              if (canProfileCatalogObject(catalog.revision, sourceMetadata)) {
                                openProfile(object, event.currentTarget);
                              }
                            }
                          }}
                          aria-keyshortcuts="Alt+P"
                          aria-label={`${object.name} ${object.kind}`}
                          role="treeitem"
                          tabIndex={0}
                        >
                        <SourceIcon kind="table" />
                        <span title={`${object.schema}.${object.name}`}>{object.name}</span>
                        <span className="row-meta-group">
                          <span>{object.kind === "view" ? "view" : `${count} cols`}</span>
                          {rowLabel && <span title={exactTitle}>{rowLabel}</span>}
                        </span>
                        <Menu
                          items={items}
                          label={`${object.name} table actions`}
                          trigger={
                            <button className="icon-button tree-row-actions" type="button">
                              <DotsThreeIcon aria-hidden="true" size={15} weight="bold" />
                            </button>
                          }
                        />
                        </div>
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
                        <div
                          className={`tree-row ${source.state === "missing" ? "source-row-missing" : ""}`}
                          key={source.id}
                          title={source.sourcePath ?? source.displayName}
                        >
                          <SourceIcon kind="table" />
                          <span>{source.displayName}</span>
                          <span className="row-meta">
                            {source.state === "missing" ? "Missing" : "Linked"}
                          </span>
                          <Menu
                            items={[
                              {
                                disabled: source.state !== "missing",
                                label: "Locate replacement",
                                onSelect: () => repairSource(source),
                              },
                              { label: "Remove link", onSelect: () => removeSource(source) },
                            ]}
                            label={`${source.displayName} source actions`}
                            trigger={
                              <button className="icon-button tree-row-actions" type="button">
                                <DotsThreeIcon aria-hidden="true" size={15} weight="bold" />
                              </button>
                            }
                          />
                        </div>
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
            <NewTableDialog
              onCreated={() => (project ? refreshProjectData(project) : undefined)}
              projectId={project?.id ?? ""}
            />
          </div>
        </aside>

        <div aria-hidden="true" className="sidebar-resize-handle" onPointerDown={resizeSidebar} />
        {checksOpen && project ? (
          <ChecksWorkspace
            catalog={catalog}
            onClose={closeChecks}
            onOpenSql={openGeneratedSql}
            onProfileTarget={profileFromChecks}
            onRepairTarget={repairSourceFromChecks}
            prefill={profileHandoff}
            project={project}
          />
        ) : profileIntent && project?.id === profileIntent.projectId ? (
          <ProfileWorkspace
            catalog={catalog}
            object={profileIntent.object}
            onClose={closeProfile}
            onCreateCheck={beginProfileCheck}
            onOpenSql={openProfileSql}
            onRefresh={refreshProfileSetup}
            openedCatalogRevision={profileIntent.catalogRevision}
            project={project}
            source={profileIntent.source}
            sourceChanged={!sameSourceIdentity(
              sourceForCatalogObject(sources, catalog.objects, profileIntent.object),
              profileIntent.source,
            )}
          />
        ) : null}
        <QueryWorkspace
          bottomOpen={bottomOpen}
          bottomPanelHeight={preferences.bottomPanelHeight}
          catalog={catalog}
          key={project?.id ?? "no-project"}
          onQuerySucceeded={() => (project ? refreshProjectData(project) : undefined)}
          onSetBottomHeight={(height) => updatePreferences({ bottomPanelHeight: height })}
          onToggleBottom={() => updatePreferences({ bottomPanelOpen: !bottomOpen })}
          effectiveTheme={effectiveTheme}
          hidden={Boolean(
            checksOpen || (profileIntent && project?.id === profileIntent.projectId),
          )}
          projectId={project?.id ?? ""}
          ref={queryWorkspaceActionsRef}
        />
      </div>

      {textIntent && (
        <TextEntryDialog
          busy={interactionBusy}
          description={
            textIntent.kind === "open-project"
              ? "Choose how this existing DuckDB file appears in Tarik."
              : `Choose a new local name for “${textIntent.project.name}”.`
          }
          hint={textIntent.kind === "open-project" ? textIntent.duckdbPath : undefined}
          label="Project name"
          onOpenChange={(open) => {
            if (!open) {
              setTextIntent(null);
              setInteractionError(null);
            }
          }}
          onSubmit={submitTextIntent}
          onValueChange={(value) =>
            setTextIntent((current) => (current ? { ...current, value } : current))
          }
          open
          operationError={interactionError}
          submitLabel={textIntent.kind === "open-project" ? "Open project" : "Rename project"}
          title={textIntent.kind === "open-project" ? "Name this project" : "Rename project"}
          value={textIntent.value}
        />
      )}

      {confirmIntent && (
        <ConfirmationDialog
          busy={interactionBusy}
          confirmLabel={appConfirmationLabel(confirmIntent)}
          description={appConfirmationDescription(confirmIntent)}
          detail={appConfirmationDetail(confirmIntent)}
          error={interactionError}
          onConfirm={submitConfirmation}
          onOpenChange={(open) => {
            if (!open) {
              setConfirmIntent(null);
              setInteractionError(null);
            }
          }}
          open
          title={`${appConfirmationLabel(confirmIntent)}?`}
          tone="destructive"
        />
      )}

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

      {profileHandoff && !checksOpen && (
        <aside aria-label="Prefilled quality check" className="profile-handoff" role="status">
          <div>
            <strong>Check draft ready for review</strong>
            <span>
              {profileHandoff.draft.name} · {profileHandoff.draft.options.kind.split("_").join(" ")}
            </span>
            <small>
              Nothing was saved or run. The guided Checks builder will require review before either
              action.
            </small>
          </div>
          <button
            aria-label="Dismiss profile check handoff"
            className="icon-button"
            onClick={() => setProfileHandoff(null)}
            type="button"
          >
            ×
          </button>
        </aside>
      )}

      {shutdownError && (
        <div className="shutdown-recovery" role="alertdialog" aria-label="Draft could not be saved">
          <strong>Tarik could not save the latest draft</strong>
          <p>Keep the window open and retry, or quit knowing the latest unsaved changes may be lost.</p>
          <code>{shutdownError}</code>
          <div>
            <button className="text-button" onClick={() => void finishShutdown(false)} type="button">
              Retry save
            </button>
            <button className="text-button" onClick={() => void finishShutdown(true)} type="button">
              Quit without latest changes
            </button>
          </div>
        </div>
      )}

      {supportIncident && (
        <div className="support-incident-dock">
          <SupportIncidentNotice
            incident={supportIncident}
            onDismiss={() => setSupportIncident(null)}
          />
        </div>
      )}

      <footer className="status-bar">
        <span className="status-bar-left"><span className={`status-mark status-mark-${engineState}`} aria-hidden="true" /> {engineState === "connected" && project ? `Connected to ${project.name}` : engineState === "connecting" ? "Connecting to DuckDB" : engineState === "standby" ? "DuckDB standby — no project session" : engineState === "recovering" ? "DuckDB stopped — reconnects on the next project operation" : engineState === "failed" ? "DuckDB connection failed" : runtime.kind === "ready" ? "No DuckDB project open" : "Starting Tarik"}</span>
        <span className="status-bar-right"><EngineResourcesDialog key={resourceStatusKey} statusKey={resourceStatusKey} /><span>UTF-8</span></span>
      </footer>
      </main>
    </>
  );
}

async function waitForStartupStorage(isActive: () => boolean): Promise<void> {
  while (isActive()) {
    try {
      if ((await getStartupStatus()).ready) return;
    } catch {
      return;
    }
    await new Promise((resolve) => window.setTimeout(resolve, 100));
  }
}

function sourceForCatalogObject(
  sources: SourceRecord[],
  objects: ProjectCatalog["objects"],
  object: ProjectCatalog["objects"][number],
): SourceRecord | null {
  // Existing source metadata predates qualified catalog identity. Tarik creates
  // imported tables and linked views unqualified in main. Associate it only
  // when main.<name> resolves to one database, never by name alone.
  if (object.schema !== "main") return null;
  const matches = objects.filter(
    (candidate) => candidate.schema === "main" && candidate.name === object.name,
  );
  if (matches.length !== 1 || matches[0].database !== object.database) return null;
  return sources.find((source) => source.duckdbName === object.name) ?? null;
}

function canProfileCatalogObject(
  catalogRevision: string | undefined,
  source: SourceRecord | null,
): boolean {
  return Boolean(catalogRevision && source?.state !== "missing");
}

function sameSourceIdentity(current: SourceRecord | null, opened: SourceRecord | null): boolean {
  if (!current || !opened) return current === opened;
  return (
    current.id === opened.id &&
    current.projectId === opened.projectId &&
    current.kind === opened.kind &&
    current.state === opened.state &&
    current.sourcePath === opened.sourcePath &&
    current.duckdbName === opened.duckdbName
  );
}

function appConfirmationLabel(intent: AppConfirmIntent): string {
  if (intent.kind === "remove-source") return "Remove link";
  if (intent.kind === "drop-object") {
    return intent.object.kind === "table" ? "Delete table" : "Delete view";
  }
  return intent.project.ownership === "managed" ? "Delete project" : "Forget project";
}

function appConfirmationDescription(intent: AppConfirmIntent): string {
  if (intent.kind === "remove-source") {
    return "Remove this link from Tarik. The original Parquet file is preserved.";
  }
  if (intent.kind === "drop-object") {
    return `This permanently removes the ${intent.object.kind} from the active DuckDB project.`;
  }
  return intent.project.ownership === "managed"
    ? "This permanently deletes the Tarik-managed project and its directory."
    : "This removes the project from Tarik. The external DuckDB file is preserved.";
}

function appConfirmationDetail(intent: AppConfirmIntent): string {
  if (intent.kind === "remove-source") return intent.source.displayName;
  if (intent.kind === "drop-object") {
    const object = `${intent.object.database}.${intent.object.schema}.${intent.object.name}`;
    return intent.source?.sourcePath
      ? `${object}\nOriginal source preserved: ${intent.source.sourcePath}`
      : object;
  }
  return `${intent.project.name}\n${intent.project.duckdbPath}`;
}

export default App;
