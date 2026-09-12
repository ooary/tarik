# E16 bounded multi-query MCP sessions design graph

**Approval:** User approved this implementation graph on September 12, 2026, including the Activity Queries/Agents navigation, desktop-owned analysis limits, extended-analysis authority, and large-result handoff. Manual/native acceptance remains separate.

PROBLEM: Support multiple bounded MCP queries and retained results with truthful recovery, monitoring, cancellation, expiry, and agent-session visibility.

X → DesignGraph<A, E, R>
│ │ │ │ │
│ │ │ │ └─ R: authenticated bridge, scheduler, DuckDB, bounded cache, monotonic clock, desktop authority
│ │ │ └──── E: quota, stale authority, timeout, disconnect, cancellation race, locked files, process ambiguity
│ │ └─────── A: snapshot → queue → execute → retain → discover/release
│ │
│ └─ nodes = functions, edges = query, result, connection, and monitoring flow
│
└─ the problem: bounded recoverable agent analysis

SHAPES: ClientProfileId, ConnectionId, ConnectionLease(active|stale|disconnected), ObservedProcessIdentity(verified|unavailable), SafeReadSnapshot(available|reserved|consumed), ExecutionLease(queued|running|cancellation_requested|terminal|cleanup_pending), ResultLease(retained|reading|expired|released|cleanup_pending), ActivitySnapshot, QueryActivityDetail, AgentSessionSummary, DeadlinePolicy(standard|extended), AnalysisLimits, QuotaReservation, BlockingResource

GRAPH:

```text
authenticate_connection (1)
│  R: pairing verifier, current grants, connection quota
│  E: revoked|bad proof|capacity ↯escape(reject without lease)
│  └─ 🔒 proof → ClientProfileId authority + ConnectionId attribution
│
├─→ heartbeat_connection (T)
│   R: bridge transport, monotonic clock
│   E: transport loss ↯escape(disconnect)
│      stale heartbeat ↯escape(reap after resume-safe grace)
│
├─→ list_active_for_profile (N)
│   R: authenticated profile, bounded activity registry
│   E: response limit ↯escape(bounded summaries)
│   └─ no foreign profile, SQL text, rows, paths, proofs, or credentials
│
└─→ reserve_snapshot_and_quota (1)
    R: available immutable SafeRead snapshot, current Analyze grant,
       profile/global count and byte budgets
    E: quota ↯escape(snapshot remains available;
       return caller-owned blocking IDs + exact release action)
    │
    └─→ enqueue_execution (1)
        R: fair scheduler, queue deadline
        E: queue full|queue timeout|cancel ↯escape(terminal;
           release slot and reservation)
        │
        └─→ claim_execution (1)
            R: active project, current profile grant, current catalog/source
               policy, claim-time DuckDB connection
            E: project/grant/catalog/source drift ↯escape(stale terminal;
               restore snapshot when engine never accepted it)
            │
            └─→ engine_accepts_snapshot (1)
                R: engine scheduler registration
                E: submission failure ↯escape(snapshot available;
                   release reservations)
                │
                └─→ consume_snapshot (1)
                    R: confirmed engine acceptance
                    E: invariant mismatch ☠die
                    │
                    └─→ execute_with_watchdog (1)
                        R: one shared heavy permit, InterruptHandle,
                           standard/extended desktop policy
                        E: deadline|cancel|engine loss ↯escape
                           (terminal cleanup, never false success)
                        │
                        └─→ release_execution_permit (1)
                            R: authoritative worker completion
                            E: cleanup incomplete ↯escape
                               (slotHeld=true, cleanupPending=true)
                            │
                            └─→ publish_result_lease (T)
                                R: atomic cache publication, exact byte count
                                E: effective writer cap|publication failure
                                   ↯escape(no arbitrary partial result)
```

```text
read_result_page (N)
│  R: same ClientProfileId + ProjectId, current Analyze grant,
│     unexpired result, page-read lease
│  E: expired|released|foreign ↯escape(actionable error)
└─ acquire read lease → bounded page read → release read lease

expire_or_release_result (N)
│  R: idle/absolute deadline, explicit release, revoke, project close,
│     desktop Release all
│  E: active reader|Windows lock ⟳retry bounded
│     → ↯escape(cleanup pending remains charged)
└─ remove authority → delete owned page directory → release byte charge
```

```text
snapshot_activity (N)
│  R: QueryCoordinator + AgentAccessManager + engine status/resources
│  E: engine unavailable ↯escape(stale/unavailable snapshot)
└─ bounded summaries only
   │
   ├─→ get_query_detail (1)
   │   R: visible desktop authority
   │   E: missing|released ↯escape(refresh)
   │   └─ bounded selectable immutable SQL; no bind values
   │
   ├─→ cancel_selected_query (1)
   │   R: owning coordinator/InterruptHandle
   │   E: completion race ↯escape(return authoritative terminal state)
   │
   └─→ release_selected_results (1)
       R: profile or connection desktop selection
       E: read/file-lock race ↯escape(cleanup pending)

open_activity_workspace (1)
│  R: existing application shell, trigger focus
│  E: project unavailable ↯escape(empty/unavailable state)
├─ Queries view (T): coalesced 500 ms summary polling while visible
├─ Agents view (T): active sessions separate from paired/disconnected clients
└─ return_to_previous_workspace (1)
   └─ restore prior Query/Profile/Quality workspace and focus without remount
```

