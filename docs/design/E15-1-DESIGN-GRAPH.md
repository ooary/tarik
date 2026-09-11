# E15.1 assisted MCP setup and guarded export delegation

## Status and design read

**Status:** Approved by the user on September 11, 2026. E15.1-T1 may proceed against this contract. Native Windows confirmation of host locations, executable forms, CLI behavior, ACL/reparse behavior, and atomic replacement remains mandatory before T0 or T1 can be marked complete.

Tarik remains the security and ownership boundary. Setup may register Tarik with an MCP host, but it never pairs a client, grants a project, approves an action, starts a third-party host, or supplies model credentials. Export delegation permits only create-new files inside a private Tarik-owned destination grant. Agents never receive or supply destination paths.

- `DESIGN_VARIANCE: 3`
- `MOTION_INTENSITY: 2`
- `VISUAL_DENSITY: 8`
- Preserve Tarik's calm, dense IDE shell, semantic tokens, Radix dialog behavior, Phosphor icons, sharp surfaces, and non-color-only states.
- The connection assistant is platform-aware. Unsupported platforms, host versions, executable forms, and configuration formats fall back to reviewed guidance without mutation.
- Windows one-click support is an implementation candidate until exercised natively on Windows x64 MSVC with real hosts.

## PROBLEM

Connect supported Windows MCP hosts safely, provide portable usage guidance, and delegate bounded full-query exports without giving agents paths or approval authority.

```text
X → DesignGraph<A, E, R>
│              │   │  │  │
│              │   │  │  └─ R: OS identity/filesystem/process APIs, host adapters,
│              │   │        Tarik pairing, SQLite, classifier, ExportCoordinator
│              │   │  └──── E: spoofed host, unsafe config, process failure,
│              │   │        policy violation, collision, cancellation, recovery
│              │   └─────── A: setup plans/receipts, guidance, destination grants,
│              │            immutable export intents, bounded terminal manifests
│              │
│              └─ nodes = functions, edges = data flow
│
└─ assisted MCP setup and guarded export delegation
```

## SHAPES

### Host setup

```text
HostKind = ClaudeDesktop | ClaudeCode | Codex | Pi | Cursor | VSCode | Generic
SetupMethod = OfficialCli | ManagedJson | Guided

HostState
  = NotInstalled
  | Unsupported
  | NotConfigured
  | Configured
  | RestartRequired
  | PairingPending
  | Paired
  | Granted
  | RepairRequired
  | Conflict

HostInstallation {
  kind,
  canonicalExecutable?,
  version?,
  setupMethod,
  evidence,
  state
}

HostSetupPlan {
  planId,
  kind,
  expectedVersion,
  operation,
  executable,
  structuredArguments,
  configChangeSummary?,
  managedEntryHash?,
  expiresAt
}

SetupReceipt {
  receiptId,
  hostKind,
  operation,
  managedEntryHash,
  executableIdentity,
  backupIdentity?,
  completedAt
}
```

A host name, PATH order, parent process, file name, or CLI label is display evidence only. A one-click CLI adapter requires a canonical directly executable regular file. A `.cmd`, `.bat`, PowerShell script, shell command, or unreviewed executable falls back to `Guided`.

### Guidance

```text
UsageGuidance = McpInstructions | McpPrompt | AgentSkill
```

Guidance teaches discovery, immutable snapshot use, resource release, approval waiting, Profile/Quality provenance, and complete export behavior. It cannot pair, grant, approve, select a destination, bypass classification, or weaken a backend check.

### Export delegation

```text
ExportDestinationGrant {
  destinationId,
  privateCanonicalPath,
  displayLabel,
  clientProfileId,
  projectId,
  allowedFormats,
  maximumRowsPerPart,
  maximumTotalBytes,
  createNewOnly=true,
  enabled,
  revision
}

ExportIntent {
  safeReadSnapshotId,
  destinationId?,
  format,
  baseName,
  rowsPerPart,
  csvOptions?,
  parquetOptions?
}

ExportDecision = Delegated | ApprovalRequired | CriticalConfirmation | Blocked

AgentExportState
  = Proposed | Queued | Running | Succeeded
  | Failed | Cancelled | RecoveryRequired | Released
```

