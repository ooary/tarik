# E15 guarded local MCP agent gateway design graph

## Status and design read

**Status:** Design candidate approved for documentation on September 11, 2026. E15-T1 implementation remains gated on review of this durable contract.

Tarik is the security and ownership boundary. The connected model or MCP host may reason about data and propose work, but it does not own project files, the DuckDB connection, execution policy, approval state, or audit truth.

- `DESIGN_VARIANCE: 3`
- `MOTION_INTENSITY: 2`
- `VISUAL_DENSITY: 8`
- The MCP transport is a host-managed background child process, not a startup daemon or embedded model runtime.
- The Tarik desktop must be running with Agent Access enabled. `tarik-mcp` never opens a competing DuckDB connection.
- MCP-host confirmation is supplemental only. Mutations are approved only through visible, direct interaction with Tarik.
- Security decisions fail closed. A returned row set or a leading `SELECT` is not evidence that a statement is safe.
- Every collection, execution, response, registry, cache, artifact, deadline, and audit history is bounded.
- E13-T6 and E12-T4 continue to own Windows portable, clean-machine, DPI, mixed-monitor, and process-lifecycle acceptance.

## PROBLEM

Allow local MCP hosts to analyze explicitly granted Tarik projects and propose controlled changes without gaining direct access to DuckDB, files, approval authority, or unbounded resources.

```text
X → DesignGraph<A, E, R>
│              │   │  │  │
│              │   │  │  └─ R: MCP SDK, local IPC, Tarik services, effect policy, SQLite, DuckDB sidecar
│              │   │  └──── E: protocol, authentication, authorization, classification, execution, rollback, audit errors
│              │   └─────── A: authenticated bounded tool calls, immutable snapshots, approvals, results, terminal outcomes
│              │
│              └─ nodes = functions, edges = data flow
│
└─ the problem: guarded local agent access to Tarik
```

## SHAPES

### Identity

- `ClientProfileId`: random opaque persistent identity assigned during local pairing.
- `ConnectionId`: random identity for one `tarik-mcp` process connection; never supplied by the host.
- `PairingRequestId`: random, expiring identity for one visible pairing decision.
- `ProjectId`: existing SQLite-owned opaque project identity.
- `AgentCatalogRevision`: content identity over relation kinds, columns/types, trusted view/source provenance, and source health. It is stronger than a display timestamp and changes when a safe-read dependency changes.
- `ExecutionId`, `ResultId`, `ProfileId`, `ApprovalId`, `AuditEventId`: random server identities whose ownership is resolved inside Tarik.
- `SnapshotHash`: SHA-256 over a versioned canonical envelope, not SQL text alone.

A self-asserted MCP `clientInfo.name`, executable path, parent process name, and command-line profile label are display evidence only. They are never authorization identity. Authorization uses the paired `ClientProfileId`, proof of possession of its secret, the fresh `ConnectionId`, and server-owned grants.

### Capabilities

```text
Inspect
Analyze
ModifyWorkspace
ModifyData
```

Capabilities are project-scoped. A new client begins with no project grants. `Inspect + Analyze` is the recommended grant. `ModifyWorkspace` does not imply `ModifyData`, and neither modify capability is inferred from an open project.

### Effect decisions

```text
SafeRead
  one known-safe analytical statement over an explicitly granted active project

ApprovalRequired
  a controlled mutation or typed workspace/filesystem action that Tarik can execute safely

CriticalConfirmation
  a destructive, whole-relation, replacement, overwrite, removal, or history-clear action

Blocked
  an unsupported, ambiguous, security-changing, external, process, transaction-control,
  multi-statement, project-file, arbitrary-file, secret, extension, or unknown operation
```

There is no fallback from `Blocked` to an approval dialog. Approval cannot make an operation executable when Tarik cannot classify or control its effects.

### Immutable SQL and action snapshots

```text
SqlSnapshot {
  snapshotVersion,
  exactSql,
  snapshotHash,
  clientProfileId,
  connectionId,
  projectId,
  agentCatalogRevision,
  effectDecision,
  statementKind,
  referencedObjects,
  invokedFunctions,
  externalEffects,
  filterEvidence,
  createdAt
}

TypedActionSnapshot {
  snapshotVersion,
  actionKind,
  canonicalArguments,
  snapshotHash,
  clientProfileId,
  connectionId,
  projectId,
  agentCatalogRevision,
  effectDecision,
  affectedObjects,
  filesystemEffects,
  createdAt
}
```

Canonical envelopes use fixed field ordering and explicit version tags. The raw MCP request is never later reinterpreted as the approved action. SQL is stored exactly as received after rejecting NUL and enforcing the byte limit; it is not silently normalized or rewritten.

### Approval record and states

```text
ApprovalSnapshot {
  approvalId,
  actionSnapshot,
  requestingClient,
  connectionId,
  projectId,
  agentCatalogRevision,
  requiredCapability,
  effectDecision,
  affectedObjects,
  filterEvidence,
  impactEvidence,
  filesystemEffects,
  typedPhraseHash?,
  createdAt,
  expiresAt
}

Proposed
  → Pending
      ├─→ Denied
      ├─→ Expired
      ├─→ Invalidated
      └─→ Approved
            └─→ Claimed
                  └─→ Running
                        ├─→ Succeeded
                        ├─→ RolledBack
                        └─→ CriticalRecovery
```

Only a direct event from the visible Tarik approval UI may perform `Pending → Approved` or `Pending → Denied`. The MCP surface contains proposal, status, and execute-by-ID operations, but no approve or confirm operation.

### Execution and result states

```text
AgentExecutionState = queued | running | succeeded | failed | cancelled | expired | lost
AgentResult = columns + bounded page artifacts + producedRows + rowTotal? + rowTotalExact
ApprovalOutcome = denied | expired | invalidated | succeeded | rolled_back | critical_recovery
```

When an MCP row cap stops result publication, `rowTotalExact` is false unless DuckDB independently supplied an exact total. Tarik never reports a capped count as the complete query count.