```text
observe_agent_transport (1)
│  R: bridge-owned transport metadata
│  E: PID cannot be independently verified ↯escape(label unavailable)
└─ register_session → monitor snapshot → disconnect/stale cleanup
```

CARDINALITY: authenticate_connection (1) · heartbeat_connection (T) · list_active_for_profile (N) · reserve_snapshot_and_quota (1) · enqueue_execution (1) · claim_execution (1) · engine_accepts_snapshot (1) · consume_snapshot (1) · execute_with_watchdog (1) · release_execution_permit (1) · publish_result_lease (T) · read_result_page (N) · expire_or_release_result (N) · snapshot_activity (N) · get_query_detail (1) · cancel_selected_query (1) · release_selected_results (1) · open_activity_workspace (1) · Queries view (T) · Agents view (T) · return_to_previous_workspace (1) · observe_agent_transport (1)

BOUNDARIES: MCP JSON becomes a typed bridge request. Pairing proof becomes authenticated ClientProfileId authority. SafeRead query/result recovery uses ClientProfileId + ProjectId; ConnectionId remains origin attribution. Approval, mutation, and export ownership stay connection-bound. MCP `tarik_list_active` exposes only the caller profile's bounded opaque IDs and summaries. Desktop Activity retrieves bounded SQL only on demand; routine polling omits full SQL and result rows. A process ID is displayed only when independently observed by Tarik's local transport; self-reported identity is never termination authority.

BEHAVIOR: ⛈ independent queue/execution watchdogs do not require agent polling · ⛈ heartbeat every 30 seconds with stale threshold 120 seconds and resume-safe grace · ⛈ one shared heavy operation initially runs per active project · ⛈ fair profile scheduling prevents starvation · ⛈ no automatic eviction of unexpired results · ⛈ no fabricated percentage, ETA, CPU, RAM, operator, rows-scanned, or throughput · ⛈ Activity polls at approximately 500 ms only while visible with no overlap.

SCOPE: ConnectionLease acquire@authenticate → release@transport-loss/heartbeat-expiry/revoke/disable/shutdown · SnapshotReservation acquire@query-admission → restore@pre-engine-failure or consume@engine-acceptance · ExecutionPermit acquire@scheduler-claim → release@authoritative-worker-terminal · ResultLease acquire@atomic-publication → release@explicit-release/TTL/revoke/project-close/startup-cleanup · PageReadLease acquire@result-page → release@response-finally · ActivityTimer acquire@workspace-visible → release@leave/unmount · MCP process acquire@host → release@host-stdio-EOF.

TEST LAYERS: R = {Clock: fake monotonic clock and simulated suspend gap, Authority: paired profiles/reconnects/grants/revocation, Scheduler: deterministic owner queues and cancellation races, Engine: scripted acceptance/status/interrupt plus real DuckDB integration, Cache: temporary result directories/byte limits/read leases/locked files, Transport: EOF/half-open heartbeat/reconnect/multiple connections, Desktop: mocked Tauri commands with real workspace reducers, ProcessObserver: verified/unavailable identity fixtures}. Same graph, no production-only branch.

VERDICT: The pre-E16 implementation does not match this graph: it permits one result per connection; query/result ownership is connection-fragile; terminal failed/cancelled records can block starts; snapshots are removed before all pre-accept failures; timeout depends on status polling; connections have no heartbeat; DuckDB connections are cloned at enqueue; analytical admission is split; result bytes/read leases/TTL/cleanup backlog are not tracked; and no active-list, Activity workspace, or Agent Monitor exists. Tarik does not spawn `tarik-mcp`, so the reported duplicate process cause remains unconfirmed pending native host evidence.

## Pinned initial policy

- Defaults for a 16 GiB/i5-class target: 4 outstanding SafeReads/profile and 16 globally; 8 retained results/profile and 32 globally; 5,000 browse rows; 500 rows/page; 1 MiB MCP page response; 32 MiB/result; 128 MiB/profile; 512 MiB global cache/staging; one heavy execution; 60-second queue and standard execution deadlines; optional desktop-authorized 300-second execution; 10-minute idle and 30-minute absolute result expiry.
- Hard customization candidates: browse rows 100–50,000; result bytes 8–128 MiB; retained results 1–16; outstanding queries 1–8; profile cache 32–512 MiB; global cache 128 MiB–1 GiB. Tarik Desktop alone controls settings, validates combinations, and snapshots effective limits at admission. Agents can observe but cannot raise them.
- Large source scans are valid when filtering/aggregation returns bounded data. Initial raw exploration should select relevant columns and use `LIMIT 100`.
- Row-cap completion is successful but explicitly incomplete: `browseLimitReached=true`, `rowTotalExact=false`, `completeResultAvailable=false`, `limitReason=browse_row_cap`. Byte-cap failure publishes no arbitrary partial result.
- Handoff order is refine/aggregate, non-executing **Open in editor** for user Run, then guarded complete-query export (prefer Parquet for large typed output).
- Activity is one header button beside Agent access. Its right workspace has **Queries** and **Agents** views and a **Return to workspace** action. Existing Query/Profile/Quality state remains mounted.
- No machine-wide MCP singleton and no killing by executable name. One host-owned stdio child per configured host transport is expected; legitimate multiple hosts/instances remain supported.
