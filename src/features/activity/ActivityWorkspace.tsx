import { CopyIcon, PauseIcon, PlayIcon, TrashIcon, UsersThreeIcon } from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  cancelAgentActivityQuery,
  cancelDesktopActivityQuery,
  getActivitySnapshot,
  getAgentQueryDetail,
  getDesktopQueryDetail,
  releaseAgentActivityResult,
  releaseAgentResults,
  type ActivityAgentQuery,
  type ActivityDesktopQuery,
  type ActivitySnapshot,
} from "../../lib/commands";
import { AnalysisLimits } from "./AnalysisLimits";
import "./activity.css";

type ActivityView = "queries" | "agents";
type SelectedQuery =
  { kind: "desktop"; query: ActivityDesktopQuery } | { kind: "agent"; query: ActivityAgentQuery };

interface ActivityWorkspaceProps {
  projectId: string;
  onClose: () => void;
  onOpenSql: (sql: string, title: string) => void;
}

function shortId(value: string) {
  return value.length > 12 ? `${value.slice(0, 8)}…${value.slice(-4)}` : value;
}

function duration(value: number) {
  if (value < 1_000) return `${value} ms`;
  return `${(value / 1_000).toFixed(value < 10_000 ? 1 : 0)} s`;
}

function bytes(value: number | null) {
  if (value === null) return "Unavailable";
  if (value < 1_024 * 1_024) return `${Math.ceil(value / 1_024)} KiB`;
  return `${(value / 1_024 / 1_024).toFixed(1)} MiB`;
}

function queryKey(query: SelectedQuery) {
  return `${query.kind}:${query.query.executionId}`;
}

