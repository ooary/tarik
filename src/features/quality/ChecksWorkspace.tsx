import {
  ArrowLeftIcon,
  ArrowSquareOutIcon,
  CopyIcon,
  FloppyDiskIcon,
  ListChecksIcon,
  PencilSimpleIcon,
  PlayIcon,
  PlusIcon,
  SpinnerGapIcon,
  TableIcon,
  TrashIcon,
  XIcon,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ConfirmationDialog } from "../../components/ui";
import {
  cancelQualityFailurePreview,
  cancelQualityRun,
  clearQualityCheckHistory,
  createQualityCheck,
  deleteQualityCheck,
  getQualityCheckHistory,
  getQualityFailurePreviewStatus,
  getQualityRunDetail,
  getQualityRunStatus,
  listLatestQualityRuns,
  listQualityChecks,
  previewQualityCheckSql,
  releaseQualityFailurePreview,
  rerunQualityRevision,
  runQualityCheck,
  runQualitySuite,
  startQualityFailurePreview,
  updateQualityCheck,
  type ActiveProject,
  type CatalogColumn,
  type CatalogObject,
  type CompiledQualityCheckPreview,
  type ProjectCatalog,
  type QualityCheckDefinition,
  type QualityCheckDraft,
  type QualityCheckOptions,
  type QualityCheckRun,
  type QualityCheckType,
  type QualityExecutionView,
  type QualityRunDetail,
} from "../../lib/commands";
import type { ProfileCheckPrefill } from "../profile/ProfileWorkspace";
import { ResultGrid } from "../results/ResultGrid";
import "./quality.css";

const PREVIEW_DELAY_MS = 220;
const RUN_POLL_DELAY_MS = 250;
const HISTORY_PAGE_SIZE = 25;
const CUSTOM_SQL_LIMIT = 256 * 1024;

type WorkspaceMode = "definitions" | "runs";
type DefinitionPane = "checks" | "definition" | "sql";
type RunsPane = "history" | "detail";

type BuilderState = {
  id: string | null;
  draft: QualityCheckDraft;
  acceptedValuesText: string;
  minimumText: string;
  maximumText: string;
  freshnessAmount: string;
  freshnessUnit: "minutes" | "hours" | "days";
};

interface ChecksWorkspaceProps {
  project: ActiveProject;
  catalog: ProjectCatalog;
  prefill: ProfileCheckPrefill | null;
  onClose: () => void;
  onOpenSql: (sql: string, title: string) => void;
  onProfileTarget: (target: QualityCheckDraft["target"]) => void;
  onRepairTarget: (target: QualityCheckDraft["target"]) => Promise<void>;
}

type RunConfirmation =
  | { kind: "latest"; check: QualityCheckDefinition }
  | { kind: "historical"; detail: QualityRunDetail }
  | { kind: "suite"; checks: QualityCheckDefinition[] };

