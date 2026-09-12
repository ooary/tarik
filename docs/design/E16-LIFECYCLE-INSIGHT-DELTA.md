# E16 agent lifecycle recovery insight delta

**Approval:** User approved this lifecycle insight delta on September 12, 2026. This approves durable E16 planning, not production implementation or manual acceptance.

PROBLEM: Prevent reconnects, terminal-state bookkeeping, and abandoned agent connections from producing unrecoverable query/result deadlocks.

X → DesignGraph<A, E, R>
│ │ │ │ │
│ │ │ │ └─ R: paired identity, heartbeat, scheduler, result registry, DuckDB
│ │ │ └──── E: disconnect, stale lease, owner drift, quota, deadline, cancellation cleanup
│ │ └─────── A: connection → query slot → result lease → release
│ │
│ └─ nodes = functions, edges = agent lifecycle and recovery flow
│
└─ the problem: recoverable bounded agent query ownership

SHAPES: ClientProfileId, ConnectionId, ConnectionLease, Heartbeat, QuerySnapshot, SnapshotReservation(available|reserved|consumed), QuerySlot(queued|running|cancellation_requested|released), QueryState, ResultLease, BlockingResource, ActivitySnapshot, DeadlinePolicy(standard|extended), OptionalDuckDbProgress

GRAPH:

```text
register_connection (1)
│  R: authenticated paired profile, connection budget
│  E: revoked|capacity ↯escape(reject; no lease)
│  └─ 🔒 challenge proof → ClientProfileId authority + ConnectionId attribution
│
├─→ refresh_heartbeat (T)
│   R: live bridge transport, monotonic clock
│   E: lease timeout ↯escape(mark stale → reap_connection)
│
├─→ list_active (N)
│   R: profile-scoped connection/query/result registries
│   E: unauthenticated ↯escape(deny) · response budget ↯escape(truncate with cursor/summary)
│   └─ only caller-profile IDs/state; no SQL values, rows, paths, proofs, or foreign-client state
│
└─→ reserve_snapshot (1)
    R: immutable SafeRead snapshot, atomic admission budgets
    E: quota ↯escape(return caller-owned blocking IDs + next tool; restore available snapshot)
    │
    └─→ claim_execution_slot (1)
        R: fair scheduler, current grant/project/catalog/source identity
        E: revoked|stale|queue timeout|cancelled ↯escape(terminal; release reservation)
        │
        └─→ consume_snapshot_after_accept (1)
            R: engine accepted immutable execution
            E: submission failure ↯escape(snapshot available; release slot)
            │
            └─→ execute_with_watchdog (1)
                R: DuckDB worker, interrupt, desktop-owned deadline policy
                E: cancellation|deadline|engine loss ↯escape(terminal cleanup)
                │
                ├─→ observe_truthful_progress (T)
                │   R: query state, queue/run clocks, optional validated DuckDB progress
                │   E: native progress unavailable ↯escape(progressAvailable=false)
                │
                └─→ release_execution_slot (1)
                    R: authoritative terminal-and-cleanup acknowledgement
                    E: cleanup incomplete ↯escape(slotHeld=true + cleanupPending; never claim available)
                    │
                    └─→ retain_result_lease (T)
                        R: ClientProfileId + ProjectId authority, originating ConnectionId attribution, cache quota
                        E: expiry|revoke|explicit release ↯escape(race-safe cleanup)

reap_connection (N)
│  R: heartbeat/transport lease, current profile mapping
│  E: cancellation or file cleanup delayed ⟳retry bounded → ↯escape(cleanup pending remains charged)
└─ cancel connection-owned live work → release connection reservations → keep profile-owned results until result TTL/revoke/release

release_all_from_desktop (1)
│  R: visible Tarik authority, selected profile or connection, current activity snapshot
│  E: concurrent page read|locked file ↯escape(mark cleanup pending)
└─ release transient query result leases only; preserve tables, linked sources, projects, and completed exports
```

CARDINALITY: register_connection (1) · refresh_heartbeat (T) · list_active (N) · reserve_snapshot (1) · claim_execution_slot (1) · consume_snapshot_after_accept (1) · execute_with_watchdog (1) · observe_truthful_progress (T) · release_execution_slot (1) · retain_result_lease (T) · reap_connection (N) · release_all_from_desktop (1)