### Structured error families

- Protocol: `ProtocolUnsupported`, `FrameTooLarge`, `MalformedRequest`, `MethodUnavailable`.
- Availability: `DesktopUnavailable`, `AgentAccessDisabled`, `ProjectNotActive`, `EngineUnavailable`, `ShuttingDown`.
- Identity: `PairingRequired`, `PairingDenied`, `AuthenticationFailed`, `ClientRevoked`, `ConnectionStale`.
- Authorization: `GrantMissing`, `ProjectMismatch`, `CapabilityDenied`, `ResultOwnershipMismatch`.
- Policy: `SqlTooLarge`, `MultipleStatements`, `SqlUnsupported`, `EffectBlocked`, `ParserDisagreement`, `CatalogStale`.
- Capacity: `RateLimited`, `Busy`, `RegistryFull`, `ResponseTooLarge`.
- Execution: `ExecutionExpired`, `ExecutionMissing`, `ExecutionLost`, `ResultMissing`, `Cancelled`.
- Approval: `ApprovalMissing`, `ApprovalExpired`, `ApprovalInvalidated`, `ApprovalAlreadyUsed`, `TypedConfirmationRequired`.
- Recovery: `RollbackFailed`, `CatalogRefreshFailed`, `AuditUnavailable`, `CriticalRecoveryRequired`.

Errors crossing from the engine, SQLite, operating system, or transport are translated at their owning layer. MCP clients receive stable codes, safe messages, optional retry guidance, and no raw SQL, values, secrets, paths, or internal exception chains.

## GRAPH

### Process, protocol, bridge, and pairing

```text
MCP host
  → M1 spawn_stdio_server
      [A: running tarik-mcp child · (1)]
      [R: executable, stdin, stdout, stderr]
      [E: spawn/configuration failure ↯escape(actionable host error)]

  → M2 negotiate_protocol
      [A: NegotiatedSession · (1)]
      [R: MCP 2025-06-18 and 2025-11-25 schemas]
      [E: unsupported version ↯escape(ProtocolUnsupported)]
      [E: malformed/oversized frame ↯escape(JSON-RPC error or close)]
      [🔒 stdin JSON → typed MCP request]

  → M3 connect_local_bridge
      [A: LocalBridgeConnection · (T)]
      [R: Unix-domain socket or Windows named pipe, runtime endpoint descriptor]
      [E: Tarik closed ⟳retry×bounded → ↯escape(DesktopUnavailable)]
      [E: unsafe endpoint ownership/permissions ↯escape(AuthenticationFailed)]

  → M4 authenticate_or_pair
      [A: AuthenticatedConnection · (T)]
      [R: challenge nonce, client secret verifier, pairing registry, visible Tarik pairing UI]
      [E: unpaired ↯escape(PairingRequired until local decision)]
      [E: denied/revoked/replayed proof ↯escape(AuthenticationFailed)]
      [🔒 claimed host metadata → untrusted display label]
      [🔒 HMAC challenge response → authenticated ClientProfileId]

  → M5 route_tool
      [A: TypedToolRequest · (N)]
      [R: static tool registry, negotiated protocol, connected Tarik version]
      [E: unknown/unavailable tool ↯escape(MethodUnavailable)]
      [🔒 tool arguments → schema-validated request]

  → M6 authorize_tool
      [A: AuthorizedToolRequest · (N)]
      [R: live connection, project grants, capability matrix]
      [E: wrong project/capability/revocation ↯escape(authorization error)]
```

Pairing does not trust a host name. A new `--profile <opaque-id>` connection generates a secret locally, sends only the verifier proof through the user-owned local channel, and becomes usable only after the user accepts the visible pairing request. The child stores its secret in a user-private profile file; Tarik stores a salted verifier in SQLite. Existing profile IDs without valid proof are rejected rather than silently re-paired. Re-pairing is a separate visible action that rotates the credential and invalidates old connections.

The threat model assumes the current operating-system user account is trusted. Any process that can read that user's protected files or inject into Tarik is outside the v1 isolation claim. This limitation is explicit; process names and same-user filesystem permissions are not presented as a sandbox.

### Inspect path

```text
M6 authorize_tool
  → M7 list_granted_projects
      [A: bounded redacted project summaries · (N)]
      [R: ProjectsRepository, grant filter, cursor codec]
      [E: stale/forged cursor ↯escape(CursorInvalid)]

  → M8 inspect_active_catalog
      [A: bounded relations, columns, source state, AgentCatalogRevision · (N)]
      [R: ProjectManager, active session, catalog/source provenance]
      [E: closed/non-active project ↯escape(ProjectNotActive)]
      [E: catalog drift ↯escape(CatalogStale)]
```

Listing does not open or activate a project, touch recent-project ordering, start DuckDB, scan source data, or reveal project/source paths. In v1, analytical tools operate only on the one project already active in visible Tarik. A granted but closed project may be listed, but not queried or opened by MCP.

### SQL classification and bounded safe-read path