export function ChecksWorkspace({
  project,
  catalog,
  prefill,
  onClose,
  onOpenSql,
  onProfileTarget,
  onRepairTarget,
}: ChecksWorkspaceProps) {
  const [mode, setMode] = useState<WorkspaceMode>("definitions");
  const [definitionPane, setDefinitionPane] = useState<DefinitionPane>(
    prefill ? "definition" : "checks",
  );
  const [runsPane, setRunsPane] = useState<RunsPane>("history");
  const [checks, setChecks] = useState<QualityCheckDefinition[]>([]);
  const [latestRuns, setLatestRuns] = useState<QualityCheckRun[]>([]);
  const [historyPage, setHistoryPage] = useState<{
    entries: QualityCheckRun[];
    nextOffset: number | null;
  }>({ entries: [], nextOffset: null });
  const [activeRuns, setActiveRuns] = useState<Map<string, QualityExecutionView>>(new Map());
  const [elapsedNow, setElapsedNow] = useState(0);
  const [runStarts, setRunStarts] = useState<Map<string, number>>(new Map());
  const [builder, setBuilder] = useState<BuilderState | null>(() =>
    prefill ? stateFromDraft(prefill.draft, null) : null,
  );
  const [previewState, setPreviewState] = useState<{
    key: string;
    compiled: CompiledQualityCheckPreview | null;
    error: string | null;
  } | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [runDetail, setRunDetail] = useState<QualityRunDetail | null>(null);
  const [runDetailError, setRunDetailError] = useState<string | null>(null);
  const [failurePreview, setFailurePreview] = useState<{
    resultId: string;
    state: "queued" | "running" | "succeeded" | "failed" | "cancelled";
    rowTotal: number | null;
    error: string | null;
  } | null>(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [search, setSearch] = useState("");
  const [deleteIntent, setDeleteIntent] = useState<QualityCheckDefinition | null>(null);
  const [runConfirmation, setRunConfirmation] = useState<RunConfirmation | null>(null);
  const [clearHistoryIntent, setClearHistoryIntent] = useState(false);
  const backButton = useRef<HTMLButtonElement>(null);
  const previewSequence = useRef(0);
  const [baseline, setBaseline] = useState<string | null>(null);
  const mounted = useRef(true);
  const failurePreviewRef = useRef<string | null>(null);

  const objects = useMemo(
    () => catalog.objects.filter((object) => object.kind === "table" || object.kind === "view"),
    [catalog.objects],
  );
  const latestByCheck = useMemo(() => {
    const latest = new Map<string, QualityCheckRun>();
    for (const run of latestRuns) if (!latest.has(run.checkId)) latest.set(run.checkId, run);
    return latest;
  }, [latestRuns]);
  const checksById = useMemo(() => new Map(checks.map((check) => [check.id, check])), [checks]);
  const visibleChecks = useMemo(() => {
    const query = search.trim().toLocaleLowerCase();
    if (!query) return checks;
    return checks.filter((check) =>
      [check.name, check.checkType, check.target.object, ...check.target.columns]
        .join(" ")
        .toLocaleLowerCase()
        .includes(query),
    );
  }, [checks, search]);
  const visibleRuns = useMemo(() => {
    const active = [...activeRuns.values()];
    const activeIds = new Set(active.map((run) => run.runId));
    return [...active, ...historyPage.entries.filter((run) => !activeIds.has(run.id))];
  }, [activeRuns, historyPage.entries]);
  const suiteSummary = useMemo(() => summarizeRuns(activeRuns), [activeRuns]);
  const selectedRunState = selectedRunId ? activeRuns.get(selectedRunId)?.state : undefined;

  useEffect(() => {
    if (!suiteSummary.active) return;
    const timer = window.setInterval(() => setElapsedNow(new Date().getTime()), 1_000);
    return () => window.clearInterval(timer);
  }, [suiteSummary.active]);

  const refresh = useCallback(
    async (showLoading = false) => {
      if (showLoading) setLoading(true);
      try {
        const [definitions, latest, history] = await Promise.all([
          listQualityChecks(project.id),
          listLatestQualityRuns(project.id),
          getQualityCheckHistory(project.id, null, 0, HISTORY_PAGE_SIZE),
        ]);
        if (!mounted.current) return;
        setChecks(definitions);
        setLatestRuns(latest);
        setHistoryPage({ entries: history.entries, nextOffset: history.nextOffset });
      } catch (error) {
        if (mounted.current) setOperationError(friendlyError(error));
      } finally {
        if (mounted.current) setLoading(false);
      }
    },
    [project.id],
  );

  useEffect(() => {
    mounted.current = true;
    backButton.current?.focus();
    const timer = window.setTimeout(() => void refresh(), 0);
    return () => {
      mounted.current = false;
      window.clearTimeout(timer);
    };
  }, [refresh]);

  useEffect(() => {
    const running = [...activeRuns.values()].filter((run) => isActiveRun(run.state));
    if (!running.length) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      Promise.all(running.map((run) => getQualityRunStatus(run.runId)))
        .then((statuses) => {
          if (cancelled) return;
          let reachedTerminal = false;
          setActiveRuns((current) => {
            const next = new Map(current);
            statuses.forEach((status, index) => {
              if (!status) {
                next.set(running[index].runId, {
                  ...running[index],
                  state: "error",
                  error: {
                    code: "quality.lost",
                    message: "The engine no longer knows this run. Refresh durable history.",
                  },
                });
                reachedTerminal = true;
              } else {
                next.set(status.runId, status);
                if (!isActiveRun(status.state)) reachedTerminal = true;
              }
            });
            return next;
          });
          if (reachedTerminal) void refresh(false);
        })
        .catch((error: unknown) => {
          if (!cancelled)
            setOperationError(`Run status unavailable. Retrying: ${friendlyError(error)}`);
        });
    }, RUN_POLL_DELAY_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [activeRuns, refresh]);

  useEffect(() => {
    if (!selectedRunId || selectedRunState === "queued" || selectedRunState === "running") return;
    let cancelled = false;
    getQualityRunDetail(project.id, selectedRunId)
      .then((detail) => {
        if (!cancelled) setRunDetail(detail);
      })
      .catch((error: unknown) => {
        if (!cancelled) setRunDetailError(friendlyError(error));
      });
    return () => {
      cancelled = true;
    };
  }, [project.id, selectedRunId, selectedRunState]);

  useEffect(() => {
    if (!failurePreview || !isPreviewActive(failurePreview.state)) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      getQualityFailurePreviewStatus(failurePreview.resultId)
        .then((status) => {
          if (cancelled) return;
          if (!status) {
            setFailurePreview((current) =>
              current ? { ...current, state: "failed", error: "Failure preview was lost." } : null,
            );
            return;
          }
          setFailurePreview((current) =>
            current
              ? {
                  ...current,
                  state: status.state,
                  rowTotal: status.result?.rowCount ?? current.rowTotal,
                  error: status.error?.message ?? null,
                }
              : null,
          );
        })
        .catch((error: unknown) => {
          if (!cancelled)
            setFailurePreview((current) =>
              current ? { ...current, error: friendlyError(error) } : null,
            );
        });
    }, RUN_POLL_DELAY_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [failurePreview]);

  useEffect(
    () => () => {
      if (failurePreviewRef.current) void releaseQualityFailurePreview(failurePreviewRef.current);
    },
    [],
  );

  const validation = useMemo(
    () => (builder ? validateBuilder(builder, catalog) : []),
    [builder, catalog],
  );
  const draft = useMemo(() => (builder ? materializeDraft(builder) : null), [builder]);
  const previewKey = draft && validation.length === 0 ? JSON.stringify(draft) : null;
  const compiled = previewState?.key === previewKey ? previewState.compiled : null;
  const previewError = previewState?.key === previewKey ? previewState.error : null;
  const dirty = Boolean(builder && baseline !== serializeDraft(builder));

  useEffect(() => {
    const sequence = ++previewSequence.current;
    if (!draft || !previewKey) return;
    const timer = window.setTimeout(() => {
      previewQualityCheckSql(draft)
        .then((value) => {
          if (sequence === previewSequence.current) {
            setPreviewState({ key: previewKey, compiled: value, error: null });
          }
        })
        .catch((error: unknown) => {
          if (sequence === previewSequence.current) {
            setPreviewState({ key: previewKey, compiled: null, error: friendlyError(error) });
          }
        });
    }, PREVIEW_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [draft, previewKey]);

  function newCheck(type: QualityCheckType = "not_null") {
    const next = newBuilder(project.id, objects, catalog.columns, type);
    setMode("definitions");
    setDefinitionPane("definition");
    setBuilder(next);
    setBaseline(null);
    setOperationError(null);
  }

  function editCheck(check: QualityCheckDefinition) {
    const next = stateFromDraft(check, check.id);
    setMode("definitions");
    setDefinitionPane("definition");
    setBuilder(next);
    setBaseline(serializeDraft(next));
    setOperationError(null);
  }

  function duplicateCheck(check: QualityCheckDefinition) {
    const next = stateFromDraft({ ...check, name: `${check.name} copy` }, null);
    setMode("definitions");
    setDefinitionPane("definition");
    setBuilder(next);
    setBaseline(null);
    setOperationError(null);
  }

  async function save() {
    if (!builder || !draft || validation.length || !compiled || busy) return;
    setBusy(true);
    setOperationError(null);
    try {
      const saved = builder.id
        ? await updateQualityCheck(builder.id, draft)
        : await createQualityCheck(draft);
      const next = stateFromDraft(saved, saved.id);
      setBuilder(next);
      setBaseline(serializeDraft(next));
      await refresh(false);
    } catch (error) {
      setOperationError(friendlyError(error));
    } finally {
      setBusy(false);
    }
  }

  function requestRun(check: QualityCheckDefinition) {
    if (busy || !check.enabled) return;
    if (check.options.kind === "custom_sql") setRunConfirmation({ kind: "latest", check });
    else void submitLatest(check);
  }

  async function submitLatest(check: QualityCheckDefinition) {
    setBusy(true);
    setOperationError(null);
    try {
      const run = await runQualityCheck(project.id, check.id);
      acceptRuns([run]);
    } catch (error) {
      setOperationError(friendlyError(error));
    } finally {
      setBusy(false);
      setRunConfirmation(null);
    }
  }

  async function submitSuite() {
    const enabled = checks.filter((check) => check.enabled);
    if (!enabled.length || busy) return;
    if (enabled.some((check) => check.options.kind === "custom_sql")) {
      setRunConfirmation({ kind: "suite", checks: enabled });
      return;
    }
    await confirmSuite();
  }

  async function confirmSuite() {
    setBusy(true);
    setOperationError(null);
    try {
      acceptRuns(await runQualitySuite(project.id));
    } catch (error) {
      setOperationError(friendlyError(error));
    } finally {
      setBusy(false);
      setRunConfirmation(null);
    }
  }

  async function submitHistorical(detail: QualityRunDetail) {
    setBusy(true);
    setOperationError(null);
    try {
      acceptRuns([await rerunQualityRevision(project.id, detail.run.id)]);
    } catch (error) {
      setOperationError(friendlyError(error));
    } finally {
      setBusy(false);
      setRunConfirmation(null);
    }
  }

  function acceptRuns(runs: QualityExecutionView[]) {
    const now = new Date().getTime();
    setRunStarts((current) => {
      const next = new Map(current);
      for (const run of runs) next.set(run.runId, now - run.durationMs);
      return new Map([...next.entries()].slice(-200));
    });
    setElapsedNow(now);
    setActiveRuns((current) => {
      const retained = [...current.values()].filter((run) => isActiveRun(run.state));
      const next = new Map(retained.map((run) => [run.runId, run]));
      for (const run of runs) next.set(run.runId, run);
      return new Map([...next.entries()].slice(-200));
    });
    setMode("runs");
    setRunsPane("detail");
    if (runs[0]) setSelectedRunId(runs[0].runId);
    setOperationError(
      runs.length
        ? `Started ${runs.length} quality ${runs.length === 1 ? "run" : "runs"}.`
        : "No enabled checks to run.",
    );
  }

  async function stopRun(runId: string) {
    try {
      const status = await cancelQualityRun(runId);
      setActiveRuns((current) => new Map(current).set(status.runId, status));
    } catch (error) {
      setOperationError(friendlyError(error));
    }
  }

  async function openFailurePreview() {
    if (!runDetail || failurePreview) return;
    setOperationError(null);
    try {
      const preview = await startQualityFailurePreview(project.id, runDetail.run.id);
      failurePreviewRef.current = preview.resultId;
      setFailurePreview({
        resultId: preview.resultId,
        state: preview.state,
        rowTotal: null,
        error: null,
      });
    } catch (error) {
      setOperationError(friendlyError(error));
    }
  }

  async function closeFailurePreview() {
    const current = failurePreview;
    if (!current) return;
    failurePreviewRef.current = null;
    setFailurePreview(null);
    try {
      if (isPreviewActive(current.state)) await cancelQualityFailurePreview(current.resultId);
      await releaseQualityFailurePreview(current.resultId);
    } catch (error) {
      setOperationError(friendlyError(error));
    }
  }

  async function loadOlderRuns() {
    if (historyPage.nextOffset == null || busy) return;
    setBusy(true);
    try {
      const page = await getQualityCheckHistory(
        project.id,
        null,
        historyPage.nextOffset,
        HISTORY_PAGE_SIZE,
      );
      setHistoryPage((current) => ({
        entries: [...current.entries, ...page.entries],
        nextOffset: page.nextOffset,
      }));
    } catch (error) {
      setOperationError(friendlyError(error));
    } finally {
      setBusy(false);
    }
  }

  async function clearHistory() {
    setBusy(true);
    try {
      await closeFailurePreview();
      await clearQualityCheckHistory(project.id, null);
      setSelectedRunId(null);
      setRunDetail(null);
      await refresh(false);
      setClearHistoryIntent(false);
      setOperationError("Quality run history cleared. Check definitions were not changed.");
    } catch (error) {
      setOperationError(friendlyError(error));
    } finally {
      setBusy(false);
    }
  }

  async function removeCheck() {
    if (!deleteIntent || busy) return;
    setBusy(true);
    setOperationError(null);
    try {
      await deleteQualityCheck(project.id, deleteIntent.id);
      if (builder?.id === deleteIntent.id) setBuilder(null);
      setDeleteIntent(null);
      await refresh(false);
    } catch (error) {
      setOperationError(friendlyError(error));
    } finally {
      setBusy(false);
    }
  }

  const selectedDefinition = builder?.id
    ? (checks.find((check) => check.id === builder.id) ?? null)
    : null;
  const canRunSelected = Boolean(selectedDefinition && !dirty && compiled && !validation.length);
  const selectedActive = selectedRunId ? activeRuns.get(selectedRunId) : null;

  return (
    <section aria-label="Quality checks workspace" className="checks-workspace">
      <header className="checks-header">
        <button
          aria-label="Back to query editor"
          className="icon-button"
          onClick={onClose}
          ref={backButton}
          type="button"
        >
          <ArrowLeftIcon aria-hidden="true" size={16} />
        </button>
        <div>
          <h1>Quality checks</h1>
          <p>Define expectations, run them explicitly, and inspect bounded evidence.</p>
        </div>
        <nav aria-label="Quality workspace views" className="quality-mode-tabs">
          <button
            aria-current={mode === "definitions" ? "page" : undefined}
            onClick={() => setMode("definitions")}
            type="button"
          >
            Definitions
          </button>
          <button
            aria-current={mode === "runs" ? "page" : undefined}
            onClick={() => setMode("runs")}
            type="button"
          >
            Runs
          </button>
        </nav>
        {mode === "definitions" ? (
          <button className="toolbar-button" onClick={() => newCheck()} type="button">
            <PlusIcon aria-hidden="true" size={14} /> New check
          </button>
        ) : (
          <button
            className="run-button"
            disabled={busy || checks.every((check) => !check.enabled)}
            onClick={() => void submitSuite()}
            type="button"
          >
            <PlayIcon aria-hidden="true" size={14} weight="fill" /> Run enabled
          </button>
        )}
      </header>

      {operationError && (
        <div className="check-operation-note" role="status">
          {operationError}
        </div>
      )}

      {mode === "definitions" ? (
        <>
          <div aria-label="Definition sections" className="quality-pane-tabs" role="tablist">
            {(["checks", "definition", "sql"] as DefinitionPane[]).map((pane) => (
              <button
                aria-selected={definitionPane === pane}
                key={pane}
                onClick={() => setDefinitionPane(pane)}
                role="tab"
                type="button"
              >
                {pane === "checks" ? "Checks" : pane === "definition" ? "Definition" : "SQL"}
              </button>
            ))}
          </div>
          <div className="checks-body">
            <aside
              aria-label="Saved quality checks"
              className="checks-list-pane"
              data-pane-active={definitionPane === "checks"}
            >
              <label className="checks-search">
                <span>Search checks</span>
                <input
                  onChange={(event) => setSearch(event.currentTarget.value)}
                  placeholder="Name, type, table, or column"
                  type="search"
                  value={search}
                />
              </label>
              <div className="checks-list-summary">
                <strong>{visibleChecks.length} checks</strong>
                <span>Maximum 200 per project</span>
              </div>
              <div className="checks-list">
                {loading && <p className="checks-empty">Loading saved checks...</p>}
                {!loading && visibleChecks.length === 0 && (
                  <div className="checks-empty">
                    <strong>{checks.length ? "No matches" : "No quality checks yet"}</strong>
                    <span>Create a guided check or start from a Profile observation.</span>
                  </div>
                )}
                {visibleChecks.map((check) => {
                  const latest = latestByCheck.get(check.id);
                  return (
                    <article
                      className={`check-list-row ${builder?.id === check.id ? "check-list-row-selected" : ""}`}
                      key={check.id}
                    >
                      <button onClick={() => editCheck(check)} type="button">
                        <span className="check-list-name">{check.name}</span>
                        <span className="check-list-meta">
                          {typeLabel(check.checkType)} / {check.target.object}
                          {check.target.columns.length ? `.${check.target.columns.join(", ")}` : ""}
                        </span>
                        <span className="check-list-state">
                          <span className={`quality-severity quality-severity-${check.severity}`}>
                            {check.severity}
                          </span>
                          <span>{check.enabled ? "Enabled" : "Disabled"}</span>
                          <span>{latest ? outcomeLabel(latest.outcome) : "Never run"}</span>
                        </span>
                      </button>
                      <div className="check-row-actions">
                        <button
                          aria-label={`Run ${check.name}`}
                          className="icon-button"
                          disabled={busy || !check.enabled}
                          onClick={() => requestRun(check)}
                          title="Run saved revision"
                          type="button"
                        >
                          <PlayIcon aria-hidden="true" size={14} weight="fill" />
                        </button>
                        <button
                          aria-label={`Duplicate ${check.name}`}
                          className="icon-button"
                          onClick={() => duplicateCheck(check)}
                          title="Duplicate as unsaved draft"
                          type="button"
                        >
                          <CopyIcon aria-hidden="true" size={14} />
                        </button>
                        <button
                          aria-label={`Delete ${check.name}`}
                          className="icon-button"
                          onClick={() => setDeleteIntent(check)}
                          title="Delete check"
                          type="button"
                        >
                          <TrashIcon aria-hidden="true" size={14} />
                        </button>
                      </div>
                    </article>
                  );
                })}
              </div>
            </aside>

            <main className="check-builder-pane" data-pane-active={definitionPane === "definition"}>
              {!builder ? (
                <div className="checks-builder-empty">
                  <strong>Choose a saved check or create a new one.</strong>
                  <span>Opening and editing never run a check.</span>
                </div>
              ) : (
                <>
                  <header className="check-builder-header">
                    <div>
                      <strong>{builder.id ? "Edit check" : "New check"}</strong>
                      <span>
                        {dirty
                          ? "Unsaved draft"
                          : `Saved revision ${selectedDefinition?.revisionNumber ?? ""}`}
                      </span>
                    </div>
                    <div>
                      <button
                        className="toolbar-button"
                        disabled={!canRunSelected || busy}
                        onClick={() => selectedDefinition && requestRun(selectedDefinition)}
                        title={dirty ? "Save this draft before running" : "Run saved revision"}
                        type="button"
                      >
                        <PlayIcon aria-hidden="true" size={14} weight="fill" /> Run
                      </button>
                      <button
                        className="run-button"
                        disabled={busy || validation.length > 0 || !compiled || !dirty}
                        onClick={() => void save()}
                        type="button"
                      >
                        <FloppyDiskIcon aria-hidden="true" size={14} />{" "}
                        {busy ? "Saving..." : "Save"}
                      </button>
                    </div>
                  </header>
                  <div className="check-builder-scroll">
                    <GuidedCheckForm
                      builder={builder}
                      catalog={catalog}
                      disabled={busy}
                      onChange={setBuilder}
                      objects={objects}
                    />
                    {validation.length > 0 && (
                      <div className="check-validation" role="alert">
                        <strong>Review this draft</strong>
                        {validation.map((message) => (
                          <span key={message}>{message}</span>
                        ))}
                      </div>
                    )}
                  </div>
                </>
              )}
            </main>

            <aside
              aria-label="Generated SQL"
              className="check-sql-pane"
              data-pane-active={definitionPane === "sql"}
            >
              <header>
                <div>
                  <strong>SQL evidence</strong>
                  <span>Compiled from this draft. Not executed.</span>
                </div>
                {compiled && draft && (
                  <div>
                    <button
                      className="quality-copy-button"
                      onClick={() => void navigator.clipboard?.writeText(compiled.failureSql)}
                      title="Copy the exact failure-row SQL without running it"
                      type="button"
                    >
                      <CopyIcon aria-hidden="true" size={13} /> Copy SQL
                    </button>
                    <button
                      className="quality-open-button"
                      onClick={() => onOpenSql(compiled.failureSql, `${draft.name} failures`)}
                      title="Open this SQL in a new editor tab without running it"
                      type="button"
                    >
                      <ArrowSquareOutIcon aria-hidden="true" size={13} /> Open SQL
                    </button>
                  </div>
                )}
              </header>
              {!builder && <p className="checks-sql-empty">Select a check to inspect its SQL.</p>}
              {builder && !compiled && !previewError && validation.length === 0 && (
                <p className="checks-sql-empty">Compiling a read-only preview...</p>
              )}
              {previewError && (
                <div className="check-validation" role="alert">
                  <strong>SQL preview unavailable</strong>
                  <span>{previewError}</span>
                </div>
              )}
              {compiled && draft && <SqlEvidence compiled={compiled} draft={draft} />}
            </aside>
          </div>
        </>
      ) : (
        <>
          <div aria-label="Run sections" className="quality-runs-pane-tabs" role="tablist">
            <button
              aria-selected={runsPane === "history"}
              onClick={() => setRunsPane("history")}
              role="tab"
              type="button"
            >
              Run history
            </button>
            <button
              aria-selected={runsPane === "detail"}
              disabled={!selectedRunId}
              onClick={() => setRunsPane("detail")}
              role="tab"
              type="button"
            >
              Run detail
            </button>
          </div>
          <div className="quality-runs-body">
            <aside
              aria-label="Quality run history"
              className="quality-run-list-pane"
              data-runs-pane-active={runsPane === "history"}
            >
              <header className="quality-runs-summary">
                <div>
                  <strong>{suiteSummary.active ? "Suite in progress" : "Run history"}</strong>
                  <span>
                    {suiteSummary.active
                      ? `${suiteSummary.terminal} of ${suiteSummary.total} finished`
                      : `${historyPage.entries.length} recent runs loaded`}
                  </span>
                </div>
                {suiteSummary.active > 0 && (
                  <span className="quality-live-state" role="status">
                    <SpinnerGapIcon aria-hidden="true" size={14} /> {suiteSummary.active} active
                  </span>
                )}
              </header>
              {activeRuns.size > 0 && (
                <dl className="quality-run-counts" aria-label="Current suite status counts">
                  {(["queued", "running", "passed", "failed", "error", "cancelled"] as const).map(
                    (state) => (
                      <div key={state}>
                        <dt>{runStateLabel(state)}</dt>
                        <dd>{suiteSummary.counts[state]}</dd>
                      </div>
                    ),
                  )}
                </dl>
              )}
              <div className="quality-run-list">
                {visibleRuns.length === 0 && !loading && (
                  <div className="checks-empty">
                    <strong>No quality runs yet</strong>
                    <span>
                      Run one saved check or all enabled checks. Nothing starts automatically.
                    </span>
                  </div>
                )}
                {visibleRuns.map((run) => {
                  const active = "runId" in run;
                  const id = active ? run.runId : run.id;
                  const check = checksById.get(run.checkId);
                  const state = active ? run.state : run.outcome;
                  return (
                    <div className="quality-run-row-wrap" key={id}>
                      <button
                        aria-current={selectedRunId === id ? "true" : undefined}
                        className="quality-run-row"
                        onClick={() => {
                          void closeFailurePreview();
                          setRunDetail(null);
                          setRunDetailError(null);
                          setSelectedRunId(id);
                          setRunsPane("detail");
                        }}
                        type="button"
                      >
                        <span className={`quality-run-state quality-run-state-${state}`}>
                          {runStateLabel(state)}
                        </span>
                        <strong>{check?.name ?? "Historical check"}</strong>
                        <span>
                          {active
                            ? formatDuration(
                                Math.max(
                                  run.durationMs,
                                  elapsedNow -
                                    (runStarts.get(run.runId) ?? elapsedNow - run.durationMs),
                                ),
                              )
                            : `${formatObservedAt(run.observedAt)} / ${formatDuration(run.durationMs)}`}
                        </span>
                      </button>
                      {active && isActiveRun(run.state) && (
                        <button
                          aria-label={`Cancel ${check?.name ?? "quality run"}`}
                          className="icon-button quality-run-cancel"
                          onClick={() => void stopRun(run.runId)}
                          type="button"
                        >
                          <XIcon aria-hidden="true" size={13} />
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
              <footer className="quality-run-list-actions">
                <button
                  className="subtle-button"
                  disabled={busy || historyPage.nextOffset == null}
                  onClick={() => void loadOlderRuns()}
                  type="button"
                >
                  Load older
                </button>
                <button
                  className="subtle-button"
                  disabled={busy || historyPage.entries.length === 0}
                  onClick={() => setClearHistoryIntent(true)}
                  type="button"
                >
                  Clear history
                </button>
              </footer>
            </aside>

            <main className="quality-run-detail-pane" data-runs-pane-active={runsPane === "detail"}>
              {!selectedRunId ? (
                <div className="checks-builder-empty">
                  <strong>Select a run to inspect its evidence.</strong>
                  <span>
                    History stores aggregate facts only. Failing rows are never persisted.
                  </span>
                </div>
              ) : selectedActive && isActiveRun(selectedActive.state) ? (
                <RunInProgress
                  elapsedMs={Math.max(
                    selectedActive.durationMs,
                    elapsedNow -
                      (runStarts.get(selectedActive.runId) ??
                        elapsedNow - selectedActive.durationMs),
                  )}
                  run={selectedActive}
                  onCancel={() => void stopRun(selectedActive.runId)}
                />
              ) : runDetailError ? (
                <div className="quality-run-error" role="alert">
                  <strong>Run detail unavailable</strong>
                  <span>{runDetailError}</span>
                  <button
                    className="subtle-button"
                    onClick={() => void refresh(false)}
                    type="button"
                  >
                    Refresh history
                  </button>
                </div>
              ) : !runDetail ? (
                <div className="quality-run-loading" role="status">
                  <SpinnerGapIcon aria-hidden="true" size={18} /> Loading immutable run evidence...
                </div>
              ) : (
                <RunDetail
                  detail={runDetail}
                  failurePreview={failurePreview}
                  onClosePreview={() => void closeFailurePreview()}
                  onEdit={() => {
                    const current = checksById.get(runDetail.run.checkId);
                    if (current) editCheck(current);
                  }}
                  onOpenPreview={() => void openFailurePreview()}
                  onOpenSql={() =>
                    onOpenSql(
                      runDetail.failureSql,
                      `${runDetail.checkName} revision ${runDetail.revisionNumber}`,
                    )
                  }
                  onProfile={() => onProfileTarget(runDetail.definition.target)}
                  onRepair={() => {
                    void onRepairTarget(runDetail.definition.target).catch((error: unknown) =>
                      setOperationError(friendlyError(error)),
                    );
                  }}
                  onRerun={() => {
                    if (runDetail.custom)
                      setRunConfirmation({ kind: "historical", detail: runDetail });
                    else void submitHistorical(runDetail);
                  }}
                />
              )}
            </main>
          </div>
        </>
      )}

      <ConfirmationDialog
        busy={busy}
        confirmLabel="Delete check"
        description="Delete this definition. Checks with run history require history to be cleared first."
        detail={deleteIntent?.name}
        error={deleteIntent ? operationError : null}
        onConfirm={removeCheck}
        onOpenChange={(open) => {
          if (!open) setDeleteIntent(null);
        }}
        open={Boolean(deleteIntent)}
        title="Delete quality check?"
        tone="destructive"
      />
      <ConfirmationDialog
        busy={busy}
        confirmLabel={runConfirmation?.kind === "suite" ? "Run suite" : "Run custom SQL"}
        description={runConfirmationDescription(runConfirmation)}
        detail={runConfirmationDetail(runConfirmation)}
        error={runConfirmation ? operationError : null}
        onConfirm={() => {
          if (runConfirmation?.kind === "latest") void submitLatest(runConfirmation.check);
          if (runConfirmation?.kind === "historical") void submitHistorical(runConfirmation.detail);
          if (runConfirmation?.kind === "suite") void confirmSuite();
        }}
        onOpenChange={(open) => {
          if (!open) setRunConfirmation(null);
        }}
        open={Boolean(runConfirmation)}
        title={
          runConfirmation?.kind === "suite"
            ? "Run suite with custom SQL?"
            : "Run custom quality SQL?"
        }
        tone="warning"
      />
      <ConfirmationDialog
        busy={busy}
        confirmLabel="Clear run history"
        description="Delete aggregate quality run history. Check definitions, revisions, project data, sources, and exports are not changed."
        error={clearHistoryIntent ? operationError : null}
        onConfirm={clearHistory}
        onOpenChange={(open) => setClearHistoryIntent(open)}
        open={clearHistoryIntent}
        title="Clear quality run history?"
        tone="destructive"
      />
    </section>
  );
}

function SqlEvidence({
  compiled,
  draft,
}: {
  compiled: CompiledQualityCheckPreview;
  draft: QualityCheckDraft;
}) {
  return (
    <>
      <div className="check-sql-block">
        <strong>Failure rows</strong>
        <pre>{compiled.failureSql}</pre>
      </div>
      <div className="check-sql-block">
        <strong>Pass/fail count</strong>
        <pre>{compiled.countSql}</pre>
      </div>
      <div className="check-explanation">
        <strong>What this SQL means</strong>
        {explainDraft(draft).map((line) => (
          <p key={line}>{line}</p>
        ))}
      </div>
      <p className="check-sql-safety">
        Copy and Open SQL never execute. Editing an opened tab never changes this definition.
      </p>
    </>
  );
}

function RunInProgress({
  elapsedMs,
  run,
  onCancel,
}: {
  elapsedMs: number;
  run: QualityExecutionView;
  onCancel: () => void;
}) {
  return (
    <div className="quality-run-progress" role="status">
      <SpinnerGapIcon aria-hidden="true" size={24} />
      <div>
        <strong>
          {run.state === "queued" ? "Waiting for DuckDB" : "Evaluating the expectation"}
        </strong>
        <span>{formatDuration(elapsedMs)} elapsed. This run uses one immutable revision.</span>
      </div>
      <button className="toolbar-button" onClick={onCancel} type="button">
        <XIcon aria-hidden="true" size={13} /> Cancel run
      </button>
    </div>
  );
}

function RunDetail({
  detail,
  failurePreview,
  onClosePreview,
  onEdit,
  onOpenPreview,
  onOpenSql,
  onProfile,
  onRepair,
  onRerun,
}: {
  detail: QualityRunDetail;
  failurePreview: {
    resultId: string;
    state: "queued" | "running" | "succeeded" | "failed" | "cancelled";
    rowTotal: number | null;
    error: string | null;
  } | null;
  onClosePreview: () => void;
  onEdit: () => void;
  onOpenPreview: () => void;
  onOpenSql: () => void;
  onProfile: () => void;
  onRepair: () => void;
  onRerun: () => void;
}) {
  const recovery = recoveryFor(detail);
  return (
    <article className="quality-run-detail">
      <header className="quality-run-detail-header">
        <div>
          <span className={`quality-run-state quality-run-state-${detail.run.outcome}`}>
            {runStateLabel(detail.run.outcome)}
          </span>
          <h2>{detail.checkName}</h2>
          <p>
            {formatObservedAt(detail.run.observedAt)} / {formatDuration(detail.run.durationMs)}
          </p>
        </div>
        <div>
          <button className="toolbar-button" onClick={onRerun} type="button">
            <PlayIcon aria-hidden="true" size={13} weight="fill" /> Rerun revision{" "}
            {detail.revisionNumber}
          </button>
          <button className="quality-open-button" onClick={onOpenSql} type="button">
            <ArrowSquareOutIcon aria-hidden="true" size={13} /> Open SQL
          </button>
        </div>
      </header>

      {!detail.isLatestRevision && (
        <div className="quality-revision-note" role="note">
          Historical run: revision {detail.revisionNumber}. Current definition: revision{" "}
          {detail.currentRevisionNumber}. Rerun uses revision {detail.revisionNumber}; it never
          switches to the latest revision silently.
        </div>
      )}

      <section className="quality-observation" aria-label="Observed and expected facts">
        <div>
          <span>Observed</span>
          <strong>{observedFact(detail)}</strong>
        </div>
        <div>
          <span>Expected</span>
          <strong>{expectedFact(detail.definition)}</strong>
        </div>
        <p>{outcomeExplanation(detail)}</p>
      </section>

      <section className="quality-recovery" aria-label="Recovery guidance">
        <div>
          <strong>{recovery.title}</strong>
          <span>{recovery.message}</span>
        </div>
        <div>
          <button className="subtle-button" onClick={onEdit} type="button">
            <PencilSimpleIcon aria-hidden="true" size={13} /> Edit check
          </button>
          <button className="subtle-button" onClick={onProfile} type="button">
            <TableIcon aria-hidden="true" size={13} /> Profile target
          </button>
          {/source|file|parquet/.test(detail.run.errorCode ?? "") && (
            <button className="subtle-button" onClick={onRepair} type="button">
              Repair link
            </button>
          )}
        </div>
      </section>

      {detail.run.outcome === "fail" && (
        <section className="quality-failure-section" aria-label="Failure examples">
          <header>
            <div>
              <strong>Current-data failure preview</strong>
              <span>
                Uses revision {detail.revisionNumber}. These are current rows, not retained
                historical rows.
              </span>
            </div>
            {failurePreview ? (
              <button className="subtle-button" onClick={onClosePreview} type="button">
                Close preview
              </button>
            ) : (
              <button className="toolbar-button" onClick={onOpenPreview} type="button">
                <ListChecksIcon aria-hidden="true" size={13} /> Preview current failures
              </button>
            )}
          </header>
          {failurePreview && isPreviewActive(failurePreview.state) && (
            <div className="quality-run-loading" role="status">
              <SpinnerGapIcon aria-hidden="true" size={16} /> Preparing bounded failure rows...
            </div>
          )}
          {failurePreview?.error && (
            <div className="quality-run-error" role="alert">
              {failurePreview.error}
            </div>
          )}
          {failurePreview?.state === "succeeded" && failurePreview.rowTotal != null && (
            <ResultGrid resultId={failurePreview.resultId} rowTotal={failurePreview.rowTotal} />
          )}
        </section>
      )}

      <details className="quality-run-sql">
        <summary>Immutable SQL evidence</summary>
        <div className="check-sql-block">
          <strong>Failure rows</strong>
          <pre>{detail.failureSql}</pre>
        </div>
        <div className="check-sql-block">
          <strong>Pass/fail count</strong>
          <pre>{detail.countSql}</pre>
        </div>
      </details>
    </article>
  );
}

function summarizeRuns(runs: Map<string, QualityExecutionView>) {
  const counts = { queued: 0, running: 0, passed: 0, failed: 0, error: 0, cancelled: 0 };
  for (const run of runs.values()) counts[run.state] += 1;
  const active = counts.queued + counts.running;
  return { counts, active, terminal: runs.size - active, total: runs.size };
}

function isActiveRun(state: QualityExecutionView["state"]): boolean {
  return state === "queued" || state === "running";
}
function isPreviewActive(state: "queued" | "running" | "succeeded" | "failed" | "cancelled") {
  return state === "queued" || state === "running";
}
function runStateLabel(state: QualityExecutionView["state"] | QualityCheckRun["outcome"]): string {
  return {
    queued: "Queued",
    running: "Running",
    passed: "Passed",
    pass: "Passed",
    failed: "Failed",
    fail: "Failed",
    error: "Error",
    cancelled: "Cancelled",
  }[state];
}
function formatDuration(durationMs: number): string {
  if (durationMs < 1_000) return `${durationMs} ms`;
  const seconds = Math.floor(durationMs / 1_000);
  return seconds < 60 ? `${seconds} sec` : `${Math.floor(seconds / 60)} min ${seconds % 60} sec`;
}
function formatObservedAt(value: string): string {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}
function observedFact(detail: QualityRunDetail): string {
  if (detail.run.outcome === "pass") return "0 failing rows";
  if (detail.run.outcome === "fail")
    return `${(detail.run.failureCount ?? 0).toLocaleString("en-US")} failing rows`;
  if (detail.run.outcome === "cancelled") return "Evaluation cancelled before a result";
  return `Expectation not evaluated${detail.run.errorCode ? ` (${detail.run.errorCode})` : ""}`;
}
function expectedFact(draft: QualityCheckDraft): string {
  if (draft.options.kind === "not_empty") return "The table or view contains at least 1 row";
  if (draft.options.kind === "not_null") return `0 rows have NULL ${draft.target.columns[0]}`;
  if (draft.options.kind === "unique")
    return `0 rows repeat the ${draft.target.columns.join(" + ")} key`;
  if (draft.options.kind === "accepted_values")
    return `0 rows fall outside ${draft.options.values.length} accepted values`;
  if (draft.options.kind === "range") return "0 rows fall outside the saved range";
  if (draft.options.kind === "relationship") return "0 child rows lack a matching parent";
  if (draft.options.kind === "freshness") return "The latest value is within the saved age limit";
  return "The saved read-only SQL returns 0 rows";
}
function outcomeExplanation(detail: QualityRunDetail): string {
  if (detail.run.outcome === "pass")
    return "DuckDB evaluated the saved expectation and found no failures.";
  if (detail.run.outcome === "fail")
    return "The check ran successfully. The data did not meet the saved expectation.";
  if (detail.run.outcome === "cancelled")
    return "No pass or fail claim was made because the run was cancelled.";
  return "Tarik could not evaluate the expectation. This is an execution error, not proof that the data failed.";
}
function recoveryFor(detail: QualityRunDetail): { title: string; message: string } {
  const code = detail.run.errorCode ?? "";
  if (/catalog|column|type/.test(code))
    return {
      title: "Review the target",
      message:
        "The table, column, or data type changed. Edit the definition or profile the current object before rerunning.",
    };
  if (/source|file|parquet/.test(code))
    return {
      title: "Repair the linked source",
      message: "Use the Explorer source action to locate the file, then rerun explicitly.",
    };
  if (/engine|lost|interrupted/.test(code))
    return {
      title: "Recover DuckDB",
      message:
        "Wait for the engine to reconnect, inspect the SQL if needed, then rerun explicitly.",
    };
  if (detail.run.outcome === "fail")
    return {
      title: "Inspect current failures",
      message:
        "Preview a bounded set of current rows, profile the target, or open the immutable SQL. Apply repairs in a query only when you choose.",
    };
  if (detail.run.outcome === "pass")
    return {
      title: "Expectation met",
      message: "Keep this aggregate result as evidence or rerun after the source changes.",
    };
  return {
    title: "Review and rerun",
    message:
      "Inspect the immutable revision and retry only after the underlying issue is understood.",
  };
}
function runConfirmationDescription(intent: RunConfirmation | null): string {
  if (intent?.kind === "suite")
    return "This suite includes custom read-only SQL. Tarik treats every returned row as a failure and runs each enabled revision explicitly.";
  if (intent?.kind === "historical")
    return `This reruns historical revision ${intent.detail.revisionNumber}, not current revision ${intent.detail.currentRevisionNumber}. Every returned row from its custom SQL is a failure.`;
  return "DuckDB validated this as one read-only result-producing statement. Tarik treats every returned row as a failure. Review the SQL before running it.";
}
function runConfirmationDetail(intent: RunConfirmation | null): string | null {
  if (intent?.kind === "latest" && intent.check.options.kind === "custom_sql")
    return intent.check.options.sql;
  if (intent?.kind === "historical") return intent.detail.failureSql;
  if (intent?.kind === "suite")
    return `${intent.checks.length} enabled checks, including ${intent.checks.filter((check) => check.options.kind === "custom_sql").length} custom SQL checks`;
  return null;
}

function GuidedCheckForm({
  builder,
  catalog,
  disabled,
  onChange,
  objects,
}: {
  builder: BuilderState;
  catalog: ProjectCatalog;
  disabled: boolean;
  onChange: (value: BuilderState) => void;
  objects: CatalogObject[];
}) {
  const draft = builder.draft;
  const selectedObject = findObject(objects, draft.target);
  const columns = columnsFor(catalog, selectedObject);
  const type = draft.options.kind;
  const setDraft = (patch: Partial<QualityCheckDraft>) =>
    onChange({ ...builder, draft: { ...draft, ...patch } });
  const setOptions = (options: QualityCheckOptions) => setDraft({ options });
  const needsColumns = type !== "not_empty" && type !== "custom_sql";
  const multiColumn = type === "unique" || type === "relationship";

  function selectObject(value: string) {
    const object = objects.find((candidate) => objectKey(candidate) === value);
    if (!object) return;
    const available = columnsFor(catalog, object);
    const count = needsColumns ? 1 : 0;
    setDraft({
      target: {
        database: object.database,
        schema: object.schema,
        object: object.name,
        columns: available.slice(0, count).map((column) => column.name),
      },
    });
  }

  function changeType(nextType: QualityCheckType) {
    const options = defaultOptions(nextType);
    const targetColumns =
      nextType === "not_empty" || nextType === "custom_sql"
        ? []
        : columns.slice(0, 1).map((column) => column.name);
    onChange({
      ...builder,
      draft: {
        ...draft,
        options,
        target: { ...draft.target, columns: targetColumns },
        nullPolicy: defaultNullPolicy(nextType),
      },
      acceptedValuesText: "",
      minimumText: "",
      maximumText: "",
      freshnessAmount: "24",
      freshnessUnit: "hours",
    });
  }

  return (
    <form className="check-form" onSubmit={(event) => event.preventDefault()}>
      <label>
        <span>Check name</span>
        <input
          disabled={disabled}
          maxLength={64}
          onChange={(event) => setDraft({ name: event.currentTarget.value })}
          value={draft.name}
        />
      </label>
      <div className="check-form-row">
        <label>
          <span>Expectation</span>
          <select
            disabled={disabled}
            onChange={(event) => changeType(event.currentTarget.value as QualityCheckType)}
            value={type}
          >
            {CHECK_TYPES.map((candidate) => (
              <option key={candidate} value={candidate}>
                {typeLabel(candidate)}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>Severity</span>
          <select
            disabled={disabled}
            onChange={(event) =>
              setDraft({ severity: event.currentTarget.value as QualityCheckDraft["severity"] })
            }
            value={draft.severity}
          >
            <option value="info">Info</option>
            <option value="warning">Warning</option>
            <option value="critical">Critical</option>
          </select>
        </label>
      </div>
      <label>
        <span>Table or view</span>
        <select
          disabled={disabled}
          onChange={(event) => selectObject(event.currentTarget.value)}
          value={selectedObject ? objectKey(selectedObject) : ""}
        >
          <option value="">Choose an object</option>
          {objects.map((object) => (
            <option key={objectKey(object)} value={objectKey(object)}>
              {object.database}.{object.schema}.{object.name} ({object.kind})
            </option>
          ))}
        </select>
      </label>

      {needsColumns && (
        <fieldset className="check-column-fieldset" disabled={disabled}>
          <legend>{multiColumn ? "Columns (ordered key)" : "Column"}</legend>
          {columns.map((column) => {
            const checked = draft.target.columns.includes(column.name);
            return (
              <label key={column.name}>
                <input
                  checked={checked}
                  onChange={(event) => {
                    const next = event.currentTarget.checked
                      ? multiColumn
                        ? [...draft.target.columns, column.name]
                        : [column.name]
                      : draft.target.columns.filter((name) => name !== column.name);
                    setDraft({ target: { ...draft.target, columns: next.slice(0, 16) } });
                  }}
                  type={multiColumn ? "checkbox" : "radio"}
                  name={multiColumn ? undefined : "quality-column"}
                />
                <span>{column.name}</span>
                <code>{column.dataType}</code>
              </label>
            );
          })}
        </fieldset>
      )}

      {type === "accepted_values" && (
        <label>
          <span>Accepted values, one per line</span>
          <textarea
            disabled={disabled}
            onChange={(event) =>
              onChange({ ...builder, acceptedValuesText: event.currentTarget.value })
            }
            rows={5}
            value={builder.acceptedValuesText}
          />
          <small>Maximum 100 non-NULL text, number, or boolean values.</small>
        </label>
      )}
      {type === "range" && (
        <div className="check-form-row">
          <label>
            <span>Minimum (optional)</span>
            <input
              disabled={disabled}
              onChange={(event) => onChange({ ...builder, minimumText: event.currentTarget.value })}
              value={builder.minimumText}
            />
          </label>
          <label>
            <span>Maximum (optional)</span>
            <input
              disabled={disabled}
              onChange={(event) => onChange({ ...builder, maximumText: event.currentTarget.value })}
              value={builder.maximumText}
            />
          </label>
        </div>
      )}
      {type === "relationship" && (
        <RelationshipFields
          builder={builder}
          catalog={catalog}
          disabled={disabled}
          objects={objects}
          onChange={onChange}
        />
      )}
      {type === "freshness" && (
        <div className="check-form-row">
          <label>
            <span>Maximum age</span>
            <input
              disabled={disabled}
              min={1}
              onChange={(event) =>
                onChange({ ...builder, freshnessAmount: event.currentTarget.value })
              }
              type="number"
              value={builder.freshnessAmount}
            />
          </label>
          <label>
            <span>Unit</span>
            <select
              disabled={disabled}
              onChange={(event) =>
                onChange({
                  ...builder,
                  freshnessUnit: event.currentTarget.value as BuilderState["freshnessUnit"],
                })
              }
              value={builder.freshnessUnit}
            >
              <option value="minutes">Minutes</option>
              <option value="hours">Hours</option>
              <option value="days">Days</option>
            </select>
          </label>
        </div>
      )}
      {type === "custom_sql" && (
        <label>
          <span>Read-only failure SQL</span>
          <textarea
            disabled={disabled}
            maxLength={CUSTOM_SQL_LIMIT}
            onChange={(event) => setOptions({ kind: "custom_sql", sql: event.currentTarget.value })}
            rows={9}
            value={draft.options.kind === "custom_sql" ? draft.options.sql : ""}
          />
          <small>
            Exactly one SELECT or WITH statement. Every returned row is a failure. File/network
            scans and mutations are rejected.
          </small>
        </label>
      )}

      {!(["not_empty", "not_null", "custom_sql"] as string[]).includes(type) && (
        <fieldset className="check-null-policy" disabled={disabled}>
          <legend>How should NULL be treated?</legend>
          <label>
            <input
              checked={draft.nullPolicy === "fail_on_null"}
              name="null-policy"
              onChange={() => setDraft({ nullPolicy: "fail_on_null" })}
              type="radio"
            />{" "}
            Fail it <small>NULL means the expectation is not met.</small>
          </label>
          <label>
            <input
              checked={draft.nullPolicy === "pass_on_null"}
              name="null-policy"
              onChange={() => setDraft({ nullPolicy: "pass_on_null" })}
              type="radio"
            />{" "}
            Ignore it <small>Only non-NULL values are tested.</small>
          </label>
        </fieldset>
      )}
      <label className="check-enabled">
        <input
          checked={draft.enabled}
          disabled={disabled}
          onChange={(event) => setDraft({ enabled: event.currentTarget.checked })}
          type="checkbox"
        />
        <span>Enabled in project suites</span>
      </label>
    </form>
  );
}

function RelationshipFields({
  builder,
  catalog,
  disabled,
  objects,
  onChange,
}: {
  builder: BuilderState;
  catalog: ProjectCatalog;
  disabled: boolean;
  objects: CatalogObject[];
  onChange: (value: BuilderState) => void;
}) {
  if (builder.draft.options.kind !== "relationship") return null;
  const options = builder.draft.options;
  const parentObject = findObject(objects, options.parent);
  const parentColumns = columnsFor(catalog, parentObject);
  function setOptions(patch: Partial<Extract<QualityCheckOptions, { kind: "relationship" }>>) {
    onChange({
      ...builder,
      draft: {
        ...builder.draft,
        options: {
          kind: "relationship",
          parent: patch.parent ?? options.parent,
          parentColumns: patch.parentColumns ?? options.parentColumns,
        },
      },
    });
  }
  return (
    <div className="relationship-fields">
      <strong>Parent key</strong>
      <label>
        <span>Parent table or view</span>
        <select
          disabled={disabled}
          onChange={(event) => {
            const object = objects.find(
              (candidate) => objectKey(candidate) === event.currentTarget.value,
            );
            if (!object) return;
            const columns = columnsFor(catalog, object);
            setOptions({
              parent: {
                database: object.database,
                schema: object.schema,
                object: object.name,
                columns: [],
              },
              parentColumns: columns
                .slice(0, builder.draft.target.columns.length)
                .map((column) => column.name),
            });
          }}
          value={parentObject ? objectKey(parentObject) : ""}
        >
          <option value="">Choose parent</option>
          {objects.map((object) => (
            <option key={objectKey(object)} value={objectKey(object)}>
              {object.database}.{object.schema}.{object.name}
            </option>
          ))}
        </select>
      </label>
      <fieldset className="relationship-key-map" disabled={disabled || !parentObject}>
        <legend>Parent columns (same order)</legend>
        {builder.draft.target.columns.length === 0 ? (
          <small>Choose one or more child-key columns first.</small>
        ) : (
          builder.draft.target.columns.map((child, index) => (
            <label key={`${child}:${index}`}>
              <span>
                {index + 1}. {child}
              </span>
              <select
                aria-label={`Parent column for ${child}`}
                onChange={(event) => {
                  const next = [...options.parentColumns];
                  next[index] = event.currentTarget.value;
                  setOptions({ parentColumns: next.slice(0, builder.draft.target.columns.length) });
                }}
                value={options.parentColumns[index] ?? ""}
              >
                <option value="">Choose column</option>
                {parentColumns.map((column) => (
                  <option key={column.name} value={column.name}>
                    {column.name} ({column.dataType})
                  </option>
                ))}
              </select>
            </label>
          ))
        )}
        <small>Child and parent keys are compared in this numbered order.</small>
      </fieldset>
    </div>
  );
}

const CHECK_TYPES: QualityCheckType[] = [
  "not_empty",
  "not_null",
  "unique",
  "accepted_values",
  "range",
  "relationship",
  "freshness",
  "custom_sql",
];

function newBuilder(
  projectId: string,
  objects: CatalogObject[],
  allColumns: CatalogColumn[],
  type: QualityCheckType,
): BuilderState {
  const object = objects[0];
  const columns = object
    ? allColumns.filter(
        (column) =>
          column.database === object.database &&
          column.schema === object.schema &&
          column.object === object.name,
      )
    : [];
  const draft: QualityCheckDraft = {
    projectId,
    name: "",
    target: {
      database: object?.database ?? "",
      schema: object?.schema ?? "",
      object: object?.name ?? "",
      columns:
        type === "not_empty" || type === "custom_sql"
          ? []
          : columns.slice(0, 1).map((column) => column.name),
    },
    options: defaultOptions(type),
    nullPolicy: defaultNullPolicy(type),
    severity: "warning",
    enabled: true,
  };
  return stateFromDraft(draft, null);
}

function stateFromDraft(draft: QualityCheckDraft, id: string | null): BuilderState {
  const accepted =
    draft.options.kind === "accepted_values" ? draft.options.values.map(String).join("\n") : "";
  const minimum =
    draft.options.kind === "range" && draft.options.minimum != null
      ? String(draft.options.minimum)
      : "";
  const maximum =
    draft.options.kind === "range" && draft.options.maximum != null
      ? String(draft.options.maximum)
      : "";
  const seconds = draft.options.kind === "freshness" ? draft.options.maximumAgeSeconds : 86_400;
  return {
    id,
    draft: cloneDraft(draft),
    acceptedValuesText: accepted,
    minimumText: minimum,
    maximumText: maximum,
    freshnessAmount: String(seconds / 3_600),
    freshnessUnit: "hours",
  };
}

function materializeDraft(builder: BuilderState): QualityCheckDraft {
  const draft = cloneDraft(builder.draft);
  if (draft.options.kind === "accepted_values")
    draft.options.values = builder.acceptedValuesText
      .split(/\r?\n/)
      .map((value) => value.trim())
      .filter(Boolean)
      .map(parseScalar);
  if (draft.options.kind === "range") {
    draft.options.minimum = builder.minimumText.trim()
      ? parseScalar(builder.minimumText.trim())
      : null;
    draft.options.maximum = builder.maximumText.trim()
      ? parseScalar(builder.maximumText.trim())
      : null;
  }
  if (draft.options.kind === "freshness") {
    const multiplier =
      builder.freshnessUnit === "minutes" ? 60 : builder.freshnessUnit === "hours" ? 3_600 : 86_400;
    draft.options.maximumAgeSeconds = Math.max(0, Number(builder.freshnessAmount) * multiplier);
  }
  return draft;
}

function validateBuilder(builder: BuilderState, catalog: ProjectCatalog): string[] {
  const draft = materializeDraft(builder);
  const errors: string[] = [];
  if (!draft.name.trim()) errors.push("Enter a check name.");
  if (new TextEncoder().encode(draft.name).length > 64)
    errors.push("Check name must be at most 64 UTF-8 bytes.");
  const object = findObject(catalog.objects, draft.target);
  if (!object) errors.push("Choose a current table or view.");
  const columns = columnsFor(catalog, object);
  for (const name of draft.target.columns)
    if (!columns.some((column) => column.name === name))
      errors.push(`Column “${name}” is no longer in the selected object.`);
  if (
    !["not_empty", "custom_sql"].includes(draft.options.kind) &&
    draft.target.columns.length === 0
  )
    errors.push("Choose at least one target column.");
  if (
    !["unique", "relationship"].includes(draft.options.kind) &&
    !["not_empty", "custom_sql"].includes(draft.options.kind) &&
    draft.target.columns.length !== 1
  )
    errors.push("This expectation requires exactly one column.");
  if (draft.target.columns.length > 16) errors.push("Composite keys support at most 16 columns.");
  if (
    draft.options.kind === "accepted_values" &&
    (draft.options.values.length === 0 || draft.options.values.length > 100)
  )
    errors.push("Enter 1-100 accepted values.");
  if (
    draft.options.kind === "range" &&
    draft.options.minimum == null &&
    draft.options.maximum == null
  )
    errors.push("Enter a minimum, maximum, or both.");
  if (
    draft.options.kind === "freshness" &&
    (!Number.isFinite(draft.options.maximumAgeSeconds) || draft.options.maximumAgeSeconds <= 0)
  )
    errors.push("Maximum age must be greater than zero.");
  if (draft.options.kind === "freshness") {
    const column = columns.find((candidate) => candidate.name === draft.target.columns[0]);
    if (column && !/DATE|TIME/.test(column.dataType.toUpperCase()))
      errors.push("Freshness requires a DATE or TIMESTAMP column.");
  }
  if (draft.options.kind === "range") {
    const column = columns.find((candidate) => candidate.name === draft.target.columns[0]);
    if (column && !isRangeType(column.dataType))
      errors.push("Range checks require a numeric, DATE, or TIMESTAMP column.");
  }
  if (draft.options.kind === "relationship") {
    const relationship = draft.options;
    if (draft.target.columns.length !== relationship.parentColumns.length)
      errors.push("Child and parent keys must have the same number of columns.");
    const parent = findObject(catalog.objects, relationship.parent);
    if (!parent) errors.push("Choose a current parent table or view.");
    const parentColumns = columnsFor(catalog, parent);
    draft.target.columns.forEach((child, index) => {
      const childType = columns.find((column) => column.name === child)?.dataType;
      const parentType = parentColumns.find(
        (column) => column.name === relationship.parentColumns[index],
      )?.dataType;
      if (childType && parentType && childType !== parentType)
        errors.push(`Relationship key ${index + 1} types differ (${childType} vs ${parentType}).`);
    });
  }
  if (draft.options.kind === "custom_sql") {
    if (!draft.options.sql.trim()) errors.push("Enter one read-only SELECT or WITH statement.");
    if (new TextEncoder().encode(draft.options.sql).length > CUSTOM_SQL_LIMIT)
      errors.push("Custom SQL exceeds 256 KiB.");
  }
  return [...new Set(errors)];
}

function defaultOptions(type: QualityCheckType): QualityCheckOptions {
  if (type === "not_empty") return { kind: "not_empty" };
  if (type === "not_null") return { kind: "not_null" };
  if (type === "unique") return { kind: "unique" };
  if (type === "accepted_values") return { kind: "accepted_values", values: [] };
  if (type === "range")
    return {
      kind: "range",
      minimum: null,
      maximum: null,
      inclusiveMinimum: true,
      inclusiveMaximum: true,
    };
  if (type === "relationship")
    return {
      kind: "relationship",
      parent: { database: "", schema: "", object: "", columns: [] },
      parentColumns: [],
    };
  if (type === "freshness") return { kind: "freshness", maximumAgeSeconds: 86_400 };
  return { kind: "custom_sql", sql: "SELECT *\nFROM table_name\nWHERE /* failing condition */" };
}

function defaultNullPolicy(type: QualityCheckType): QualityCheckDraft["nullPolicy"] {
  return ["unique", "range", "relationship", "freshness"].includes(type)
    ? "pass_on_null"
    : "fail_on_null";
}

function explainDraft(draft: QualityCheckDraft): string[] {
  const target = `${draft.target.database}.${draft.target.schema}.${draft.target.object}`;
  const nulls =
    draft.nullPolicy === "fail_on_null"
      ? "NULL values count as failures."
      : "NULL values are ignored by this expectation.";
  if (draft.options.kind === "not_empty")
    return [
      `DuckDB counts rows in ${target}.`,
      "The check fails with one failure when the object has no rows.",
    ];
  if (draft.options.kind === "not_null")
    return [
      `DuckDB selects rows where ${draft.target.columns[0]} IS NULL.`,
      "The check passes only when that exact failure count is zero.",
    ];
  if (draft.options.kind === "unique")
    return [
      `DuckDB groups the selected key columns and finds repeated groups.`,
      `Rows belonging to repeated non-NULL keys fail. ${nulls}`,
      "Distinct profile counts do not prove this row-level uniqueness rule.",
    ];
  if (draft.options.kind === "accepted_values")
    return [
      `DuckDB compares the selected column with the visible accepted-value list.`,
      `Rows outside the list fail. ${nulls}`,
    ];
  if (draft.options.kind === "range")
    return [
      `DuckDB compares the selected column with the inclusive bounds shown in the form.`,
      `Rows below the minimum or above the maximum fail. ${nulls}`,
    ];
  if (draft.options.kind === "relationship")
    return [
      "DuckDB left-joins each child key to the selected parent key.",
      `Child rows without a matching parent fail. ${nulls}`,
    ];
  if (draft.options.kind === "freshness")
    return [
      "DuckDB finds the latest non-NULL timestamp and compares it with the current time.",
      `The check fails when the latest value is older than the chosen threshold. ${nulls}`,
    ];
  return [
    "DuckDB validates this as exactly one read-only SELECT or WITH statement.",
    "Every row returned by the custom SQL represents one failure.",
    "Running custom SQL always requires a separate confirmation.",
  ];
}

function parseScalar(value: string): string | number | boolean {
  if (/^(true|false)$/i.test(value)) return value.toLowerCase() === "true";
  if (/^-?(?:\d+\.?\d*|\.\d+)$/.test(value)) return Number(value);
  return value;
}
function cloneDraft(draft: QualityCheckDraft): QualityCheckDraft {
  return JSON.parse(JSON.stringify(draft)) as QualityCheckDraft;
}
function serializeDraft(builder: BuilderState): string {
  return JSON.stringify(materializeDraft(builder));
}
function objectKey(object: CatalogObject): string {
  return `${object.database}\u0000${object.schema}\u0000${object.name}\u0000${object.kind}`;
}
function findObject(
  objects: CatalogObject[],
  target: { database: string; schema: string; object?: string; name?: string },
): CatalogObject | undefined {
  const name = target.object ?? target.name;
  return objects.find(
    (object) =>
      object.database === target.database &&
      object.schema === target.schema &&
      object.name === name,
  );
}
function columnsFor(catalog: ProjectCatalog, object?: CatalogObject): CatalogColumn[] {
  return object
    ? catalog.columns.filter(
        (column) =>
          column.database === object.database &&
          column.schema === object.schema &&
          column.object === object.name,
      )
    : [];
}
function isRangeType(type: string): boolean {
  return /^(U?TINYINT|U?SMALLINT|U?INTEGER|U?BIGINT|UHUGEINT|HUGEINT|FLOAT|DOUBLE|REAL|DECIMAL|NUMERIC|DATE|TIME|TIMESTAMP)/.test(
    type.toUpperCase(),
  );
}
function typeLabel(type: QualityCheckType): string {
  return {
    not_empty: "Table is not empty",
    not_null: "Value is not NULL",
    unique: "Key is unique",
    accepted_values: "Value is accepted",
    range: "Value is in range",
    relationship: "Key has a parent",
    freshness: "Timestamp is fresh",
    custom_sql: "Custom read-only SQL",
  }[type];
}
function outcomeLabel(outcome: QualityCheckRun["outcome"]): string {
  return { pass: "Passed", fail: "Failed", error: "Error", cancelled: "Cancelled" }[outcome];
}
function friendlyError(error: unknown): string {
  const message = String(error);
  if (message.includes("QualityCheckConflict"))
    return "A check with this name already exists in the project.";
  if (message.includes("history must be cleared"))
    return "Clear this check’s run history before deleting its definition.";
  return message.replace(/^Error:\s*/, "");
}
