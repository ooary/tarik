import {
  ArrowLeftIcon,
  ArrowSquareOutIcon,
  CaretDownIcon,
  CheckSquareIcon,
  ClockIcon,
  CopyIcon,
  GearSixIcon,
  MagnifyingGlassIcon,
  PlayIcon,
  SpinnerGapIcon,
  StopIcon,
} from "@phosphor-icons/react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  cancelProfile,
  executeProfile,
  getProfileStatus,
  type ActiveProject,
  type CatalogColumn,
  type CatalogObject,
  type MetricProvenance,
  type ProfileMetric,
  type ProfileMetricKind,
  type ProfileMode,
  type ProfileSqlEvidence,
  type ProfileStatus,
  type ProjectCatalog,
  type QualityCheckDraft,
  type SourceRecord,
} from "../../lib/commands";
import "./profile.css";

const POLL_MS = 150;
const DEFAULT_COLUMN_COUNT = 12;

type ProfileView = "columns" | "metrics" | "sql";

export interface ProfileCheckPrefill {
  draft: QualityCheckDraft;
  observation: ProfileMetric;
}

interface ProfileWorkspaceProps {
  project: ActiveProject;
  object: CatalogObject;
  catalog: ProjectCatalog;
  openedCatalogRevision: string;
  source: SourceRecord | null;
  sourceChanged: boolean;
  onClose: () => void;
  onCreateCheck: (prefill: ProfileCheckPrefill) => void;
  onOpenSql: (sql: string, title: string) => void;
  onRefresh: () => Promise<void>;
}

interface MetricGroupDefinition {
  id: string;
  title: string;
  description: string;
  kinds: ProfileMetricKind[];
}

const METRIC_GROUPS: MetricGroupDefinition[] = [
  {
    id: "completeness",
    title: "Completeness",
    description: "NULL means a value is missing. It is different from empty text.",
    kinds: ["null_count", "null_rate"],
  },
  {
    id: "cardinality",
    title: "Cardinality",
    description: "Distinct values describe variety. They do not prove that every row is unique.",
    kinds: ["distinct_count"],
  },
  {
    id: "range",
    title: "Range",
    description: "Range measurements use non-NULL numeric or temporal values.",
    kinds: ["minimum", "maximum", "average"],
  },
  {
    id: "text-shape",
    title: "Text shape",
    description: "Text lengths count characters in non-NULL values.",
    kinds: ["text_length_minimum", "text_length_maximum", "text_length_average"],
  },
  {
    id: "values",
    title: "Values",
    description: "Common values are exact. Representative values are bounded examples.",
    kinds: ["common_values", "representative_values"],
  },
];