```text
M6 authorize_tool
  → M9 capture_sql_snapshot
      [A: immutable SqlSnapshot · (1)]
      [R: byte limit, canonical hashing, authenticated identity]
      [E: empty/oversized/NUL SQL ↯escape(SqlUnsupported)]
      [🔒 MCP SQL text → immutable bounded snapshot]

  → M10 classify_effect
      [A: EffectDecision · (1)]
      [R: sqlparser DuckDbDialect AST, pinned DuckDB parser and statement type,
          recursive visitor, function policy, catalog/source provenance,
          non-executing bind/plan evidence]
      [E: multiple statements ↯escape(Blocked)]
      [E: unsupported AST/function/macro/view ↯escape(Blocked)]
      [E: parser/type/plan disagreement ↯escape(Blocked)]
      [E: stale catalog ↯escape(CatalogStale)]
      [E: classifier invariant violation ☠die]

      ├─ SafeRead
      │   → M11 submit_bounded_read
      │       [A: AgentExecution queued · (1)]
      │       [R: MCP execution coordinator, EngineManager, active project,
      │           row/page/byte/deadline budgets]
      │       [E: capacity exhausted ↯escape(Busy + retryAfter)]
      │       [E: engine unavailable ↯escape(EngineUnavailable)]
      │
      │   → M12 observe_execution
      │       [A: queued|running|terminal status · (N)]
      │       [R: client-owned execution registry, monotonic clock]
      │       [E: deadline ↯escape(cancel then Expired)]
      │       [E: sidecar loss ↯escape(ExecutionLost)]
      │
      │   → M13 read_result_page
      │       [A: bounded JSON-safe page · (N)]
      │       [R: engine page artifacts, safe decoder, ownership registry,
      │           page and response byte budgets]
      │       [E: wrong owner ↯escape(ResultOwnershipMismatch)]
      │       [E: released/expired result ↯escape(ResultMissing)]
      │       [🔒 DuckDB values → bounded JSON values, NULL/truncation markers]
      │
      │   → M14 release_execution
      │       [A: released result and ownership · (1)]
      │       [R: result store, execution registry]
      │       [E: already released ↯escape(idempotent released state)]
      │
      └─ Blocked
          → M22 reject_effect
              [A: deterministic reason with no approval route · (1)]
              [R: effect policy]
              [E: none]
```

Safe-read classification is necessary but not sufficient. Immediately before submission Tarik atomically verifies the same connection, grant, project, snapshot hash, and `AgentCatalogRevision`. The sidecar receives an immutable execution ticket containing the approved snapshot hash and MCP result limits. It does not accept a later SQL replacement under that ticket.

MCP result limiting is an execution option, not an invisible `LIMIT` rewrite. The sidecar stops publishing after the allowed rows/pages/bytes, marks the result incomplete, and releases excess material. The exact SQL remains visible and auditable.

### Approval and mutation path

```text
M10 classify_effect
  → ApprovalRequired | CriticalConfirmation
      → M15 register_approval
          [A: Pending ApprovalSnapshot · (1)]
          [R: approval registry, secure IDs, SQLite audit intent, 120-second clock]
          [E: registry full ↯escape(RegistryFull)]
          [E: audit unavailable ↯escape(AuditUnavailable)]

      → M16 present_in_tarik
          [A: visible approval request · (T)]
          [R: Tarik window, approval center, exact server-held snapshot]
          [E: app hidden/minimized ↯escape(bring visible; never auto-approve)]
          [E: close/dismiss/timeout ↯escape(Denied or Expired)]
          [🔒 direct local UI event → ApprovalDecision]

      → M17 decide_locally
          [A: Approved|Denied|Expired · (1)]
          [R: direct Tauri UI command, typed phrase verifier]
          [E: wrong critical phrase ↯escape(remain Pending)]
          [E: catalog/client/grant drift ↯escape(Invalidated)]

      → M18 claim_approval
          [A: atomically consumed immutable action · (1)]
          [R: approval registry transaction, same authenticated connection,
              project, capability, snapshot hash, and catalog revision]
          [E: replay/race/stale/used approval ↯escape(ApprovalInvalidated)]

      → M19 execute_approved_mutation
          [A: committed mutation or confirmed rollback · (1)]
          [R: exclusive analytical gate, dedicated sidecar mutation lane,
              DuckDB transaction, immutable engine ticket]
          [E: execution/cancellation ↯escape(ROLLBACK)]
          [E: rollback failure ↯escape(CriticalRecovery)]
          [E: process loss or ambiguous commit ↯escape(CriticalRecovery; never replay)]

      → M20 refresh_identity
          [A: fresh catalog/source/cache state · (1)]
          [R: ProjectManager, source repository, invalidation registry]
          [E: refresh failure ↯escape(CriticalRecovery)]

      → M21 finalize_audit
          [A: durable terminal audit fact · (1)]
          [R: SQLite immediate transaction, bounded retention]
          [E: terminal audit failure ↯escape(CriticalRecovery; freeze agent mutations)]
```

`tarik_execute_approved` accepts only `approvalId`. The exact action comes from Tarik's server-held snapshot. Approval is atomically consumed before execution and cannot be transferred, retried, or reused. An ambiguous outcome after a sidecar crash is not automatically replayed. Tarik enters project-scoped critical recovery, blocks further agent mutation, preserves evidence, refreshes/inspects state where possible, and directs the user to recover visibly.

Cancellation of a mutation is reported as `cancelled/rolled_back` only after rollback is confirmed. If rollback or terminal audit cannot be confirmed, the outcome is `critical_recovery`, not success and not an ordinary cancellation.

### Data-trust and typed workspace paths

```text
M6 authorize_tool
  → M23 run_bounded_profile
      [A: queued/profile status/snapshot · (N)]
      [R: existing ProfileCoordinator, catalog identity, client ownership]
      [E: busy/stale/cancelled ↯escape(structured status)]

  → M24 inspect_quality
      [A: bounded definitions/runs/current-data preview · (N)]
      [R: QualityCoordinator, immutable revisions, grant filter]
      [E: stale revision/preview limit ↯escape(structured error)]

  → M25 propose_typed_action
      [A: immutable typed source/export/saved-query/check/editor action · (1)]
      [R: action-specific validator, capability and effect policy]
      [E: arbitrary path/action ↯escape(Blocked)]
      [then M15 → M16 → M17 → M18 → owning typed Tarik service]
```

Profile keeps Exact/Approximate/Sampled provenance and current profile limits. Quality failure previews always say **Current-data preview using revision N** and never imply historical-row retention. Creating/updating/deleting checks or saved queries, running custom quality SQL, opening an editor tab, exporting, importing, linking, repairing, and clearing history use typed approved actions. Opening or saving SQL never executes it.

### Disconnect and shutdown