### Structured errors

- Setup: `HostMissing`, `HostUnsupported`, `UnsafeExecutable`, `ProcessTimeout`, `ProcessFailed`, `VerificationFailed`.
- Configuration: `ConfigMissing`, `ConfigTooLarge`, `ConfigMalformed`, `ConfigConflict`, `UnsafeConfigPath`, `BackupFailed`, `AtomicReplaceFailed`, `UndoConflict`.
- Guidance: `GuidanceUnsupported`, `GuidanceConflict`, `UnsafeSkillPath`.
- Destination: `DestinationMissing`, `DestinationUnsafe`, `DestinationMoved`, `DestinationForeign`, `DestinationDisabled`.
- Export: `SnapshotMissing`, `SnapshotStale`, `InvalidExportIntent`, `QuotaExceeded`, `OutputCollision`, `ExportBusy`, `ExportLost`, `ExportRecoveryRequired`.

Operating-system, process, config-parser, SQLite, and engine failures are translated at their owning layer. UI and MCP errors are bounded and path-free unless a direct Tarik setup review must show the local executable/configuration target to the user.

## GRAPH

### Host detection and setup

```text
C1 locate_packaged_mcp (1)
│ A: canonical tarik-mcp.exe identity
│ R: current executable, package layout, filesystem metadata
│ E: missing/non-regular/reparse path ↯escape(PackageIncomplete)
│ 🔒 current executable path → trusted packaged sibling
└→ C2 detect_hosts (1)
   │ A: bounded HostInstallation[]
   │ R: Windows environment, closed location manifest, PATH enumerator
   │ E: individual probe failure ↯escape(host unavailable; continue)
   │ 🔒 environment/PATH entries → candidate paths
   └→ C3 inspect_host_candidate (N)
      │ A: supported canonical executable and version
      │ R: ownership/reparse checks, PE check, bounded direct process runner
      │ E: shell shim/non-PE/wrong owner ↯escape(Guided)
      │ E: timeout/output overflow ↯escape(Unsupported)
      │ 🔒 executable/version output → HostInstallation
      └→ C4 inspect_existing_configuration (N)
         │ A: HostState
         │ R: official host get/list adapter or managed JSON reader
         │ E: malformed/conflicting state ↯escape(Conflict; no write)
         │ 🔒 CLI output/config JSON → typed current configuration
         └→ C5 build_setup_plan (1)
            │ A: immutable expiring HostSetupPlan
            │ R: host adapter, canonical tarik-mcp.exe, current configuration
            │ E: unsupported version/method ↯escape(Guided)
            └→ C6 present_setup_plan (1)
               │ A: direct local approve/cancel decision
               │ R: visible Tarik UI
               │ E: cancel/expiry ↯escape(no change)
               │ 🔒 direct Tarik event → LocalSetupDecision
               ├→ OfficialCli
               │  └→ C7 invoke_official_cli (1)
               │     │ A: bounded process outcome
               │     │ R: direct process runner, structured argument vector
               │     │ E: timeout/failure ↯escape(no retry, show recovery)
               │     └→ C8 verify_official_cli (1)
               │        │ A: verified exact Tarik entry
               │        │ R: host get/list, strict bounded parser
               │        │ E: mismatch ↯escape(VerificationFailed)
               ├→ ManagedJson
               │  └→ C9 prepare_managed_json (1)
               │     │ A: parsed original + exact merged document
               │     │ R: ownership/type/reparse checks, JSON parser
               │     │ E: malformed/oversized/conflict ↯escape(no write)
               │     │ 🔒 config bytes → typed object
               │     └→ C10 backup_and_atomic_replace (1) @config-write
               │        │ A: durable backup + replaced configuration
               │        │ R: same-directory stage, flush, Windows atomic API
               │        │ E: backup/write/flush/replace failure ↯escape(original retained)
               │        └→ C11 verify_managed_json (1)
               │           │ A: verified exact managed entry
               │           │ R: fresh ownership checks and JSON parse
               │           │ E: mismatch ↯escape(restore backup or RepairRequired)
               └→ Guided
                  └→ C12 generate_guided_setup (1)
                     │ A: reviewed command/config and copy action
                     │ R: host template, canonical packaged path
                     │ E: clipboard failure ↯escape(show selectable text)

C8 | C11
└→ C13 record_setup_receipt (1)
   │ A: bounded private SetupReceipt
   │ R: SQLite/settings repository
   │ E: persistence failure ↯escape(config remains verified; warn)
   └→ C14 observe_pairing_state (T)
      │ A: configured → pairing pending → paired → granted state
      │ R: existing AgentAccessManager status
      │ E: host not restarted/desktop disabled ↯escape(actionable state)

C15 repair_managed_setup (1)
│ A: new reviewed HostSetupPlan
│ R: receipt, current packaged path, exact current host entry
│ E: unrelated drift/conflict ↯escape(no write)
└→ C6 present_setup_plan

C16 remove_managed_setup (1)
│ A: removed exact Tarik entry
│ R: receipt, current host state, official remove or managed JSON writer
│ E: entry drift/foreign replacement ↯escape(UndoConflict; no removal)
└→ C8 | C11 verify absence
```