BOUNDARIES: Authentication proof becomes paired-client authority; connection ID remains origin attribution. SafeRead query/result recovery may cross reconnects only within the same paired ClientProfileId and ProjectId after current Analyze revalidation. Approval, mutation, and guarded-export ownership do not change implicitly. `tarik_list_active` and quota errors expose only bounded caller-profile opaque IDs and state—never SQL text from another client, row values, cache paths, credentials, pairing proofs, or private endpoint identity. Desktop Activity may show bounded submitted SQL under visible local authority; routine snapshots omit it.

BEHAVIOR: ⛈ heartbeat lease wraps authenticated transport without treating ordinary inactivity as death · ⛈ independent queue and execution watchdogs wrap jobs without depending on agent polling · ⛈ blocking errors include caller-owned IDs plus an exact next tool · ⛈ deadlineApproaching reports a clock fact, never a prediction such as willExceedDeadline · ⛈ optional DuckDB progress is labelled unavailable unless safe native integration is proven · ⛈ profile/result and connection/execution quotas remain bounded and atomically reserved.

SCOPE: Connection lease acquire@authenticate → release@transport disconnect/heartbeat expiry · Snapshot reservation acquire@admission → restore@pre-accept failure or consume@engine acceptance · Execution permit acquire@claim → release@authoritative terminal cleanup · Result lease acquire@atomic publication → release@explicit release/desktop release-all/TTL/revoke/startup cleanup · Page-read lease acquire@page read → release@response so cleanup cannot race an active read.

TEST LAYERS: R = {Clock: fake monotonic time, HeartbeatTransport: scripted live/stale bridge, Scheduler: deterministic fair queue, Engine: scripted accept/status/cancel/cleanup plus temporary DuckDB integration, Cache: bounded temporary files, Authority: paired-profile/grant fixtures}. Test reconnect under the same profile, two simultaneous connections of one profile, foreign-profile denial, failed/cancelled terminal slot release, stale process/socket, sleep/resume, snapshot restoration before acceptance, blocking IDs, independent deadline expiry, release-all races, and native Windows process overlap.

VERDICT: The current E15 implementation does not match this delta. Confirmed mismatches are: no agent-visible active-state listing; SafeRead query/result ownership is connection-fragile; failed/cancelled no-result records can satisfy the current `result_id.is_none()` active check and block later starts; blocking errors omit IDs; authenticated connections have no heartbeat TTL; timeout enforcement depends on status polling; and snapshot removal occurs before every post-admission failure point. Existing transport disconnect cleanup is present and must be preserved, so an abandoned result is not assumed without evidence. E16 implementation must separate execution-slot state from result leases, use profile+project authority with connection attribution only for SafeRead recovery, add `tarik_list_active`, make snapshot consumption reserve/commit, and keep extended reads desktop-controlled. Exact percentage/operator/rows-scanned progress remains conditional on a safe measured DuckDB integration and is not approved as a fabricated fallback.

## Approved policy clarifications

- Current `agent.result_limit` checks happen before snapshot removal; that specific error does not currently consume the snapshot. Snapshot restoration is still required for later pre-execution failures after reservation.
- A terminal query and a retained result are distinct: terminal work releases its execution slot; a successful result may retain only a bounded cache lease.
- SafeRead authority becomes `ClientProfileId + ProjectId`; `ConnectionId` remains origin/monitoring attribution. This does not alter approval, mutation, critical confirmation, or delegated-export ownership without another approved delta.
- Candidate heartbeat: every 30 seconds; stale after 2 minutes. E16-T0 must validate Windows sleep/resume and host behavior before pinning these values.
- Standard execution remains 60 seconds. Candidate extended SafeRead is up to 300 seconds, enabled visibly per client/project in Tarik Desktop and unavailable for agent self-selection. E16-T0 must pin authority, persistence, revocation, and resource admission.
- Progress baseline is state, queue wait, running time, deadline/remaining time, deadlineApproaching, slotHeld/slotAvailable, cancellationRequested, resultRetained, and cleanupPending. Native percentage, rows processed, and operator/pipeline are optional only if proven truthful.
- Do not silently LRU-evict an unexpired result to accept new work. Use multiple bounded leases, TTL, explicit release, desktop release-all, and actionable quota errors.