```text
stdin EOF | disconnect | revoke | project close | Agent Access off | Tarik shutdown
  → M26 cleanup_connection
      [A: all client-owned resources released · (1)]
      [R: connection ownership registry, coordinators, approval registry]
      [E: cleanup failure ↯escape(bounded warning + shutdown continues)]
```

Cleanup first invalidates authorization and pending approvals, then prevents new claims, cancels queued/running analytical work, waits within the existing shutdown budget, and releases results/previews/profiles. A mutation already in `Running` follows commit/rollback recovery rules; disconnect never converts it into an untracked operation.

## CARDINALITY

`M1 spawn_stdio_server (1)` · `M2 negotiate_protocol (1)` · `M3 connect_local_bridge (T)` · `M4 authenticate_or_pair (T)` · `M5 route_tool (N)` · `M6 authorize_tool (N)` · `M7 list_granted_projects (N)` · `M8 inspect_active_catalog (N)` · `M9 capture_sql_snapshot (1)` · `M10 classify_effect (1)` · `M11 submit_bounded_read (1)` · `M12 observe_execution (N)` · `M13 read_result_page (N)` · `M14 release_execution (1)` · `M15 register_approval (1)` · `M16 present_in_tarik (T)` · `M17 decide_locally (1)` · `M18 claim_approval (1)` · `M19 execute_approved_mutation (1)` · `M20 refresh_identity (1)` · `M21 finalize_audit (1)` · `M22 reject_effect (1)` · `M23 run_bounded_profile (N)` · `M24 inspect_quality (N)` · `M25 propose_typed_action (1)` · `M26 cleanup_connection (1)`.

## BOUNDARIES

1. `🔒 MCP host stdin JSON → typed MCP request`.
2. `🔒 host-provided name/version/profile label → non-authoritative display metadata`.
3. `🔒 runtime endpoint descriptor → verified user-owned local endpoint`.
4. `🔒 nonce plus HMAC proof → authenticated ClientProfileId and fresh ConnectionId`.
5. `🔒 tool JSON arguments → one typed schema with unknown fields rejected where security-relevant`.
6. `🔒 SQL text → byte-bounded immutable SqlSnapshot`.
7. `🔒 sqlparser AST + pinned DuckDB parse/type + catalog policy → agreed EffectDecision`.
8. `🔒 current catalog/source records → AgentCatalogRevision and redacted MCP metadata`.
9. `🔒 DuckDB values → bounded JSON-safe cells with NULL and truncation markers`.
10. `🔒 direct local Tarik button/typed phrase → ApprovalDecision`.
11. `🔒 filesystem action → Tarik-selected or previously registered typed identity`; no MCP arbitrary path crosses this boundary.
12. `🔒 local table values containing instructions → inert result data`; values cannot become tool calls, grants, typed phrases, or approval decisions.
13. `🔒 engine/SQLite/OS failures → stable redacted MCP errors`.

The v1 security claim protects against malicious prompts, hostile or unpaired MCP clients, confused-deputy requests, prompt injection in data, cross-project access, stale/replayed approvals, and arbitrary SQL/file/network effects. It does not claim isolation from malware already running as the same operating-system user or from a compromised Tarik process.

## Protocol contract

### SDK and versions

The implementation candidate is the official Rust MCP SDK crate `rmcp` 3.3.x with only server, macros/schema, and stdio/async-I/O features. Its Rust 1.88 minimum is compatible with Tarik's pinned Rust 1.91. HTTP, OAuth, SSE, child-process client, and network transport features remain disabled. The exact version is locked in `Cargo.lock` when T2 begins and added to third-party notices.

Tarik supports MCP `2025-06-18` and `2025-11-25`, negotiating the highest common version. It does not advertise the SDK's later experimental/future protocol constants without a separate design review. Unsupported versions return the standard unsupported-version error and execute no tool.

### Stdio discipline

- Standard MCP JSON-RPC messages use the SDK's stdio framing.
- One encoded input message is limited to 1 MiB before unbounded allocation.
- `stdout` is protocol-only from process start through exit. No panic, tracing subscriber, dependency, or startup banner may write to stdout.
- Bounded redacted diagnostics use `stderr`; at most 30 recent diagnostic lines are retained for an actionable startup failure.
- Malformed messages produce a standard JSON-RPC error when an ID can be recovered safely; otherwise the connection closes.
- Duplicate initialization, calls before initialization, and calls after shutdown are rejected.
- EOF cancels the bridge connection and triggers M26. The child exits without becoming a daemon.

### Server capabilities

V1 advertises only tools and logging/progress behavior required by implemented calls. It does not advertise prompts, roots, sampling, elicitation, resources, completion, HTTP sessions, or model APIs. Tool-list changes caused by pairing, grant, project, or Tarik-version changes use the negotiated tool-list-changed notification when supported; every call still reauthorizes server-side.

MCP request cancellation maps only to the execution owned by that request or returned agent execution. Cancellation cannot target another connection and cannot stand in for rollback confirmation. Progress tokens, when supplied, receive bounded phase updates no faster than once per second.

### Pagination and cursors

Catalog, project, quality, audit, and other collections return:

```text
items
nextCursor?     opaque authenticated cursor
hasMore
limitApplied
responseBytes
```

The cursor contains or authenticates the client, connection, project, collection kind, sort key, grant revision, catalog revision where applicable, expiry, and MAC. It reveals no path or SQL. A changed grant/catalog or expired/foreign cursor is rejected, not silently restarted. Results use `resultId + offset`; the offset is aligned to the existing 500-row page boundary and remains connection-owned.

### Core typed tool inventory

Tools are statically implemented and filtered by Tarik version and live grants. The gateway never mirrors Tauri commands or engine methods dynamically.