### Guidance contract

```text
G1 publish_server_instructions (1)
│ A: concise security/workflow instructions
│ R: MCP server capability metadata
│ E: protocol does not support instructions ↯escape(prompts/docs)
└→ G2 expose_portable_prompts (N)
   │ A: versioned bounded prompts
   │ R: static prompt registry
   │ E: unsupported prompt capability ↯escape(server instructions)
   └→ G3 optionally_install_skill (1)
      │ A: exact reviewed Tarik skill entry
      │ R: compatible host adapter, direct local approval
      │ E: unsupported/unsafe/conflicting path ↯escape(no write)
```

### Destination delegation contract

```text
D1 choose_destination_in_tarik (1)
│ A: user-selected candidate directory
│ R: native folder picker
│ E: cancel ↯escape(no grant)
│ 🔒 picker result → candidate path
└→ D2 canonicalize_destination (1)
   │ A: trusted local destination identity
   │ R: filesystem/platform path policy
   │ E: root/network/reparse/app/project/cache path ↯escape(DestinationUnsafe)
   └→ D3 configure_delegation (1)
      │ A: bounded formats/chunk/byte policy
      │ R: visible Tarik policy editor
      │ E: invalid quota ↯escape(inline correction)
      └→ D4 persist_destination_grant (1)
         │ A: private ExportDestinationGrant
         │ R: SQLite, client/project identities, secure opaque ID
         │ E: write failure ↯escape(no grant)
         └→ D5 list_redacted_destinations (N)
            │ A: ID/label/formats/quotas/readiness only
            │ R: authenticated connection and project grant
            │ E: foreign/revoked client ↯escape(empty/authorization error)
```

### Full-query export contract