export function ProfileWorkspace({
  project,
  object,
  catalog,
  openedCatalogRevision,
  source,
  sourceChanged,
  onClose,
  onCreateCheck,
  onOpenSql,
  onRefresh,
}: ProfileWorkspaceProps) {
  const objectColumns = useMemo(
    () =>
      catalog.columns.filter(
        (column) =>
          column.database === object.database &&
          column.schema === object.schema &&
          column.object === object.name,
      ),
    [catalog.columns, object],
  );
  const [selected, setSelected] = useState(() =>
    objectColumns.slice(0, DEFAULT_COLUMN_COUNT).map((column) => column.name),
  );
  const [mode, setMode] = useState<ProfileMode>("approximate");
  const [status, setStatus] = useState<ProfileStatus | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [setupOpen, setSetupOpen] = useState(true);
  const [search, setSearch] = useState("");
  const [columnSearch, setColumnSearch] = useState("");
  const [activeColumn, setActiveColumn] = useState(() => selected[0] ?? "");
  const [activeMetricKey, setActiveMetricKey] = useState("");
  const [activeView, setActiveView] = useState<ProfileView>("columns");
  const [error, setError] = useState<string | null>(null);
  const [elapsedMs, setElapsedMs] = useState(0);
  const backButton = useRef<HTMLButtonElement | null>(null);
  const columnButtons = useRef(new Map<string, HTMLButtonElement>());
  const submittingRef = useRef(false);
  const cancellingRef = useRef(false);
  const mounted = useRef(true);
  const activeProfileId = useRef<string | null>(null);
  const runStartedAt = useRef<number | null>(null);

  const active = status?.state === "queued" || status?.state === "running";
  const loading = submitting || active;
  const polledProfileId = active ? status.profileId : undefined;
  const targetExists = catalog.objects.some(
    (candidate) =>
      candidate.database === object.database &&
      candidate.schema === object.schema &&
      candidate.name === object.name &&
      candidate.kind === object.kind,
  );
  const stale = Boolean(
    sourceChanged ||
    !targetExists ||
    (openedCatalogRevision && catalog.revision && openedCatalogRevision !== catalog.revision),
  );
  const snapshot = status?.state === "succeeded" ? status.snapshot : null;
  const profiledColumns = useMemo(() => {
    const names = new Set(
      (snapshot?.metrics ?? [])
        .map((metric) => metric.column)
        .filter((column): column is string => Boolean(column)),
    );
    return objectColumns.filter((column) => names.has(column.name));
  }, [objectColumns, snapshot]);
  const visibleProfiledColumns = useMemo(() => {
    const term = columnSearch.trim().toLocaleLowerCase();
    return term
      ? profiledColumns.filter(
          (column) =>
            column.name.toLocaleLowerCase().includes(term) ||
            column.dataType.toLocaleLowerCase().includes(term),
        )
      : profiledColumns;
  }, [columnSearch, profiledColumns]);
  const activeCatalogColumn = objectColumns.find((column) => column.name === activeColumn) ?? null;
  const activeMetrics = useMemo(
    () => (snapshot?.metrics ?? []).filter((metric) => metric.column === activeColumn),
    [activeColumn, snapshot],
  );
  const selectedMetric = useMemo(() => {
    const direct = (snapshot?.metrics ?? []).find(
      (metric) => metricKey(metric) === activeMetricKey && !metric.unavailableReason,
    );
    return direct ?? activeMetrics.find((metric) => !metric.unavailableReason) ?? null;
  }, [activeMetricKey, activeMetrics, snapshot]);
  const evidenceStatements = useMemo(
    () => evidenceForMetric(snapshot?.statements ?? [], selectedMetric),
    [selectedMetric, snapshot],
  );

  useEffect(() => {
    mounted.current = true;
    backButton.current?.focus();
    return () => {
      mounted.current = false;
      const profileId = activeProfileId.current;
      activeProfileId.current = null;
      if (profileId) void cancelProfile(profileId).catch(() => undefined);
    };
  }, []);

  useEffect(() => {
    activeProfileId.current = active && status ? status.profileId : null;
  }, [active, status]);

  useEffect(() => {
    if (!loading) return;
    if (runStartedAt.current === null) runStartedAt.current = Date.now();
    const updateElapsed = () => {
      if (runStartedAt.current !== null) setElapsedMs(Date.now() - runStartedAt.current);
    };
    updateElapsed();
    const timer = window.setInterval(updateElapsed, 1_000);
    return () => window.clearInterval(timer);
  }, [loading, status?.profileId]);

  useEffect(() => {
    if (!polledProfileId) return;
    let disposed = false;
    let timer = 0;
    const poll = async () => {
      try {
        const next = await getProfileStatus(polledProfileId);
        if (disposed || !mounted.current) return;
        if (!next) {
          setError("Profile status expired. Run the profile again.");
          setStatus((current) =>
            current
              ? {
                  ...current,
                  state: "failed",
                  error: { code: "profile.not_found", message: "Profile status expired." },
                }
              : current,
          );
          return;
        }
        setStatus(next);
        setError(next.state === "failed" ? (next.error?.message ?? "Profile failed.") : null);
        if (next.state === "succeeded" && next.snapshot) {
          const firstColumn = next.snapshot.metrics.find((metric) => metric.column)?.column;
          if (firstColumn) setActiveColumn(firstColumn);
          setSetupOpen(false);
          setActiveView("metrics");
        }
        if (next.state === "queued" || next.state === "running") {
          timer = window.setTimeout(() => void poll(), POLL_MS);
        }
      } catch (cause) {
        if (!disposed && mounted.current) {
          setError(`Profile status could not be refreshed: ${String(cause)}`);
          timer = window.setTimeout(() => void poll(), POLL_MS);
        }
      }
    };
    timer = window.setTimeout(() => void poll(), POLL_MS);
    return () => {
      disposed = true;
      window.clearTimeout(timer);
    };
  }, [polledProfileId]);

  async function run() {
    const revision = catalog.revision;
    if (!revision || selected.length === 0 || active || submittingRef.current || stale) return;
    submittingRef.current = true;
    runStartedAt.current = Date.now();
    setElapsedMs(0);
    setSetupOpen(false);
    setSubmitting(true);
    setError(null);
    setStatus(null);
    try {
      const next = await executeProfile({
        projectId: project.id,
        target: {
          database: object.database,
          schema: object.schema,
          name: object.name,
          kind: object.kind,
        },
        columns: objectColumns
          .filter((column) => selected.includes(column.name))
          .map((column) => ({ name: column.name, dataType: column.dataType })),
        catalogRevision: revision,
        mode,
      });
      if (!mounted.current) {
        if (next.state === "queued" || next.state === "running") {
          void cancelProfile(next.profileId).catch(() => undefined);
        }
        return;
      }
      setStatus(next);
    } catch (cause) {
      if (mounted.current) setError(String(cause));
    } finally {
      submittingRef.current = false;
      if (mounted.current) setSubmitting(false);
    }
  }

  async function cancel() {
    if (!status || !active || cancellingRef.current) return;
    cancellingRef.current = true;
    setCancelling(true);
    try {
      const next = await cancelProfile(status.profileId);
      if (next) setStatus(next);
    } catch (cause) {
      setError(String(cause));
    } finally {
      cancellingRef.current = false;
      if (mounted.current) setCancelling(false);
    }
  }

  async function refreshSetup() {
    if (active || refreshing) return;
    setRefreshing(true);
    setError(null);
    try {
      await onRefresh();
    } catch (cause) {
      if (mounted.current) setError(`Profile setup could not be refreshed: ${String(cause)}`);
    } finally {
      if (mounted.current) setRefreshing(false);
    }
  }

  function close() {
    mounted.current = false;
    if (active && status) {
      activeProfileId.current = null;
      void cancelProfile(status.profileId).catch(() => undefined);
    }
    onClose();
  }

  function chooseColumn(column: string) {
    setActiveColumn(column);
    const first = (snapshot?.metrics ?? []).find(
      (metric) => metric.column === column && !metric.unavailableReason,
    );
    setActiveMetricKey(first ? metricKey(first) : "");
    setActiveView("metrics");
  }

  function moveColumnFocus(event: React.KeyboardEvent<HTMLButtonElement>, index: number) {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const nextIndex =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? visibleProfiledColumns.length - 1
          : event.key === "ArrowDown"
            ? Math.min(index + 1, visibleProfiledColumns.length - 1)
            : Math.max(index - 1, 0);
    const next = visibleProfiledColumns[nextIndex];
    if (next) columnButtons.current.get(next.name)?.focus();
  }

  function moveViewFocus(event: React.KeyboardEvent<HTMLButtonElement>, index: number) {
    const views: ProfileView[] = ["columns", "metrics", "sql"];
    if (!["ArrowRight", "ArrowLeft", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const nextIndex =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? views.length - 1
          : event.key === "ArrowRight"
            ? (index + 1) % views.length
            : (index - 1 + views.length) % views.length;
    const next = views[nextIndex];
    setActiveView(next);
    document.getElementById(`profile-${next}-tab`)?.focus();
  }

  const stateMessage = profileStateMessage(status);
  const sourceDescription = describeSource(source, object);

  return (
    <section aria-label={`Profile ${object.name}`} className="profile-workspace">
      <header className="profile-header">
        <button
          aria-label="Back to query editor"
          className="icon-button"
          onClick={close}
          ref={backButton}
          type="button"
        >
          <ArrowLeftIcon aria-hidden="true" size={16} />
        </button>
        <div className="profile-heading">
          <h1>Profile: {object.name}</h1>
          <p>
            <code>
              {object.database}.{object.schema}.{object.name}
            </code>
            <span>{sourceDescription.label}</span>
            <span
              className={`profile-source-state profile-source-state-${source?.state ?? "ready"}`}
            >
              {source?.state === "missing" ? "Source missing" : "Ready"}
            </span>
          </p>
        </div>
        <div className="profile-actions">
          <button
            aria-expanded={setupOpen}
            className="toolbar-button profile-settings-trigger"
            onClick={() => setSetupOpen((current) => !current)}
            type="button"
          >
            <GearSixIcon aria-hidden="true" size={14} />
            Settings
          </button>
          {active ? (
            <button
              className="toolbar-button"
              disabled={cancelling}
              onClick={() => void cancel()}
              type="button"
            >
              <StopIcon aria-hidden="true" size={14} />
              {cancelling ? "Cancelling profile" : "Cancel profile"}
            </button>
          ) : (
            <button
              className="toolbar-button profile-run"
              disabled={
                !catalog.revision ||
                selected.length === 0 ||
                source?.state === "missing" ||
                stale ||
                submitting
              }
              onClick={() => void run()}
              type="button"
            >
              <PlayIcon aria-hidden="true" size={14} weight="fill" />
              {submitting ? "Starting" : snapshot ? "Run again" : "Run profile"}
            </button>
          )}
        </div>
      </header>

      <div className="profile-body">
        {setupOpen && (
          <section aria-label="Profile settings" className="profile-setup">
            <div className="profile-setup-summary">
              <div>
                <strong>
                  {selected.length} {selected.length === 1 ? "column" : "columns"} selected
                </strong>
                <span>
                  Choose up to 100. Tarik selected the first{" "}
                  {Math.min(DEFAULT_COLUMN_COUNT, objectColumns.length)}.
                </span>
              </div>
              <div>
                <strong>
                  Distinct counts: {mode === "approximate" ? "Fast estimate" : "Exact scan"}
                </strong>
                <span>
                  {mode === "approximate"
                    ? "Recommended for exploration."
                    : "May take longer on large columns."}
                </span>
              </div>
              <div>
                <strong>Local, explicit scan</strong>
                <span>No scan runs until you choose Run profile.</span>
              </div>
            </div>
            <div className="profile-setup-controls">
              <div className="profile-column-picker">
                <div className="profile-picker-heading">
                  <label htmlFor="profile-column-search">Columns</label>
                  <span>
                    {selected.length} of {objectColumns.length}
                  </span>
                </div>
                <div className="profile-search-field">
                  <MagnifyingGlassIcon aria-hidden="true" size={14} />
                  <input
                    disabled={active || submitting || cancelling}
                    id="profile-column-search"
                    onChange={(event) => setSearch(event.currentTarget.value)}
                    placeholder="Find a column"
                    type="search"
                    value={search}
                  />
                </div>
                <div className="profile-column-actions">
                  <button
                    className="subtle-button"
                    disabled={active || submitting || cancelling}
                    onClick={() =>
                      setSelected(objectColumns.slice(0, 100).map((column) => column.name))
                    }
                    type="button"
                  >
                    Select up to 100
                  </button>
                  <button
                    className="subtle-button"
                    disabled={active || submitting || cancelling}
                    onClick={() => setSelected([])}
                    type="button"
                  >
                    Clear
                  </button>
                </div>
                <div className="profile-column-checklist">
                  {objectColumns
                    .filter((column) =>
                      `${column.name} ${column.dataType}`
                        .toLocaleLowerCase()
                        .includes(search.toLocaleLowerCase()),
                    )
                    .map((column) => (
                      <label key={column.name}>
                        <input
                          checked={selected.includes(column.name)}
                          disabled={
                            active ||
                            submitting ||
                            cancelling ||
                            (!selected.includes(column.name) && selected.length >= 100)
                          }
                          onChange={(event) => {
                            const checked = event.currentTarget.checked;
                            const columnName = column.name;
                            setSelected((current) =>
                              checked
                                ? [...current, columnName]
                                : current.filter((name) => name !== columnName),
                            );
                          }}
                          type="checkbox"
                        />
                        <span>{column.name}</span>
                        <code>{column.dataType}</code>
                      </label>
                    ))}
                </div>
              </div>

              <fieldset className="profile-mode" disabled={active || submitting || cancelling}>
                <legend>Distinct counts</legend>
                <label>
                  <input
                    checked={mode === "approximate"}
                    name="profile-mode"
                    onChange={() => setMode("approximate")}
                    type="radio"
                  />
                  <span>
                    <strong>Fast estimate</strong>
                    <small>Uses an estimator and is labeled Approximate.</small>
                  </span>
                </label>
                <label>
                  <input
                    checked={mode === "exact"}
                    name="profile-mode"
                    onChange={() => setMode("exact")}
                    type="radio"
                  />
                  <span>
                    <strong>Exact scan</strong>
                    <small>Counts each distinct value and may use more time and memory.</small>
                  </span>
                </label>
                <p>Other measurements keep their own Exact or Sampled label.</p>
              </fieldset>

              <section aria-label="Local explicit scan details" className="profile-scan-scope">
                <strong>Scan behavior</strong>
                <dl>
                  <div>
                    <dt>Explicit start</dt>
                    <dd>Nothing runs until you choose Run profile.</dd>
                  </div>
                  <div>
                    <dt>Local execution</dt>
                    <dd>DuckDB reads the selected columns on this device.</dd>
                  </div>
                  <div>
                    <dt>Temporary results</dt>
                    <dd>Profile values are not saved to history.</dd>
                  </div>
                </dl>
              </section>
            </div>
          </section>
        )}

        {(stateMessage || stale || error) && (
          <div aria-live="polite" className="profile-state-line">
            {stateMessage && <span>{stateMessage}</span>}
            {stale && (
              <div className="profile-stale-message">
                <strong>The table or source changed after Profile opened.</strong>
                <button
                  className="subtle-button"
                  disabled={active || refreshing}
                  onClick={() => void refreshSetup()}
                  type="button"
                >
                  {refreshing ? "Refreshing" : "Refresh setup"}
                </button>
              </div>
            )}
            {error && <strong role="alert">{error}</strong>}
          </div>
        )}

        <div className="profile-content">
          {!status && !error && !submitting && (
            <div className="profile-ready-state">
              <div className="profile-ready-note">
                <div>
                  <strong>
                    Ready to inspect {selected.length}{" "}
                    {selected.length === 1 ? "column" : "columns"}
                  </strong>
                  <span>Review the settings, then run the profile.</span>
                </div>
                <span>Results and SQL evidence appear here after the scan completes.</span>
              </div>
              <div aria-label="Profile result areas" className="profile-ready-layout" role="region">
                <div>
                  <strong>Columns</strong>
                  <span>Choose the fields to inspect in Settings.</span>
                </div>
                <div>
                  <strong>Measurements</strong>
                  <span>
                    Completeness, cardinality, shape, and bounded values appear after the scan.
                  </span>
                </div>
                <div>
                  <strong>SQL evidence</strong>
                  <span>Inspect the exact SQL used for a selected measurement.</span>
                </div>
              </div>
            </div>
          )}
          {loading && (
            <ProfileLoading
              elapsedMs={Math.max(elapsedMs, status?.durationMs ?? 0)}
              selectedCount={selected.length}
              state={submitting ? "starting" : status?.state === "running" ? "running" : "queued"}
            />
          )}
          {status?.state === "failed" && (
            <div className="profile-empty">
              <strong>Profile did not complete</strong>
              <span>
                Review the error, refresh the setup if needed, then run the profile again.
              </span>
            </div>
          )}
          {status?.state === "cancelled" && (
            <div className="profile-empty">
              <strong>No partial profile was kept</strong>
              <span>Change the selected columns or distinct-count method, then run again.</span>
            </div>
          )}
          {snapshot && (
            <>
              <ProfileSummary
                columnCount={profiledColumns.length}
                durationMs={status?.durationMs ?? 0}
                onInspectRows={() => {
                  const rowMetric = snapshot.metrics.find((metric) => metric.kind === "row_count");
                  if (rowMetric) setActiveMetricKey(metricKey(rowMetric));
                  setActiveView("sql");
                }}
                observedAt={snapshot.observedAtUnixMs}
                rowCount={snapshot.metrics.find((metric) => metric.kind === "row_count") ?? null}
              />
              <div aria-label="Profile views" className="profile-view-tabs" role="tablist">
                {(["columns", "metrics", "sql"] as ProfileView[]).map((view, index) => (
                  <button
                    aria-controls={`profile-${view}-panel`}
                    aria-selected={activeView === view}
                    className={activeView === view ? "profile-view-tab-active" : ""}
                    id={`profile-${view}-tab`}
                    key={view}
                    onClick={() => setActiveView(view)}
                    onKeyDown={(event) => moveViewFocus(event, index)}
                    role="tab"
                    tabIndex={activeView === view ? 0 : -1}
                    type="button"
                  >
                    {view === "sql" ? "SQL evidence" : `${view[0].toUpperCase()}${view.slice(1)}`}
                  </button>
                ))}
              </div>
              <div className={`profile-browser profile-browser-${activeView}`}>
                <nav
                  aria-label="Profiled columns"
                  aria-labelledby="profile-columns-tab"
                  className={`profile-column-nav profile-view-panel ${activeView === "columns" ? "profile-view-panel-active" : ""}`}
                  id="profile-columns-panel"
                >
                  <div className="profile-pane-heading">
                    <strong>Columns</strong>
                    <span>{profiledColumns.length} profiled</span>
                  </div>
                  <div className="profile-search-field profile-result-search">
                    <MagnifyingGlassIcon aria-hidden="true" size={14} />
                    <input
                      aria-label="Search profiled columns"
                      onChange={(event) => setColumnSearch(event.currentTarget.value)}
                      placeholder="Find profiled column"
                      type="search"
                      value={columnSearch}
                    />
                  </div>
                  <div className="profile-column-nav-list">
                    {visibleProfiledColumns.map((column, index) => {
                      const metrics = snapshot.metrics.filter(
                        (metric) => metric.column === column.name,
                      );
                      const nullMetric = metrics.find((metric) => metric.kind === "null_count");
                      const nullCount = numericMetric(nullMetric);
                      return (
                        <button
                          aria-current={activeColumn === column.name ? "true" : undefined}
                          className={activeColumn === column.name ? "profile-column-active" : ""}
                          key={column.name}
                          onClick={() => chooseColumn(column.name)}
                          onKeyDown={(event) => moveColumnFocus(event, index)}
                          ref={(element) => {
                            if (element) columnButtons.current.set(column.name, element);
                            else columnButtons.current.delete(column.name);
                          }}
                          type="button"
                        >
                          <span>{column.name}</span>
                          <code>{column.dataType}</code>
                          <small>
                            {nullCount === null
                              ? "NULL count unavailable"
                              : nullCount > 0
                                ? `${formatInteger(nullCount)} NULL values`
                                : "No NULL values"}
                          </small>
                        </button>
                      );
                    })}
                    {visibleProfiledColumns.length === 0 && (
                      <p>No profiled columns match this search.</p>
                    )}
                  </div>
                </nav>

                <section
                  aria-label="Column measurements"
                  aria-labelledby="profile-metrics-tab"
                  className={`profile-metrics-pane profile-view-panel ${activeView === "metrics" ? "profile-view-panel-active" : ""}`}
                  id="profile-metrics-panel"
                >
                  {activeCatalogColumn ? (
                    <ColumnMetrics
                      column={activeCatalogColumn}
                      metrics={activeMetrics}
                      onCreateCheck={(metric) =>
                        onCreateCheck(
                          createCheckPrefill(project.id, object, activeCatalogColumn.name, metric),
                        )
                      }
                      onSelectMetric={(metric) => {
                        setActiveMetricKey(metricKey(metric));
                        setActiveView("sql");
                      }}
                      selectedMetric={selectedMetric}
                    />
                  ) : (
                    <div className="profile-empty profile-pane-empty">
                      <strong>Select a column</strong>
                      <span>Choose a profiled column to inspect its measurements.</span>
                    </div>
                  )}
                </section>

                <aside
                  aria-label="SQL evidence"
                  aria-labelledby="profile-sql-tab"
                  className={`profile-evidence-pane profile-view-panel ${activeView === "sql" ? "profile-view-panel-active" : ""}`}
                  id="profile-sql-panel"
                >
                  <EvidenceInspector
                    metric={selectedMetric}
                    onOpenSql={(sql) =>
                      onOpenSql(sql, `Profile evidence: ${selectedMetric?.column ?? object.name}`)
                    }
                    statements={evidenceStatements}
                  />
                </aside>
              </div>
            </>
          )}
        </div>
      </div>
    </section>
  );
}

function ProfileSummary({
  rowCount,
  columnCount,
  observedAt,
  durationMs,
  onInspectRows,
}: {
  rowCount: ProfileMetric | null;
  columnCount: number;
  observedAt: number;
  durationMs: number;
  onInspectRows: () => void;
}) {
  return (
    <div className="profile-summary">
      <button onClick={onInspectRows} type="button">
        <strong>{formatMetricValue(rowCount?.value, "row_count")}</strong>
        <span>rows read</span>
        <small>Exact. Inspect SQL</small>
      </button>
      <div>
        <strong>{columnCount.toLocaleString("en-US")}</strong>
        <span>columns profiled</span>
      </div>
      <div>
        <strong>
          {new Date(observedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
        </strong>
        <span>observed</span>
        <small>{formatDuration(durationMs)}</small>
      </div>
    </div>
  );
}

function ColumnMetrics({
  column,
  metrics,
  selectedMetric,
  onSelectMetric,
  onCreateCheck,
}: {
  column: CatalogColumn;
  metrics: ProfileMetric[];
  selectedMetric: ProfileMetric | null;
  onSelectMetric: (metric: ProfileMetric) => void;
  onCreateCheck: (metric: ProfileMetric) => void;
}) {
  const unavailableReasons = Array.from(
    new Set(
      metrics
        .map((metric) => metric.unavailableReason)
        .filter((reason): reason is string => Boolean(reason)),
    ),
  );
  return (
    <div className="profile-column-detail">
      <header>
        <div>
          <h2>{column.name}</h2>
          <code>{column.dataType}</code>
        </div>
        <span>{column.nullable ? "Nullable" : "Required by schema"}</span>
      </header>
      {METRIC_GROUPS.map((group) => {
        const groupMetrics = metrics.filter(
          (metric) => group.kinds.includes(metric.kind) && !metric.unavailableReason,
        );
        if (groupMetrics.length === 0) return null;
        return (
          <section className="profile-metric-group" key={group.id}>
            <div className="profile-metric-group-heading">
              <div>
                <h3>{group.title}</h3>
                <p>{group.description}</p>
              </div>
              <MetricCheckAction metrics={groupMetrics} onCreateCheck={onCreateCheck} />
            </div>
            {group.id === "values" ? (
              <ValueMetrics metrics={groupMetrics} onSelectMetric={onSelectMetric} />
            ) : (
              <div className="profile-metric-list">
                {groupMetrics.map((metric) => (
                  <button
                    aria-pressed={metricKey(metric) === metricKey(selectedMetric)}
                    className={
                      metricKey(metric) === metricKey(selectedMetric) ? "profile-metric-active" : ""
                    }
                    key={metric.kind}
                    onClick={() => onSelectMetric(metric)}
                    type="button"
                  >
                    <span>{metricLabel(metric.kind)}</span>
                    <strong title={formatMetricValue(metric.value, metric.kind)}>
                      {formatMetricValue(metric.value, metric.kind)}
                    </strong>
                    <ProvenanceLabel value={metric.provenance} />
                  </button>
                ))}
              </div>
            )}
          </section>
        );
      })}
      {unavailableReasons.length > 0 && (
        <details className="profile-unavailable-details">
          <summary>Unavailable measurements</summary>
          <ul>
            {unavailableReasons.map((reason) => (
              <li key={reason}>{reason}</li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

function MetricCheckAction({
  metrics,
  onCreateCheck,
}: {
  metrics: ProfileMetric[];
  onCreateCheck: (metric: ProfileMetric) => void;
}) {
  const metric =
    metrics.find(
      (candidate) => candidate.kind === "null_count" && numericMetric(candidate) !== 0,
    ) ??
    metrics.find((candidate) => candidate.kind === "distinct_count") ??
    metrics.find((candidate) => candidate.kind === "minimum") ??
    metrics.find((candidate) => candidate.kind === "maximum");
  if (!metric || !canCreateCheck(metric)) return null;
  return (
    <button
      className="subtle-button profile-check-action"
      onClick={() => onCreateCheck(metric)}
      type="button"
    >
      <CheckSquareIcon aria-hidden="true" size={14} />
      {checkActionLabel(metric)}
    </button>
  );
}

function ValueMetrics({
  metrics,
  onSelectMetric,
}: {
  metrics: ProfileMetric[];
  onSelectMetric: (metric: ProfileMetric) => void;
}) {
  return (
    <div className="profile-value-groups">
      {metrics.map((metric) => {
        const values = Array.isArray(metric.value) ? metric.value : [];
        return (
          <details
            key={metric.kind}
            onToggle={(event) => {
              if (event.currentTarget.open) onSelectMetric(metric);
            }}
          >
            <summary>
              <span>{metricLabel(metric.kind)}</span>
              <span>{values.length} values</span>
              <ProvenanceLabel value={metric.provenance} />
              <CaretDownIcon aria-hidden="true" size={13} />
            </summary>
            <div className="profile-value-detail">
              {metric.kind === "common_values" ? (
                <table>
                  <thead>
                    <tr>
                      <th>Value</th>
                      <th>Count</th>
                    </tr>
                  </thead>
                  <tbody>
                    {values.map((entry, index) => {
                      const record = isRecord(entry) ? entry : {};
                      return (
                        <tr key={`${String(record.value)}:${index}`}>
                          <td title={String(record.value ?? "NULL")}>
                            {String(record.value ?? "NULL")}
                          </td>
                          <td>
                            {formatInteger(typeof record.count === "number" ? record.count : 0)}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              ) : (
                <ul className="profile-example-values">
                  {values.map((value, index) => (
                    <li key={`${String(value)}:${index}`}>{String(value)}</li>
                  ))}
                </ul>
              )}
              {metric.kind === "representative_values" && (
                <p>These are bounded examples, not a complete distribution.</p>
              )}
              {metric.truncated && <p>One or more displayed values were truncated.</p>}
              <button
                className="subtle-button"
                onClick={() => void navigator.clipboard?.writeText(formatValueList(metric))}
                type="button"
              >
                <CopyIcon aria-hidden="true" size={13} />
                Copy values
              </button>
            </div>
          </details>
        );
      })}
    </div>
  );
}

function EvidenceInspector({
  metric,
  statements,
  onOpenSql,
}: {
  metric: ProfileMetric | null;
  statements: ProfileSqlEvidence[];
  onOpenSql: (sql: string) => void;
}) {
  if (!metric) {
    return (
      <div className="profile-empty profile-pane-empty">
        <strong>Select a measurement</strong>
        <span>Its provenance, meaning, value, and exact executed SQL will appear here.</span>
      </div>
    );
  }
  const sql = statements.map((statement) => statement.sql.trim().replace(/;$/, "")).join(";\n\n");
  const value = formatMetricValue(metric.value, metric.kind);
  return (
    <div className="profile-evidence">
      <header className="profile-pane-heading">
        <strong>SQL evidence</strong>
        <ProvenanceLabel value={metric.provenance} />
      </header>
      <div className="profile-evidence-observation">
        <span>{metricLabel(metric.kind)}</span>
        <strong title={value}>{value}</strong>
        <p>{metricMeaning(metric)}</p>
        {metric.kind === "null_rate" && (
          <small>The rate uses the NULL-count statement and the exact table row count.</small>
        )}
      </div>
      <div className="profile-evidence-actions">
        <button
          className="subtle-button profile-copy-value"
          onClick={() => void navigator.clipboard?.writeText(value)}
          title="Copy this displayed measurement value"
          type="button"
        >
          <CopyIcon aria-hidden="true" size={13} />
          Copy value
        </button>
        <button
          className="subtle-button profile-copy-sql"
          disabled={!sql}
          onClick={() => void navigator.clipboard?.writeText(sql)}
          title="Copy the exact SQL used for this measurement"
          type="button"
        >
          <CopyIcon aria-hidden="true" size={13} />
          Copy SQL
        </button>
        <button
          className="subtle-button profile-open-sql"
          disabled={!sql}
          onClick={() => onOpenSql(sql)}
          title="Open this SQL in a new query tab without running it"
          type="button"
        >
          <ArrowSquareOutIcon aria-hidden="true" size={13} />
          Open SQL
        </button>
      </div>
      <div className="profile-sql-block">
        <div>
          <strong>
            {statements.length === 1
              ? "Statement executed"
              : `${statements.length} statements executed`}
          </strong>
          <span>Opening this SQL does not run it.</span>
        </div>
        {statements.map((statement, index) => (
          <pre key={`${statement.sql}:${index}`}>
            <code>{statement.sql}</code>
          </pre>
        ))}
      </div>
    </div>
  );
}

function ProvenanceLabel({ value }: { value: MetricProvenance }) {
  return (
    <span className={`profile-provenance profile-provenance-${value}`}>
      {value[0].toUpperCase() + value.slice(1)}
    </span>
  );
}

function ProfileLoading({
  elapsedMs,
  selectedCount,
  state,
}: {
  elapsedMs: number;
  selectedCount: number;
  state: "starting" | "queued" | "running";
}) {
  const title =
    state === "starting"
      ? "Starting profile"
      : state === "queued"
        ? "Profile is queued"
        : "Profiling selected columns";
  const detail =
    state === "running"
      ? `Reading ${selectedCount} ${selectedCount === 1 ? "column" : "columns"} and calculating bounded measurements.`
      : "Waiting for the local DuckDB session to begin the scan.";
  return (
    <div aria-label="Profile loading" className="profile-loading" role="status">
      <div aria-hidden="true" className="profile-loading-indicator">
        <SpinnerGapIcon className="profile-loading-spinner" size={24} />
      </div>
      <div className="profile-loading-copy">
        <strong>{title}</strong>
        <span>{detail}</span>
      </div>
      <div
        aria-label={`Elapsed time ${formatElapsedDuration(elapsedMs)}`}
        className="profile-elapsed"
      >
        <ClockIcon aria-hidden="true" size={15} />
        <span>Elapsed</span>
        <strong>{formatElapsedDuration(elapsedMs)}</strong>
      </div>
      <div aria-hidden="true" className="profile-loading-track">
        <span />
      </div>
    </div>
  );
}

function evidenceForMetric(
  statements: ProfileSqlEvidence[],
  metric: ProfileMetric | null,
): ProfileSqlEvidence[] {
  if (!metric) return [];
  const matching = statements.filter(
    (statement) =>
      statement.metricKinds.includes(metric.kind) &&
      (metric.column === null || statement.columns.includes(metric.column)),
  );
  if (metric.kind !== "null_rate") return matching;
  const rowCount = statements.find((statement) => statement.metricKinds.includes("row_count"));
  return rowCount ? [rowCount, ...matching] : matching;
}

function metricKey(metric: ProfileMetric | null): string {
  return metric ? `${metric.column ?? "table"}:${metric.kind}` : "";
}

function profileStateMessage(status: ProfileStatus | null): string | null {
  if (!status) return null;
  if (status.state === "queued") return "Profile queued in the local DuckDB session.";
  if (status.state === "running") return "Profile scan is running locally.";
  if (status.state === "cancelled") return "Profile cancelled. The project remains ready.";
  if (status.state === "succeeded" && status.snapshot) {
    return `Observed ${new Date(status.snapshot.observedAtUnixMs).toLocaleString()}. Completed in ${formatDuration(status.durationMs)}.`;
  }
  return null;
}

function describeSource(source: SourceRecord | null, object: CatalogObject): { label: string } {
  if (source?.kind === "linked_parquet") return { label: "Linked Parquet" };
  if (source?.kind === "linked_csv") return { label: "Linked CSV" };
  if (source?.kind === "duckdb_table") {
    const format =
      typeof source.options.format === "string" ? source.options.format.toUpperCase() : null;
    return { label: format ? `Imported ${format}` : "DuckDB table" };
  }
  return { label: object.kind === "view" ? "DuckDB view" : "DuckDB object" };
}

function metricLabel(kind: ProfileMetricKind): string {
  return {
    row_count: "Row count",
    null_count: "NULL values",
    null_rate: "NULL rate",
    distinct_count: "Distinct values",
    minimum: "Minimum",
    maximum: "Maximum",
    average: "Average",
    text_length_minimum: "Shortest text",
    text_length_maximum: "Longest text",
    text_length_average: "Average text length",
    common_values: "Common values",
    representative_values: "Representative values",
  }[kind];
}

function metricMeaning(metric: ProfileMetric): string {
  if (metric.unavailableReason) return metric.unavailableReason;
  return {
    row_count: "Rows read from this table or view.",
    null_count: "Rows where this value is missing.",
    null_rate: "Share of rows where this value is NULL, not empty text.",
    distinct_count: "Different non-NULL values. Distinct count does not prove uniqueness.",
    minimum: "Smallest non-NULL numeric or temporal value.",
    maximum: "Largest non-NULL numeric or temporal value.",
    average: "Arithmetic mean of non-NULL numeric values.",
    text_length_minimum: "Fewest characters in non-NULL text.",
    text_length_maximum: "Most characters in non-NULL text.",
    text_length_average: "Average characters in non-NULL text.",
    common_values: "Up to 20 values with the highest exact frequencies.",
    representative_values: "Up to 20 bounded examples. They are not a distribution.",
  }[metric.kind];
}

function formatMetricValue(value: unknown, kind?: ProfileMetricKind): string {
  if (value === null || value === undefined) return "NULL";
  if (kind === "null_rate" && typeof value === "number") {
    return value.toLocaleString("en-US", { style: "percent", maximumFractionDigits: 2 });
  }
  if (typeof value === "number") {
    return Number.isInteger(value)
      ? formatInteger(value)
      : value.toLocaleString("en-US", { maximumFractionDigits: 4 });
  }
  if (Array.isArray(value)) return `${value.length} values`;
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

function formatInteger(value: number): string {
  return value.toLocaleString("en-US", { maximumFractionDigits: 0 });
}

function formatDuration(durationMs: number): string {
  return durationMs < 1_000 ? `${durationMs} ms` : `${(durationMs / 1_000).toFixed(1)} s`;
}

function formatElapsedDuration(durationMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1_000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function numericMetric(metric: ProfileMetric | undefined): number | null {
  return typeof metric?.value === "number" ? metric.value : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function formatValueList(metric: ProfileMetric): string {
  if (!Array.isArray(metric.value)) return "";
  if (metric.kind === "common_values") {
    return metric.value
      .map((entry) => {
        const record = isRecord(entry) ? entry : {};
        return `${String(record.value ?? "NULL")}\t${String(record.count ?? 0)}`;
      })
      .join("\n");
  }
  return metric.value.map(String).join("\n");
}

function checkActionLabel(metric: ProfileMetric): string {
  if (metric.kind === "null_count") return "Create not-null check";
  if (metric.kind === "distinct_count") return "Review uniqueness check";
  if (metric.kind === "minimum") return "Use minimum in range check";
  return "Use maximum in range check";
}

function createCheckPrefill(
  projectId: string,
  object: CatalogObject,
  column: string,
  metric: ProfileMetric,
): ProfileCheckPrefill {
  const label =
    metric.kind === "null_count"
      ? "is not NULL"
      : metric.kind === "distinct_count"
        ? "is unique"
        : "stays in range";
  const options: QualityCheckDraft["options"] =
    metric.kind === "null_count"
      ? { kind: "not_null" }
      : metric.kind === "distinct_count"
        ? { kind: "unique" }
        : {
            kind: "range",
            minimum: metric.kind === "minimum" ? metric.value : null,
            maximum: metric.kind === "maximum" ? metric.value : null,
            inclusiveMinimum: true,
            inclusiveMaximum: true,
          };
  return {
    draft: {
      projectId,
      name: `${column} ${label}`,
      target: {
        database: object.database,
        schema: object.schema,
        object: object.name,
        columns: [column],
      },
      options,
      nullPolicy: options.kind === "unique" ? "pass_on_null" : "fail_on_null",
      severity: "warning",
      enabled: true,
    },
    observation: { ...metric },
  };
}

function canCreateCheck(metric: ProfileMetric): boolean {
  return (
    !metric.unavailableReason &&
    ["null_count", "distinct_count", "minimum", "maximum"].includes(metric.kind)
  );
}