| Tool                      | Principal input                     | Principal output                              | Capability/effect                        |
| ------------------------- | ----------------------------------- | --------------------------------------------- | ---------------------------------------- |
| `tarik_server_info`       | none                                | version, availability, pairing/grant guidance | paired status only / SafeRead            |
| `tarik_list_projects`     | cursor, limit                       | granted redacted project summaries            | Inspect / SafeRead                       |
| `tarik_list_catalog`      | projectId, filters, cursor          | bounded object summaries + revision           | Inspect / SafeRead                       |
| `tarik_describe_relation` | projectId, opaque relation identity | bounded columns/type/source state             | Inspect / SafeRead                       |
| `tarik_propose_sql`       | projectId, exact SQL                | snapshot/effect/reason or pending approval    | Analyze or modify / classified           |
| `tarik_start_query`       | safe snapshotId                     | queued AgentExecution                         | Analyze / SafeRead only                  |
| `tarik_query_status`      | executionId                         | bounded lifecycle/status                      | owner / SafeRead                         |
| `tarik_cancel_query`      | executionId                         | current cancellation state                    | owner / SafeRead                         |
| `tarik_get_result_page`   | resultId, offset                    | maximum 500-row bounded JSON page             | owner / SafeRead                         |
| `tarik_release_result`    | resultId                            | idempotent release state                      | owner / SafeRead                         |
| `tarik_explain_query`     | safe snapshotId, estimate or actual | bounded structured flow                       | Analyze / SafeRead; Actual explicit      |
| `tarik_execute_approved`  | approvalId                          | queued/terminal approved-action identity      | matching modify grant / already approved |
| Profile/quality tools     | typed project/relation/run IDs      | bounded provenance/history/preview            | Inspect or Analyze                       |
| Typed workspace tools     | canonical typed action              | pending approval identity                     | ModifyWorkspace/Data / approval          |

`tarik_propose_sql` never executes. For `SafeRead` it creates a short-lived immutable snapshot ID. For mutations it creates an approval request only when the operation is controllable and the capability exists. For `Blocked` it returns a deterministic policy reason and no executable ID.

## Capability and effect matrix

| Operation                                                       | Required capability   | Decision                           |
| --------------------------------------------------------------- | --------------------- | ---------------------------------- |
| Server and pairing status                                       | paired connection     | `SafeRead`                         |
| List explicitly granted projects                                | `Inspect`             | `SafeRead`                         |
| Inspect active granted catalog                                  | `Inspect`             | `SafeRead`                         |
| One known-safe analytical query                                 | `Analyze`             | `SafeRead`                         |
| Estimate Flow                                                   | `Analyze`             | `SafeRead`                         |
| Actual Flow                                                     | `Analyze`             | `SafeRead`, explicit execution     |
| Profile a registered relation                                   | `Analyze`             | `SafeRead`                         |
| Inspect quality definitions/history                             | `Inspect`             | `SafeRead`                         |
| Current-data failure preview                                    | `Analyze`             | `SafeRead`, explicitly labeled     |
| `INSERT` or `MERGE`                                             | `ModifyData`          | `ApprovalRequired`                 |
| Filtered top-level `UPDATE`/`DELETE`                            | `ModifyData`          | `ApprovalRequired`                 |
| Unfiltered or unprovably filtered `UPDATE`/`DELETE`             | `ModifyData`          | `CriticalConfirmation`             |
| Create/alter object                                             | `ModifyData`          | `ApprovalRequired`                 |
| Drop/truncate                                                   | `ModifyData`          | `CriticalConfirmation`             |
| Create-or-replace/overwrite                                     | `ModifyData`          | `CriticalConfirmation`             |
| Save/update query or check                                      | `ModifyWorkspace`     | `ApprovalRequired`                 |
| Delete saved query/check                                        | `ModifyWorkspace`     | `ApprovalRequired`                 |
| Clear query/check/MCP audit history                             | `ModifyWorkspace`     | `CriticalConfirmation`             |
| Typed import/link/export                                        | matching modify grant | `ApprovalRequired`                 |
| Replace export/repair source                                    | matching modify grant | `CriticalConfirmation`             |
| Remove/forget source                                            | `ModifyData`          | `CriticalConfirmation`             |
| Open SQL in editor                                              | `ModifyWorkspace`     | `ApprovalRequired`; never executes |
| Raw path or URL in SQL                                          | none                  | `Blocked`                          |
| Raw `COPY`, `ATTACH`, `DETACH`, `EXPORT`, `IMPORT`              | none                  | `Blocked`                          |
| `INSTALL`, `FORCE INSTALL`, `LOAD`, extension operations        | none                  | `Blocked`                          |
| Create/drop/use secret operations                               | none                  | `Blocked`                          |
| Network, remote scans, environment/credential access            | none                  | `Blocked`                          |
| Shell/process execution                                         | none                  | `Blocked`                          |
| Unsafe `SET`, `RESET`, `PRAGMA`, `CALL`, `CHECKPOINT`, `VACUUM` | none                  | `Blocked`                          |
| Explicit `BEGIN`, `COMMIT`, `ROLLBACK`, savepoints              | none                  | `Blocked`                          |
| Prepare/execute/deallocate and dynamic SQL                      | none                  | `Blocked`                          |
| Multiple statements                                             | none                  | `Blocked`                          |
| User macro, unknown view dependency, unknown AST/function       | none                  | `Blocked`                          |
| Managed project delete or arbitrary file delete                 | none                  | `Blocked`                          |

Typed Tarik operations are separate from similarly named raw SQL. A user may approve a typed export to a location chosen through Tarik, while raw `COPY ... TO '/path'` remains blocked with no approval route.

## Parser and classifier decision

### Spike result

A temporary, non-repository spike tested `sqlparser` 0.62.0 with `DuckDbDialect`. It produced distinct AST variants for queries, external table functions, `COPY`, DuckDB `ATTACH`, `INSTALL`, `LOAD`, `CREATE SECRET`, `PRAGMA`, `SET`, `CALL`, filtered/unfiltered updates, delete-returning, create-or-replace, drop, truncate, transaction start, macros, and multiple statements. It also parsed DuckDB `QUALIFY`, wildcard `EXCLUDE`/`REPLACE`, and dollar-quoted strings.

