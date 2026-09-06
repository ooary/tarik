import {
  ArrowLeftIcon,
  CheckSquareIcon,
  CopyIcon,
  PlayIcon,
  StopIcon,
} from "@phosphor-icons/react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  cancelProfile,
  executeProfile,
  getProfileStatus,
  type ActiveProject,
  type CatalogObject,
  type MetricProvenance,
  type ProfileMetric,
  type ProfileMode,
  type ProfileStatus,
  type ProjectCatalog,
  type QualityCheckDraft,
  type SourceRecord,
} from "../../lib/commands";
import "./profile.css";

const POLL_MS = 150;

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
}

export function ProfileWorkspace({
  project,
  object,
  catalog,
  openedCatalogRevision,
  source,
  sourceChanged,
  onClose,
  onCreateCheck,
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
    objectColumns.slice(0, 100).map((column) => column.name),
  );
  const [mode, setMode] = useState<ProfileMode>("approximate");
  const [status, setStatus] = useState<ProfileStatus | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const backButton = useRef<HTMLButtonElement | null>(null);
  const submittingRef = useRef(false);
  const cancellingRef = useRef(false);
  const mounted = useRef(true);
  const activeProfileId = useRef<string | null>(null);
  const active = status?.state === "queued" || status?.state === "running";
  const polledProfileId = active ? status?.profileId : undefined;
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
        if (next.state === "failed") setError(next.error?.message ?? "Profile failed.");
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

  function close() {
    mounted.current = false;
    if (active && status) {
      activeProfileId.current = null;
      void cancelProfile(status.profileId).catch(() => undefined);
    }
    onClose();
  }

  const grouped = useMemo(() => groupMetrics(status?.snapshot?.metrics ?? []), [status?.snapshot]);

  return (
    <section aria-label={`Profile ${object.name}`} className="profile-workspace">
      <header className="profile-header">
        <button
          className="icon-button"
          onClick={close}
          ref={backButton}
          type="button"
          aria-label="Back to query editor"
        >
          <ArrowLeftIcon aria-hidden="true" size={16} />
        </button>
        <div className="profile-heading">
          <h1>Profile data</h1>
          <p>
            <code>
              {object.database}.{object.schema}.{object.name}
            </code>
            <span>{object.kind}</span>
            <span>
              {source?.state === "missing"
                ? "Missing source"
                : source?.kind === "linked_parquet"
                  ? "Linked Parquet"
                  : "DuckDB object"}
            </span>
          </p>
        </div>
        <div className="profile-actions">
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
              {submitting ? "Starting profile" : "Run profile"}
            </button>
          )}
        </div>
      </header>

      <div className="profile-controls">
        <fieldset disabled={active || submitting || cancelling}>
          <legend>Distinct-count method</legend>
          <label>
            <input
              checked={mode === "approximate"}
              name="profile-mode"
              onChange={() => setMode("approximate")}
              type="radio"
            />{" "}
            Approximate <small>Recommended. Faster and clearly labeled.</small>
          </label>
          <label>
            <input
              checked={mode === "exact"}
              name="profile-mode"
              onChange={() => setMode("exact")}
              type="radio"
            />{" "}
            Exact <small>May scan and hash every distinct value.</small>
          </label>
        </fieldset>
        <div className="profile-column-picker">
          <div>
            <strong>Columns</strong>
            <span>
              {selected.length} of {objectColumns.length} selected · maximum 100
            </span>
          </div>
          <div className="profile-column-actions">
            <button
              className="subtle-button"
              disabled={active || submitting || cancelling}
              onClick={() => setSelected(objectColumns.slice(0, 100).map((column) => column.name))}
              type="button"
            >
              Select all
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
          <div className="profile-column-list">
            {objectColumns.map((column) => (
              <label key={column.name}>
                <input
                  checked={selected.includes(column.name)}
                  disabled={
                    active ||
                    submitting ||
                    cancelling ||
                    (!selected.includes(column.name) && selected.length >= 100)
                  }
                  onChange={(event) =>
                    setSelected((current) =>
                      event.currentTarget.checked
                        ? [...current, column.name]
                        : current.filter((name) => name !== column.name),
                    )
                  }
                  type="checkbox"
                />
                <span>{column.name}</span>
                <code>{column.dataType}</code>
              </label>
            ))}
          </div>
        </div>
        <aside className="profile-cost-note">
          <strong>No scan runs when this workspace opens.</strong>
          <p>
            Run profile reads the selected columns locally in DuckDB. Full scans can take time, but
            Tarik returns only bounded summaries.
          </p>
          <p>
            <b>Exact</b> means calculated from the scan. <b>Approximate</b> uses a bounded estimator
            and can differ slightly from an exact count. <b>Sampled</b> shows up to 20
            representative values; it does not describe the full distribution.
          </p>
        </aside>
      </div>

      <div aria-live="polite" className="profile-state-line">
        {active && (
          <span>
            {status?.state === "queued" ? "Profile queued" : "Profiling selected columns"} ·{" "}
            {formatDuration(status?.durationMs ?? 0)}
          </span>
        )}
        {status?.state === "cancelled" && (
          <span>Profile cancelled. The project remains ready.</span>
        )}
        {status?.state === "succeeded" && (
          <span>
            {status.snapshot
              ? `Observed ${new Date(status.snapshot.observedAtUnixMs).toLocaleString()}`
              : "Observation time unavailable"}{" "}
            · {formatDuration(status.durationMs)}
          </span>
        )}
        {stale && (
          <strong>
            The catalog or linked source changed after Profile opened. Return to Explorer and open a
            fresh profile.
          </strong>
        )}
        {error && <strong role="alert">{error}</strong>}
      </div>

      <div className="profile-results">
        {!status && (
          <div className="profile-empty">
            <strong>Choose columns, then run the profile.</strong>
            <span>
              Results will appear as a dense metric table with provenance beside every value.
            </span>
          </div>
        )}
        {active && <ProfileSkeleton />}
        {status?.state === "failed" && (
          <div className="profile-empty" role="alert">
            <strong>Profile did not complete.</strong>
            <span>{status.error?.message ?? "Review the error above, then run it again."}</span>
          </div>
        )}
        {status?.state === "cancelled" && (
          <div className="profile-empty">
            <strong>No partial metrics were kept.</strong>
            <span>Adjust the selected columns or method, then run again when ready.</span>
          </div>
        )}
        {status?.state === "succeeded" && status.snapshot && (
          <table>
            <thead>
              <tr>
                <th>Column</th>
                <th>Metric</th>
                <th>Value</th>
                <th>Provenance</th>
                <th>Meaning</th>
                <th>
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {grouped.map(({ column, metric }) => {
                const catalogColumn = objectColumns.find((candidate) => candidate.name === column);
                return (
                  <tr key={`${column ?? "table"}:${metric.kind}`}>
                    <td>
                      <code>{column ?? "Table"}</code>
                    </td>
                    <td>{metricLabel(metric.kind)}</td>
                    <td>
                      <ProfileValue metric={metric} />
                    </td>
                    <td>
                      {metric.unavailableReason ? (
                        <span className="profile-unavailable">Not applicable</span>
                      ) : (
                        <ProvenanceLabel value={metric.provenance} />
                      )}
                    </td>
                    <td>{metricMeaning(metric)}</td>
                    <td>
                      {catalogColumn && canCreateCheck(metric) && (
                        <button
                          aria-label={`Create check from ${column} ${metricLabel(metric.kind)}`}
                          className="icon-button"
                          onClick={() =>
                            onCreateCheck(
                              createCheckPrefill(project.id, object, catalogColumn.name, metric),
                            )
                          }
                          title="Create check (review before saving)"
                          type="button"
                        >
                          <CheckSquareIcon aria-hidden="true" size={15} />
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}

function groupMetrics(
  metrics: ProfileMetric[],
): Array<{ column: string | null; metric: ProfileMetric }> {
  return metrics.map((metric) => ({ column: metric.column, metric }));
}

function ProfileValue({ metric }: { metric: ProfileMetric }) {
  const [expanded, setExpanded] = useState(false);
  if (metric.unavailableReason) return <span className="profile-unavailable">—</span>;
  if (Array.isArray(metric.value)) {
    return (
      <div className="profile-value-list">
        <button
          className="subtle-button"
          onClick={() => setExpanded((current) => !current)}
          type="button"
        >
          {expanded ? "Hide" : "Show"} {metric.value.length} bounded values
        </button>
        {expanded && (
          <div className="profile-value-list-expanded">
            {metric.truncated && <small>One or more values were truncated</small>}
            <button
              className="subtle-button"
              onClick={() =>
                void navigator.clipboard?.writeText(formatMetricValue(metric.value, metric.kind))
              }
              type="button"
            >
              <CopyIcon aria-hidden="true" size={13} /> Copy values
            </button>
            <pre>{formatMetricValue(metric.value, metric.kind)}</pre>
          </div>
        )}
      </div>
    );
  }
  const text = formatMetricValue(metric.value, metric.kind);
  return (
    <span className="profile-value">
      <span title={text}>{text}</span>
      <button
        aria-label="Copy metric value"
        className="icon-button"
        onClick={() => void navigator.clipboard?.writeText(text)}
        type="button"
      >
        <CopyIcon aria-hidden="true" size={14} />
      </button>
      {metric.truncated && <small>truncated</small>}
    </span>
  );
}

function ProvenanceLabel({ value }: { value: MetricProvenance }) {
  return (
    <span className={`profile-provenance profile-provenance-${value}`}>
      {value[0].toUpperCase() + value.slice(1)}
    </span>
  );
}

function ProfileSkeleton() {
  return (
    <div aria-label="Profile loading" className="profile-skeleton" role="status">
      {Array.from({ length: 7 }, (_, index) => (
        <span key={index} />
      ))}
    </div>
  );
}

function metricLabel(kind: ProfileMetric["kind"]): string {
  return {
    row_count: "Row count",
    null_count: "NULL count",
    null_rate: "NULL rate",
    distinct_count: "Distinct count",
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
    distinct_count: "Different non-NULL values. Distinct does not prove uniqueness.",
    minimum: "Smallest non-NULL numeric or temporal value.",
    maximum: "Largest non-NULL numeric or temporal value.",
    average: "Arithmetic mean of non-NULL numeric values.",
    text_length_minimum: "Fewest characters in non-NULL text.",
    text_length_maximum: "Most characters in non-NULL text.",
    text_length_average: "Average characters in non-NULL text.",
    common_values: "Up to 20 values with the highest exact frequencies.",
    representative_values: "Up to 20 sampled values; they are not a distribution.",
  }[metric.kind];
}

function formatMetricValue(value: unknown, kind?: ProfileMetric["kind"]): string {
  if (value === null || value === undefined) return "NULL";
  if (kind === "null_rate" && typeof value === "number") {
    return value.toLocaleString("en-US", {
      style: "percent",
      maximumFractionDigits: 2,
    });
  }
  if (typeof value === "number")
    return Number.isInteger(value)
      ? value.toLocaleString("en-US")
      : value.toLocaleString("en-US", { maximumFractionDigits: 4 });
  if (typeof value === "object") return JSON.stringify(value, null, 2);
  return String(value);
}

function formatDuration(durationMs: number): string {
  return durationMs < 1_000 ? `${durationMs} ms` : `${(durationMs / 1_000).toFixed(1)} s`;
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