```text
E1 accept_export_intent (1)
│ A: typed bounded ExportIntent
│ R: bridge/MCP schema
│ E: SQL/path/raw COPY/unknown fields ↯escape(Blocked)
│ 🔒 MCP JSON → ExportIntent
└→ E2 consume_safe_read_snapshot (1)
   │ A: one-use server-held exact SQL snapshot
   │ R: existing snapshot registry, client/connection/project ownership
   │ E: missing/replayed/foreign snapshot ↯escape(Blocked)
   └→ E3 reclassify_snapshot (1)
      │ A: current SafeRead snapshot
      │ R: existing dual parser and AgentCatalogRevision
      │ E: drift/non-SafeRead ↯escape(SnapshotStale/Blocked)
      └→ E4 resolve_destination_policy (1)
         │ A: server-held path + ExportDecision
         │ R: destination repository, capability grant, filesystem policy
         │ E: guessed/foreign/moved destination ↯escape(Blocked)
         ├→ within grant + no collision → Delegated
         ├→ controllable policy exception → ApprovalRequired
         ├→ existing target/replacement → CriticalConfirmation
         └→ path/URL/raw COPY/unknown effect → Blocked
             ├→ E5 claim_delegated_or_approved_intent (1)
             │  │ A: one-use immutable export ticket
             │  │ R: atomic registry, exact client/project/snapshot/destination revisions
             │  │ E: race/replay/drift ↯escape(Invalidated)
             │  └→ E6 reserve_output_set (1) @export
             │     │ A: collision-checked generated output identities
             │     │ R: private path, base-name validator, create-new staging
             │     │ E: collision/quota ↯escape(no query starts)
             │     └→ E7 execute_complete_export (1)
             │        │ A: streamed CSV/Parquet parts
             │        │ R: existing ExportCoordinator and DuckDB Arrow stream
             │        │ E: SQL/write/cancel ↯escape(truthful partial terminal)
             │        │ E: ambiguous replacement ↯escape(RecoveryRequired)
             │        └→ E8 redact_export_manifest (T)
             │           │ A: bounded state/counters/relative filenames
             │           │ R: owner registry and response budget
             │           │ E: foreign/released export ↯escape(ExportMissing)
             ├→ E9 cancel_export (1)
             │  │ A: cancellation/cleanup status
             │  │ R: owner registry, existing interrupt/cancel path
             │  │ E: completed export ↯escape(idempotent terminal state)
             └→ E10 release_export (1)
                │ A: released transient ownership record
                │ R: export registry
                │ E: already released ↯escape(idempotent released state)
```

## CARDINALITY

`C1 locate_packaged_mcp (1)` · `C2 detect_hosts (1)` · `C3 inspect_host_candidate (N)` · `C4 inspect_existing_configuration (N)` · `C5 build_setup_plan (1)` · `C6 present_setup_plan (1)` · `C7 invoke_official_cli (1)` · `C8 verify_official_cli (1)` · `C9 prepare_managed_json (1)` · `C10 backup_and_atomic_replace (1)` · `C11 verify_managed_json (1)` · `C12 generate_guided_setup (1)` · `C13 record_setup_receipt (1)` · `C14 observe_pairing_state (T)` · `C15 repair_managed_setup (1)` · `C16 remove_managed_setup (1)` · `G1 publish_server_instructions (1)` · `G2 expose_portable_prompts (N)` · `G3 optionally_install_skill (1)` · `D1 choose_destination_in_tarik (1)` · `D2 canonicalize_destination (1)` · `D3 configure_delegation (1)` · `D4 persist_destination_grant (1)` · `D5 list_redacted_destinations (N)` · `E1 accept_export_intent (1)` · `E2 consume_safe_read_snapshot (1)` · `E3 reclassify_snapshot (1)` · `E4 resolve_destination_policy (1)` · `E5 claim_delegated_or_approved_intent (1)` · `E6 reserve_output_set (1)` · `E7 execute_complete_export (1)` · `E8 redact_export_manifest (T)` · `E9 cancel_export (1)` · `E10 release_export (1)`.

## BOUNDARIES

1. `🔒 Windows environment/PATH → bounded candidate paths`.
2. `🔒 filesystem metadata/PE/version output → HostInstallation`.
3. `🔒 host CLI output → typed current setup`.
4. `🔒 Claude Desktop JSON bytes → typed configuration object`.
5. `🔒 direct visible Tarik interaction → setup decision`.
6. `🔒 packaged executable location → canonical tarik-mcp.exe`.
7. `🔒 host configuration entry → exact managed-entry hash`.
8. `🔒 folder-picker result → private canonical destination`.
9. `🔒 MCP destination ID → server-held path`; the path never crosses back.
10. `🔒 MCP JSON → typed ExportIntent`; unknown security-relevant fields fail closed.
11. `🔒 snapshot ID → exact server-held SafeRead SQL`.
12. `🔒 generated part paths → contained destination files`.
13. `🔒 engine status/paths → redacted relative manifest`.
14. `🔒 host/OS/SQLite/engine errors → stable path-free errors`.
15. `🔒 data rows containing instructions → inert values`; they cannot alter setup, grants, destinations, or approvals.

## Host adapter contract

Initial one-click behavior on Windows is closed and versioned:

| Host                | Contract                                                                                                                         |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| Claude Desktop      | `%APPDATA%\Claude\claude_desktop_config.json`; merge only `mcpServers.tarik` with `command` and `args`                           |
| Claude Code         | Direct executable invocation: `claude mcp add --scope user tarik -- <tarik-mcp.exe> --profile claude-code --label "Claude Code"` |
| Codex               | Direct executable invocation: `codex mcp add tarik -- <tarik-mcp.exe> --profile codex --label Codex`                             |
| Pi, Cursor, VS Code | Guided setup only                                                                                                                |
| Generic             | Generated stdio configuration only                                                                                               |

Development-machine evidence captured before implementation:

- Claude Code `2.1.231` exposes `mcp add/get/list/remove`; user scope is explicit on add/remove.
- Codex CLI `0.149.1` exposes `mcp add/get/list/remove`; `get` and `list` support JSON.
- Claude Code verification uses bounded `mcp get tarik`; its output is untrusted and must contain the exact expected command and arguments.
- Codex verification uses bounded `mcp get tarik --json` and a strict typed parser.
- Windows one-click requires a canonical directly executable `.exe`. Shell-only npm shims fall back to guided instructions.

The exact Windows install locations, `.exe` availability, version output, Claude Desktop behavior, and host-launch result remain unverified until the native Windows evidence pass. Unknown locations or versions are `Guided`, never guessed.

## Managed Claude Desktop rules

- Maximum configuration input: 1 MiB.
- A missing file may be created only inside the verified current-user Claude directory.
- Existing input must be UTF-8 JSON with an object root.
- `mcpServers` must be absent or an object.
- Unrelated keys and MCP servers are preserved semantically.
- An identical `tarik` entry is already configured.
- A different `tarik` entry is a conflict and requires a new explicit replacement plan.
- Reject symbolic links, reparse points, non-regular files, unexpected owner/ACL, and unsafe parent directories.
- Write a same-directory timestamped backup and create-new staging file; flush before atomic replacement.
- Verify by reopening and parsing the resulting file.
- Undo removes only the exact entry identified by the receipt. If it drifted, undo fails without changing the file.
- Retain at most three Tarik-created backups per host; never delete a backup required by the current receipt.

## D5 decision: empty exports

The existing E9 contract remains authoritative: a successful query returning zero rows creates **zero files**.

```text
state=succeeded
rowsWritten=0
filesWritten=0
bytesWritten=0
completedParts=[]
```

The UI and MCP response explicitly state that no files were created. Tarik does not create a schema-only CSV or Parquet file.

## Bounded defaults

| Resource                        |                      Hard bound |
| ------------------------------- | ------------------------------: |
| Host candidates                 |                      8 per host |
| Concurrent host probes          |                               2 |
| Version probe timeout           |                       5 seconds |
| Setup/remove process timeout    |                      20 seconds |
| Captured stdout/stderr          |                     64 KiB each |
| Managed host config             |                           1 MiB |
| Retained setup receipts         |                              32 |
| Retained backups                |              3 per managed host |
| Setup plan lifetime             |                       5 minutes |
| Automatic setup retries         |                               0 |
| Destination grants              |            8 per client/project |
| Export base name                |          64 portable characters |
| Rows per part                   |                     1-1,000,000 |
| Delegated export size           | explicit grant, maximum 100 GiB |
| Active agent exports            |      1 per connection, 4 global |
| Export manifest response        |                         256 KiB |
| Retained terminal agent exports |                             256 |
| Export status update rate       |                maximum 1/second |

## BEHAVIOR

- `⛈ timeout/output-cap` wraps host probes and official CLI calls.
- `⛈ no-shell` wraps every host process invocation.
- `⛈ redacted diagnostics` wraps host/config/export operations.
- `⛈ plan-expiry` wraps setup, repair, remove, and one-use approvals.
- `⛈ backup/atomicity` wraps managed host configuration.
- `⛈ response-budget` wraps host state and export manifests.
- `⛈ rate-limit` wraps MCP destination/export tools.
- `⛈ quota` wraps delegated export staging/publication.
- `⛈ collision detection` wraps every part publication.
- `⛈ accessibility` wraps setup and destination-policy UI.
- `⛈ retention` wraps receipts, backups, export records, and audit.