This supports using it as the recursive policy AST, but not as the sole authority. Its accepted grammar may differ from pinned DuckDB, future AST variants are possible, and AST parsing alone does not prove relation/function provenance.

### Required dual-parser algorithm

A snapshot is `SafeRead` only when every stage agrees:

1. Reject empty, NUL-containing, or over-256-KiB SQL.
2. Tokenize/parse once with `sqlparser::dialect::DuckDbDialect`; require exactly one `Statement::Query`.
3. Recursively visit every statement, query, table factor, relation, function, expression, nested CTE, subquery, and clause. Unknown variants fail closed.
4. Reject table functions and dynamic relation arguments by default. No path or URL supplied by SQL is allowed.
5. Resolve every base relation against the current granted catalog. V1 allows base tables and Tarik-registered linked views. Other views require recursively classified stored SQL plus a provenance digest before they can be admitted; until that exists they are blocked.
6. Resolve every invoked function against a pinned safe-function policy. Only explicitly classified pure scalar, aggregate, and window functions are allowed. User macros, table functions, environment/settings functions, side-effecting functions, and unknown overloads are blocked.
7. Use DuckDB's C API `duckdb_extract_statements` to require exactly one pinned-DuckDB statement, then prepare only that extracted statement and read `duckdb_prepared_statement_type`. Never call the high-level multi-statement prepare path before the one-statement check because that path may execute intermediate statements.
8. Require DuckDB statement type `SELECT`. Reject all other or invalid statement types.
9. Only after AST policy rejects external effects, perform non-executing DuckDB bind/plan validation under disabled extension autoload/autoinstall and disabled external access where compatible with already registered linked-source execution.
10. Require the plan/relation evidence to agree with the AST and catalog policy. Any parse, bind, type, provenance, or policy disagreement is `Blocked`.
11. Recompute and compare `AgentCatalogRevision` immediately before execution.

Lexical checks remain defense in depth for forbidden tokens, semicolon/comment edge cases, and diagnostics, but never upgrade a decision to `SafeRead`. Existing `validate_quality_read_only` is not reused as the MCP security authority.

### Catalog and function provenance

Current `CatalogSnapshot.revision` covers object and column identity but does not prove view definition or registered-source provenance. E15 adds `AgentCatalogRevision` rather than weakening the existing contract. It includes hashes of admitted view definitions/dependencies and source identities/states without exposing their paths to MCP.

DuckDB extension autoload/autoinstall and external access settings must be explicitly configured and read back for the agent lane. If disabling external access would break an existing registered linked Parquet view, the sidecar permits only that server-resolved registered source identity; it never accepts the path from MCP SQL. If the pinned DuckDB API cannot enforce this distinction, linked views remain unavailable to `SafeRead` rather than broadening filesystem access.

## Threat model

| Threat                                       | Boundary/control                                         | Required test                                            |
| -------------------------------------------- | -------------------------------------------------------- | -------------------------------------------------------- |
| Malicious model emits destructive SQL        | effect classifier + approval class                       | destructive corpus never reaches SafeRead                |
| Prompt injection stored in a table value     | values remain inert result data                          | injected “approve/run” text causes no state transition   |
| Host claims to be Claude/Pi/admin            | claimed metadata is display-only                         | spoofed names do not change ClientProfileId/grants       |
| Unpaired or revoked process calls tools      | challenge proof + live revocation                        | pair/deny/revoke/reconnect matrix                        |
| Confused deputy requests another project     | project-scoped grant checked every call                  | cross-project IDs/cursors/results rejected               |
| Client replaces SQL after review             | server-held immutable snapshot                           | changed SQL/hash cannot execute by approval ID           |
| Approval replay or double race               | atomic one-use claim                                     | concurrent claims produce exactly one winner             |
| Approval transferred to another connection   | client/connection binding                                | same client on another connection cannot claim it        |
| Catalog changes after classification         | AgentCatalogRevision binding                             | DDL/source change invalidates snapshot/approval          |
| SQL parser disagreement                      | dual parser + fail closed                                | one-parser-only syntax is blocked                        |
| Side effects hidden in CTE/subquery/function | recursive AST/function provenance                        | nested adversarial fixtures blocked                      |
| User view or macro hides external access     | provenance digest or block                               | unknown view/macro cannot enter SafeRead                 |
| Raw path traversal or URL                    | no raw path SQL; typed operations only                   | relative/absolute/UNC/symlink/URL corpus blocked         |
| Extension/secret/settings operation          | no approval route                                        | every family deterministically Blocked                   |
| Denial of service                            | frame, rate, concurrency, deadline, row/page/byte bounds | capacity and cleanup tests                               |
| Sidecar crash during read                    | lost terminal state + artifact cleanup                   | restart does not expose/reuse stale result               |
| Sidecar crash during mutation                | no replay; critical recovery                             | ambiguous commit blocks mutations and preserves evidence |
| Tarik hidden/locked                          | bring visible; no host confirmation substitution         | pending request cannot self-approve                      |
| Desktop shutdown/disconnect                  | invalidate first, then cancel/release                    | no pending approval, job, result, listener residue       |
| stdout contamination                         | protocol-only writer                                     | subprocess golden test rejects any non-JSON stdout       |
| Local secret disclosure in logs              | redaction and zeroization                                | log/stderr corpus contains no secret/SQL/value/path      |

## Bounded defaults