export function ActivityWorkspace({ projectId, onClose, onOpenSql }: ActivityWorkspaceProps) {
  const [view, setView] = useState<ActivityView>("queries");
  const [snapshot, setSnapshot] = useState<ActivitySnapshot | null>(null);
  const [selected, setSelected] = useState<SelectedQuery | null>(null);
  const [sql, setSql] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [stale, setStale] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false);

  const refresh = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    try {
      const next = await getActivitySnapshot(projectId);
      setSnapshot(next);
      setStale(false);
      setError(null);
      setSelected((current) => {
        if (!current) return null;
        const replacement =
          current.kind === "desktop"
            ? next.desktopQueries.find((query) => query.executionId === current.query.executionId)
            : next.agentQueries.find((query) => query.executionId === current.query.executionId);
        return replacement ? ({ kind: current.kind, query: replacement } as SelectedQuery) : null;
      });
    } catch (reason) {
      setStale(true);
      setError(`Activity is unavailable: ${String(reason)}`);
    } finally {
      setLoading(false);
      inFlight.current = false;
    }
  }, [projectId]);

  useEffect(() => {
    let active = true;
    const initial = window.setTimeout(() => {
      if (active) void refresh();
    }, 0);
    const timer = window.setInterval(() => {
      if (active) void refresh();
    }, 500);
    return () => {
      active = false;
      window.clearTimeout(initial);
      window.clearInterval(timer);
    };
  }, [refresh]);

  useEffect(() => {
    if (!selected) return;
    let active = true;
    const detail = selected.kind === "desktop" ? getDesktopQueryDetail : getAgentQueryDetail;
    const load = window.setTimeout(() => {
      detail(selected.query.executionId)
        .then((detail) => {
          if (active) setSql(detail.sql);
        })
        .catch((reason) => {
          if (active) setError(`SQL detail is unavailable: ${String(reason)}`);
        });
    }, 0);
    return () => {
      active = false;
      window.clearTimeout(load);
    };
  }, [selected]);

  const rows = useMemo<SelectedQuery[]>(() => {
    if (!snapshot) return [];
    return [
      ...snapshot.desktopQueries.map((query) => ({ kind: "desktop" as const, query })),
      ...snapshot.agentQueries.map((query) => ({ kind: "agent" as const, query })),
    ];
  }, [snapshot]);

  const selectedSql = sql;
  const selectedAgent = selected?.kind === "agent" ? selected.query : null;
  const canCancel = selected
    ? selected.query.state === "queued" || selected.query.state === "running"
    : false;

  async function act(operation: () => Promise<unknown>) {
    setError(null);
    try {
      await operation();
      await refresh();
    } catch (reason) {
      setError(String(reason));
    }
  }

  return (
    <section aria-label="Activity" className="activity-workspace">
      <header className="activity-header">
        <div>
          <h1>Activity</h1>
          <span className={stale ? "activity-stale" : ""} role="status">
            {stale ? "Last known state" : "Live local state"}
          </span>
        </div>
        <button className="text-button" onClick={onClose} type="button">
          Return to workspace
        </button>
      </header>

      <div aria-label="Activity views" className="activity-tabs" role="tablist">
        <button
          aria-selected={view === "queries"}
          onClick={() => setView("queries")}
          role="tab"
          type="button"
        >
          <PlayIcon aria-hidden="true" size={14} /> Queries
        </button>
        <button
          aria-selected={view === "agents"}
          onClick={() => setView("agents")}
          role="tab"
          type="button"
        >
          <UsersThreeIcon aria-hidden="true" size={15} /> Agents
        </button>
      </div>

      {error && (
        <p className="activity-error" role="alert">
          {error}
        </p>
      )}

      {view === "queries" ? (
        <div className="activity-query-layout">
          <div aria-label="Query activity" className="activity-list" role="listbox">
            {loading && !snapshot ? (
              <div className="activity-empty">Loading authoritative query state…</div>
            ) : rows.length === 0 ? (
              <div className="activity-empty">
                <strong>No query activity</strong>
                <span>Desktop and agent queries for this project will appear here.</span>
              </div>
            ) : (
              rows.map((item) => {
                const agent = item.kind === "agent" ? item.query : null;
                const state = agent?.cancellationRequested
                  ? "cancellation requested"
                  : item.query.state;
                return (
                  <button
                    aria-selected={selected ? queryKey(selected) === queryKey(item) : false}
                    className="activity-query-row"
                    key={queryKey(item)}
                    onClick={() => {
                      setSql(null);
                      setSelected(item);
                    }}
                    role="option"
                    type="button"
                  >
                    <span className={`activity-state activity-state-${item.query.state}`} />
                    <span className="activity-query-main">
                      <strong>
                        {item.kind === "agent" ? item.query.clientName : "Tarik Desktop"}
                      </strong>
                      <code>{shortId(item.query.executionId)}</code>
                    </span>
                    <span className="activity-query-state">{state.replace(/_/g, " ")}</span>
                    <span className="activity-query-time">
                      {duration(
                        item.kind === "agent"
                          ? item.query.runningMs || item.query.queueWaitMs
                          : item.query.durationMs,
                      )}
                    </span>
                  </button>
                );
              })
            )}
          </div>

          <aside aria-label="Selected query details" className="activity-detail">
            {!selected ? (
              <div className="activity-empty">
                <strong>Select a query</strong>
                <span>Details load on demand; result rows are never polled.</span>
              </div>
            ) : (
              <>
                <div className="activity-detail-heading">
                  <div>
                    <strong>
                      {selected.kind === "agent" ? selected.query.clientName : "Tarik Desktop"}
                    </strong>
                    <code>{selected.query.executionId}</code>
                  </div>
                  <span>{selected.query.state}</span>
                </div>
                <dl className="activity-metrics">
                  {selected.kind === "agent" ? (
                    <>
                      <div>
                        <dt>Queue wait</dt>
                        <dd>{duration(selected.query.queueWaitMs)}</dd>
                      </div>
                      <div>
                        <dt>Running</dt>
                        <dd>{duration(selected.query.runningMs)}</dd>
                      </div>
                      <div>
                        <dt>Deadline</dt>
                        <dd>{selected.query.limits.executionDeadlineSeconds} s</dd>
                      </div>
                      <div>
                        <dt>Browse cap</dt>
                        <dd>{selected.query.limits.browseRowCap.toLocaleString("en-US")} rows</dd>
                      </div>
                      <div>
                        <dt>Result cache</dt>
                        <dd>{bytes(selected.query.cacheBytes)}</dd>
                      </div>
                      <div>
                        <dt>Slot</dt>
                        <dd>{selected.query.slotHeld ? "Held" : "Available"}</dd>
                      </div>
                    </>
                  ) : (
                    <>
                      <div>
                        <dt>Elapsed</dt>
                        <dd>{duration(selected.query.durationMs)}</dd>
                      </div>
                      <div>
                        <dt>Rows</dt>
                        <dd>
                          {selected.query.rowsProduced?.toLocaleString("en-US") ??
                            selected.query.rowsAffected?.toLocaleString("en-US") ??
                            "Unavailable"}
                        </dd>
                      </div>
                      <div>
                        <dt>Memory</dt>
                        <dd>
                          {snapshot?.resources
                            ? `${snapshot.resources.memoryLimitMib.toLocaleString("en-US")} MiB shared`
                            : "Unavailable"}
                        </dd>
                      </div>
                      <div>
                        <dt>Threads</dt>
                        <dd>
                          {snapshot?.resources
                            ? `${snapshot.resources.threads} shared`
                            : "Unavailable"}
                        </dd>
                      </div>
                    </>
                  )}
                  <div>
                    <dt>Progress</dt>
                    <dd>Unavailable</dd>
                  </div>
                </dl>
                {selectedAgent?.browseLimitReached && (
                  <p className="activity-limit-note" role="note">
                    Limited browse result. The displayed row count is not an exact total. Refine or
                    aggregate, open the SQL as a user-run draft, or use guarded complete export.
                  </p>
                )}
                <div className="activity-sql-heading">
                  <strong>Submitted SQL</strong>
                  <span>{selectedSql ? "Immutable submitted SQL" : "Loading on demand"}</span>
                </div>
                <pre className="activity-sql" tabIndex={0}>
                  {selectedSql ?? "Loading SQL…"}
                </pre>
                <div className="activity-actions">
                  {selectedSql && (
                    <button
                      className="text-button"
                      onClick={() => void navigator.clipboard?.writeText(selectedSql)}
                      type="button"
                    >
                      <CopyIcon aria-hidden="true" size={14} /> Copy SQL
                    </button>
                  )}
                  {selectedSql && (
                    <button
                      className="text-button"
                      onClick={() =>
                        onOpenSql(
                          selectedSql,
                          selected.kind === "agent" ? "Agent query" : "Activity query",
                        )
                      }
                      type="button"
                    >
                      Open in editor
                    </button>
                  )}
                  {canCancel && (
                    <button
                      className="text-button activity-danger"
                      onClick={() =>
                        void act(() =>
                          selected.kind === "agent"
                            ? cancelAgentActivityQuery(selected.query.executionId)
                            : cancelDesktopActivityQuery(selected.query.executionId),
                        )
                      }
                      type="button"
                    >
                      <PauseIcon aria-hidden="true" size={14} />{" "}
                      {selected.query.state === "queued" ? "Cancel queued query" : "Cancel query"}
                    </button>
                  )}
                  {selectedAgent?.resultId && (
                    <button
                      className="text-button"
                      onClick={() =>
                        void act(() => releaseAgentActivityResult(selectedAgent.resultId!))
                      }
                      type="button"
                    >
                      <TrashIcon aria-hidden="true" size={14} /> Release result
                    </button>
                  )}
                </div>
              </>
            )}
          </aside>
        </div>
      ) : (
        <div className="activity-agents">
          <AnalysisLimits />
          {!snapshot || snapshot.agentConnections.length === 0 ? (
            <div className="activity-empty">
              <strong>No authenticated agent sessions</strong>
              <span>
                Paired clients without a live authenticated connection are not shown as active.
              </span>
            </div>
          ) : (
            snapshot.agentConnections.map((connection) => (
              <article className="activity-agent-row" key={connection.connectionId}>
                <div>
                  <strong>{connection.clientName}</strong>
                  <code title={connection.connectionId}>{shortId(connection.connectionId)}</code>
                </div>
                <dl>
                  <div>
                    <dt>Status</dt>
                    <dd>{connection.heartbeatStale ? "Heartbeat stale" : "Connected"}</dd>
                  </div>
                  <div>
                    <dt>Connected</dt>
                    <dd>{duration(connection.connectedForMs)}</dd>
                  </div>
                  <div>
                    <dt>Heartbeat</dt>
                    <dd>{duration(connection.lastHeartbeatMsAgo)} ago</dd>
                  </div>
                  <div>
                    <dt>Queries</dt>
                    <dd>
                      {connection.queuedQueries} queued · {connection.runningQueries} running
                    </dd>
                  </div>
                  <div>
                    <dt>Results</dt>
                    <dd>
                      {connection.retainedResults} · {bytes(connection.retainedCacheBytes)}
                    </dd>
                  </div>
                  <div>
                    <dt>Adapter PID</dt>
                    <dd>{connection.adapterPid ?? "Unavailable"}</dd>
                  </div>
                </dl>
                {connection.retainedResults > 0 && (
                  <button
                    className="text-button"
                    onClick={() =>
                      void act(() => releaseAgentResults({ connectionId: connection.connectionId }))
                    }
                    type="button"
                  >
                    Release connection results
                  </button>
                )}
              </article>
            ))
          )}
        </div>
      )}
    </section>
  );
}