There is no automatic retry for a configuration mutation, CLI setup, file replacement, or export overwrite.

## SCOPE

- `Host probe process acquire@C3 → wait/kill/reap@C3`.
- `Host setup process acquire@C7 → wait/kill/reap@C7`.
- `Config read handle acquire@C9 → close@C9`.
- `Config lock acquire@C9 → release@C11/error`.
- `Config stage acquire@C10 → publish/remove@C10`.
- `Config backup acquire@C10 → retain/restore/prune@C11`.
- `Setup plan acquire@C5 → consume/expire@C6`.
- `Setup receipt acquire@C13 → supersede/remove@C15/C16`.
- `Pairing observation timer acquire@C14 → release@close/pair/cancel`.
- `Folder picker acquire@D1 → release@select/cancel`.
- `Destination grant acquire@D4 → revoke@user/client/project removal`.
- `SafeRead snapshot acquire@classification → consume@E2`.
- `Export ticket acquire@E5 → terminal/release@E10`.
- `Output reservation acquire@E6 → publish/cleanup@E7`.
- `DuckDB Arrow stream acquire@E7 → drop@terminal/cancel`.
- `Current staging part acquire@E7 → publish/remove@part-terminal`.

## Threat model

| Threat                                   | Control                                                                                      | Required evidence                                                      |
| ---------------------------------------- | -------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| PATH/executable spoofing                 | closed locations, canonical regular PE, owner/reparse check, version allowlist, visible plan | spoofed executable and location fixtures; native Windows identity test |
| Shell-shim injection                     | no shell; `.cmd`, `.bat`, and PowerShell are guided only                                     | structured-argument process goldens                                    |
| Malicious CLI output                     | byte/time bounds and strict parsing                                                          | timeout and output-overflow tests                                      |
| Malformed Claude JSON                    | parse before write; no mutation                                                              | malformed/oversized/root/type fixtures                                 |
| Symlink/reparse replacement              | parent and target checks before read, stage, and publish                                     | symbolic-link fixtures plus native Windows reparse test                |
| Lost unrelated settings                  | semantic merge, backup, verification, exact undo                                             | config fixture matrix                                                  |
| Concurrent config edit                   | before-hash comparison immediately before replace                                            | injected race test                                                     |
| Portable Tarik moved                     | receipt mismatch gives `RepairRequired`                                                      | moved package fixture and native portable run                          |
| Skill prompt injection                   | guidance has no authority                                                                    | hostile row/prompt corpus                                              |
| Agent supplies destination               | export schema contains no path                                                               | schema golden/adversarial JSON                                         |
| Destination ID guessing                  | opaque ID plus client/project binding                                                        | cross-client/project tests                                             |
| Destination substitution                 | canonical identity and grant revision rechecked                                              | moved/replaced destination tests                                       |
| Capped browse result mistaken for export | export consumes complete immutable SQL snapshot, not result pages                            | >5,000-row full export test                                            |
| Quota exhaustion                         | staging/publication budget and active-export limits                                          | quota boundary tests                                                   |
| Filename collision                       | create-new reservation; overwrite uses critical approval                                     | collision/race tests                                                   |
| Approval replay                          | atomic one-use ticket bound to connection/project/snapshot/destination                       | concurrent claim tests                                                 |
| Path leakage                             | redacted manifests, audit, errors, and logs                                                  | redaction assertions                                                   |
| Cancellation ambiguity                   | truthful partial manifest; replacement ambiguity is recovery                                 | injected cancel/recovery tests                                         |

## Existing implementation inventory