| Resource                              |                 Default hard bound |
| ------------------------------------- | ---------------------------------: |
| Encoded MCP input message             |                              1 MiB |
| SQL snapshot                          |                            256 KiB |
| Paired client profiles                |                                  8 |
| Simultaneous bridge connections       |                                  4 |
| Granted projects per client           |                                 16 |
| Pending pairing requests              |                                  4 |
| Pairing request lifetime              |                          5 minutes |
| Pending approvals globally            |                                 16 |
| Pending approvals per client          |                                  4 |
| Approval lifetime                     |                        120 seconds |
| In-memory terminal MCP tool records   |                                256 |
| Durable MCP audit records per project |                              5,000 |
| Authenticated tool-call rate          |         60/minute/client, burst 10 |
| Active MCP query executions           |                 1/client, 4 global |
| Active MCP result sets                |                 1/client, 4 global |
| Rows published per MCP result         |                              5,000 |
| Rows per result page                  |                        maximum 500 |
| Pages published per MCP result        |                         maximum 10 |
| Encoded MCP result-page response      |                              1 MiB |
| Catalog/metadata page                 |             100 entries or 256 KiB |
| Ordinary synchronous tool deadline    |                         10 seconds |
| Read/flow execution deadline          |                         60 seconds |
| Profile deadline                      |                        120 seconds |
| Profile request/snapshot              | existing 100-column/256-KiB bounds |
| Progress updates                      |         maximum 1/second/execution |
| Retained stderr tail                  |                           30 lines |

A stricter lower bound wins when an existing subsystem is smaller. Capacity exhaustion returns a stable `Busy` or `RateLimited` response and starts no hidden work. There is no unbounded pending queue.

## BEHAVIOR

- `⛈ rate-limit` wraps every authenticated tool call.
- `⛈ timeout` wraps bridge calls and analytical execution, never auto-replaying mutations.
- `⛈ response-budget` wraps catalog, result, profile, quality, and audit responses.
- `⛈ redacted diagnostics` writes bounded facts to stderr and existing Tarik logs.
- `⛈ structured audit` wraps pairing, grants, authorization denial, approval, and terminal execution.
- `⛈ cancellation` maps MCP cancellation only to connection-owned work.
- `⛈ progress` emits no more than one phase update per second.
- `⛈ retention` prunes terminal registries and durable audit history.
- `⛈ accessibility` wraps pairing and approval UI with direct focus, keyboard, screen-reader, text-plus-color, minimum-viewport, and reduced-motion-safe behavior.
- `⛈ visibility` brings Tarik's approval center to the foreground; it never converts host interaction into approval.

Behavior layers are removable without changing the happy-path graph. Policy classification, authorization, approval claiming, transaction ownership, and cleanup are core nodes, not optional middleware.

## SCOPE

- `tarik-mcp child acquire@MCP-host-spawn → release@stdin-EOF/process-exit`.
- `stdio streams acquire@M1 → release@M26`.
- `desktop local listener acquire@Tarik-startup-with-Agent-Access → release@Agent-Access-off/Tarik-shutdown`.
- `runtime endpoint descriptor + instance nonce acquire@listener-start → delete@listener-stop`.
- `authenticated bridge connection acquire@M4 → release@M26`.
- `client secret bytes acquire@profile-load/pairing → zeroize@rotation/revocation/process-exit`.
- `pairing request acquire@M4 → release@approve/deny/expire/disconnect`.
- `safe SQL snapshot acquire@M9 → release@start/expire/disconnect/catalog-change`.
- `approval snapshot acquire@M15 → release@deny/expire/invalidate/atomic-claim`.
- `engine execution acquire@M11-or-M19 → cancel/release@terminal/timeout/disconnect/revoke/shutdown`.
- `result artifacts/pages acquire@safe-read-success → release@M14/M26/expiry/project-close/shutdown`.
- `DuckDB mutation transaction acquire@M19 → commit-or-confirmed-rollback@M19`.
- `catalog/source snapshot acquire@classification → invalidate@catalog-change/project-close/engine-restart`.
- `project mutation gate acquire@M18 → release@terminal-audit-or-critical-recovery`.

On Linux the bridge is a Unix-domain socket in a verified user runtime directory with directory mode `0700` and endpoint/credential files mode `0600`. Symlinks, unexpected ownership, group/world permissions, and unsafe fallback directories are rejected. On Windows it is a named pipe with an ACL restricted to the current user SID; the descriptor and profile credential use a current-user ACL. No TCP or LAN listener exists.

## Existing implementation inventory and reuse boundary

| Existing owner                 | Reusable contract                                                | E15 mismatch/action                                                                   |
| ------------------------------ | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `ProjectManager`               | one active project and one desktop-owned engine session          | expose redacted active/granted identity only; never paths/open/remove commands        |
| `EngineManager`                | lazy sidecar, session recovery, serialized protocol requests     | bridge through it; add agent tickets and no competing DuckDB owner                    |
| `QueryCoordinator`             | immutable SQL, async polling, cancellation, exactly-once history | add MCP owner/deadline/terminal bounds or a shared bounded coordinator facade         |
| engine `JobRegistry`           | FIFO jobs, interruption, Arrow page publication                  | clone the session connection when a worker claims work; add MCP cap and mutation lane |
| `ResultStore`                  | 500-row pages, 12-page decoded LRU, explicit release             | add client/project ownership and 1-MiB MCP encoding budget                            |
| plan coordinator               | non-executing Estimate and explicit Actual Flow                  | require SafeRead snapshot and MCP ownership/budgets                                   |
| `ProfileCoordinator`           | bounded typed profiles and cancellation                          | bind connection/project ownership and MCP deadline                                    |
| `QualityCoordinator`           | immutable revisions, aggregate history, bounded previews         | grant-filter metadata; preserve current-data preview wording and release              |
| SQLite metadata                | projects, sources, queries, checks, settings                     | add client/grant/approval/audit migrations and bounded repositories                   |
| shutdown coordinator           | cancel, wait, release, close, checkpoint                         | invalidate bridge and approvals first; include MCP-owned counts/resources             |
| observability                  | bounded redacted log fields                                      | add MCP event kinds without SQL, values, paths, or credentials                        |
| current lexical SQL validation | warnings and custom-quality defense                              | never use as MCP authority; retain only as secondary check                            |

Commands that must never be mirrored directly include project create/open/rename/remove/close, arbitrary source inspection/path operations, raw import/link/repair/drop, catalog drop, resource settings, query execution, result release-all, exports/reveal, saved-query/check/history mutations, support/log/cache commands, and shutdown commands. MCP receives a separate allowlisted service API whose arguments are narrower and whose ownership/policy checks cannot be skipped.

The current engine clones a session connection when work is enqueued. A query queued behind approved DDL could therefore retain a pre-DDL catalog view. E15 requires cloning when the worker claims the job, or equivalent serialization that proves catalog visibility, before approved schema mutation is enabled.

## Implementation boundaries and dependency order

1. **E15-T1:** local listener, credential lifecycle, pairing, project grants, revocation, and shutdown ownership. No SQL tools.
2. **E15-T2:** `tarik-mcp` Rust binary using the reviewed stdio SDK and a versioned private bridge protocol. Server-info/pairing guidance only until authenticated.
3. **E15-T3:** redacted granted-project and active-catalog inspection.
4. **E15-T5:** dual-parser classifier, source/function provenance, immutable snapshots, and adversarial corpus.
5. **E15-T4:** SafeRead execution, paging, cancellation, flow, and cleanup after the classifier contract exists.
6. **E15-T6:** visible approval center and atomic one-use state machine.
7. **E15-T7:** exclusive transactional mutation lane, catalog visibility correction, audit, rollback, and critical recovery.
8. **E15-T8:** bounded profile, quality, saved-query, editor, and other typed actions.
9. **E15-T9:** packaging, host compatibility, adversarial review, resource evidence, and explicit manual sign-off.

T4 is listed before T5 in task numbering but cannot implement executable SQL until T5's classifier contract is present. Discovery and pairing may be developed without exposing execution.

## TEST LAYERS

Production requirements are replaced without changing the graph:

```text
MCP transport       → in-memory duplex JSON-RPC stream
Clock               → frozen/advancing monotonic test clock
Entropy             → deterministic secure-ID and nonce provider
Local bridge        → scripted authenticated bridge
Pairing UI          → recorded direct local decisions
Grant repository    → in-memory SQLite
SQL parser          → real sqlparser DuckDbDialect
DuckDB classifier   → pinned temporary DuckDB engine/C API
Catalog/source      → deterministic project and linked-source fixtures
Query engine        → scripted queued/running/terminal executor
Result store        → bounded temporary page artifacts
Approval registry   → in-memory plus SQLite race fixtures
Mutation engine     → transactional DuckDB fixture with failure injection
Audit repository    → in-memory SQLite with write-failure injection
Filesystem          → temporary user-owned directories and permission fixtures
Logger              → redaction-asserting sink
```

Required test layers and corpora:

- MCP initialization/version negotiation, duplicate init, schema goldens, malformed/oversized messages, cancellation, progress, EOF, and stdout/stderr separation.
- Local endpoint ownership/permissions/ACLs, profile secret entropy and rotation, pairing approve/deny/expire, spoofed identity, proof replay, revocation, and concurrent clients.
- Project/capability isolation, cursor MAC/expiry/revision, catalog byte/entry caps, source path redaction, and no query on catalog inspection.
- Every supported DuckDB statement family, comments, quotes, dollar strings, Unicode identifiers, nested CTEs/subqueries, table/scalar functions, macros, views, registered versus arbitrary sources, URLs, extension/secret/settings/process operations, and parser disagreement.
- Safe-read queue/running/success/failure/cancel/loss/deadline, cap truthfulness, first/later pages, 64-KiB cell truncation, NULL fidelity, one-owner access, release, revoke, disconnect, restart, and residue.
- Approval approve/deny/dismiss/expire, typed critical phrase, no-paste path, snapshot/client/connection/project/revision mismatch, replay/race, registry full, no MCP approve tool, and no hidden execution.
- Mutation insert/update/delete/merge/create/alter/drop/truncate, filtered/unfiltered risk, exactly-once claim, transaction commit/error/cancel/rollback, process-loss ambiguity, rollback/audit failure, catalog refresh, and critical-recovery freeze.
- Profile provenance/bounds/cancel, quality immutable revision/history/current-data labels, custom confirmation, saved-query/check typed actions, open-without-run, and preview release.
- Fixed-workload memory/disk/process tests on Linux and native Windows, including no orphan `tarik-mcp` or engine process.
- Real-Tauri accessibility and visual review of pairing/approval in light, dark, system, minimum viewport, keyboard-only, screen-reader, reduced motion, and 100/125/150/200% DPI states.

## VERDICT

The design graph is complete: every node has A, E, R and cardinality; each untrusted edge is parsed once; every domain failure has a retry, escape, or defect strategy; resource acquisition has structural release; and tests can substitute every requirement without changing the graph. The happy path is separate from layer-specific error joins.

The current code does not yet match the graph:

- No MCP executable or authenticated local bridge exists.
- Current Tauri and engine methods are broader than the MCP contract and must not be mirrored.
- Query/result/profile/quality registries do not track MCP connection ownership.
- Query execution lacks MCP-specific row, page, byte, deadline, rate, and terminal-record bounds.
- Current lexical validation is not a security classifier.
- Current catalog revision does not include view/source provenance needed by SafeRead.
- No approval registry, immutable engine ticket, exclusive mutation lane, bounded MCP audit, or critical-recovery freeze exists.
- Engine work clones its session connection too early for DDL visibility guarantees.
- Shutdown does not yet invalidate pairings/approvals or release client-scoped resources.

Therefore E15 implementation is not yet valid against this graph. T1 must begin at the bridge/pairing boundary, no executable SQL tool may ship before T5, and T7 may not enable mutations until atomic approval, transaction, rollback, catalog refresh, and terminal audit are all proven.

The intended process topology is:

```text
MCP host
  └── tarik-mcp background stdio child
        └── authenticated OS-local bridge
              └── visible running Tarik Desktop
                    └── existing desktop-owned DuckDB sidecar
```

`tarik-mcp` exits with its host. It is not an always-running service, does not listen on TCP, and never owns the project database directly.