| Existing component       | Reuse                                  | Required change                                                    |
| ------------------------ | -------------------------------------- | ------------------------------------------------------------------ |
| `AgentAccessManager`     | pairing, grants, snapshots, approvals  | setup-state facade in T1; export owner/ticket registry in T4       |
| `AgentBridge`            | authenticated private transport        | typed destination/export actions in T4                             |
| `tarik-mcp`              | static stdio MCP server                | prompts in T2; export tools in T4                                  |
| `ExportCoordinator`      | immutable SQL, polling, history        | explicit agent ownership, bounded retention/release, redacted view |
| Engine export jobs       | one-pass Arrow CSV/Parquet chunking    | byte quota/reservation and recovery semantics                      |
| E9 validation            | base name, options, paths, exact parts | keep path server-held; expose only typed options                   |
| Agent Access dialog      | pairing, grants, approvals             | connection assistant in T1 without changing authority              |
| Windows portable package | sibling `tarik-mcp.exe`                | setup uses canonical packaged sibling                              |
| SQLite metadata          | clients, grants, audit                 | setup receipts in T1; destination migration in T3                  |
| Tauri dialog plugin      | folder picker                          | destination selection in T3                                        |

`ExportCoordinator` currently retains exports in an unbounded `HashMap` and its internal completed-part summaries contain absolute paths. T4 must add owner-bound bounded release/retention and a separate redacted MCP view. It must not expose `ExportView` directly.

## Implementation boundaries

1. **T1:** platform-neutral host/setup domain, bounded direct process runner, Windows adapters, safe Claude Desktop merge, receipts, setup-plan UI, guided fallback, repair/remove, and tests.
2. **T2:** concise server instructions, versioned MCP prompts, optional reviewed Agent Skill, compatibility/install/remove adapters.
3. **T3:** private destination-grant migration/repository, path policy, folder picker, policy UI, redacted listing.
4. **T4:** typed MCP/bridge export actions, immutable ticket, complete streaming export, owner/status/cancel/release, audit/recovery/redaction.
5. **T5:** native Windows and Linux package, host, security, accessibility, resource, recovery, and manual evidence.

T1 is valid as a Linux-developed implementation candidate, but T0 and T1 checkboxes remain open until the native Windows contract and runtime evidence are attached.

## TEST LAYERS

Production requirements are swapped without changing the graph:

```text
Windows environment → fixture environment map
Filesystem identity → temporary tree + scripted owner/reparse metadata
Process runner       → scripted executable/version/stdout/stderr/timeout
Host adapters        → Claude/Codex golden outputs
Config writer        → temporary config + injected write/flush/replace failures
Clock                → frozen/advancing clock
Tarik UI decision    → recorded direct local decision provider
Setup receipts       → in-memory SQLite
Folder picker        → selected/cancelled provider
Destination policy   → temporary local/root/network/reparse fixtures
Agent authentication → deterministic client/connection/project grants
Classifier           → existing real dual-parser fixtures
Export engine        → real temporary DuckDB plus injected writer failures
Logger               → redaction-asserting sink
```

T1 coverage includes zero/one/multiple installations; `.exe` versus shell shims; spoofed name/type, wrong owner, reparse points, spaces, Unicode, and moved package; process success/nonzero/timeout/output cap/kill/reap; Claude/Codex add/get/list/remove argument goldens; missing/valid/malformed/oversized/conflicting/concurrently changed Claude JSON; backup/flush/atomic replacement/verification/repair/exact undo/failure injection; unrelated-setting preservation; minimum viewport; keyboard/focus; screen-reader labels; non-color states; and reduced motion.

Native Windows evidence must cover real Claude Desktop, Claude Code, and Codex host detection and launch; `%APPDATA%`; executable identity; ACL/reparse checks; process argument handling; atomic replacement under Windows file locking; portable movement/repair; WebView2; DPI/mixed-monitor; accessibility; and clean process exit.

## VERDICT

The graph preserves the E15 authority boundary. Setup does not pair clients or grant projects. No shell is invoked. Unsupported executables, versions, configuration shapes, and unverified Windows cases fail to Guided mode. The agent never receives or supplies a destination path. Routine create-new export may use a bounded Tarik-created delegation, while overwrite remains a fresh Tarik-owned critical confirmation. Empty exports create no files, and E9 streaming is reused instead of exporting capped MCP result pages.

The happy path and error joins are structurally separated. Every node has A/E/R, cardinality, boundary parsing, behavior, scoped resources, and replaceable test requirements. T1 implementation is valid only when code follows C1-C16 and does not broaden setup on an unsupported host.
