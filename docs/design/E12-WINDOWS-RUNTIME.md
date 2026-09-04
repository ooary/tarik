# E12 Windows portable runtime verification design graph

PROBLEM: Verify that the extracted Windows portable release starts with isolated user state and remains bounded under real DuckDB work, without substituting hosted automation for clean-machine, DPI, or mixed-monitor review.

X → DesignGraph<A, E, R>
│ │ │ │ │
│ │ │ │ └─ R: extracted portable folder, Windows x64, WebView2, native process APIs, pinned DuckDB sidecar, disposable AppData
│ │ │ └──── E: invalid package, missing WebView2, launch timeout, early exit, process leak, memory breach, result/export residue, unavailable manual environment
│ │ └─────── A: PortableCandidate, RuntimePrerequisite, IsolatedProfile, LaunchEvidence, WorkloadEvidence, ManualMatrix
│ │
│ └─ nodes = functions, edges = package/process/protocol/evidence flow
│
└─ the problem: truthful Windows runtime evidence

SHAPES: PortableCandidate(root, checksums, manifest), RuntimePrerequisite(webView2Version|missing), IsolatedProfile(ephemeralRunner, cleanKnownFolders, TEMP), LaunchEvidence(pid, window, startupLog, metadata, restartCount, peakProcessTreeRss), WorkloadEvidence(engineInfo, dataset, largeResult, completedExport, cancelledExport, cleanup, peakSidecarRss), ManualMatrix(os, dpi, monitors, theme, keyboard, workflow, reviewer), RuntimeFailure(invalidPackage|missingPrerequisite|timeout|earlyExit|memoryBudget|residue), ReviewState(automatedPass|manualPending|approved)

GRAPH:

```text
locate_release_outputs (1)
│  R: target/release-artifacts/windows
│  E: missing ZIP|manifest|outer checksum ↯escape(non-zero; upload nothing)
└─ 🔒 release files → PortableCandidate
   └─→ extract_and_verify (1)
       R: exact allow-list, PE validation, inner + outer SHA-256
       E: extra|missing|tampered file ☠die(candidate rejected)
       └─→ probe_webview2 (1)
           R: native WebView2 runtime discovery
           E: runtime missing ↯escape(clear Microsoft prerequisite guidance; no launch claim)
           └─ 🔒 native runtime response → RuntimePrerequisite
              └─→ create_isolated_profile (1) @runtime-smoke
                  R: ephemeral GitHub Actions runner + empty Tarik known-folder roots
                  E: known-folder resolution|profile cleanup failure ↯escape(non-zero; preserve diagnostics)
                  ├─→ launch_portable (N) @desktop-launch
                  │   R: extracted Tarik.exe, freshly deleted Roaming/Local AppData roots
                  │   E: early exit|startup timeout|no window ↯escape(collect process/log evidence; terminate tree)
                  │   ├─→ observe_startup (T)
                  │   │   R: metadata file, structured startup log, native process tree
                  │   │   E: evidence timeout ↯escape(fail launch smoke)
                  │   ├─→ sample_process_tree_memory (T)
                  │   │   R: CIM process parent/working-set records, fixed deadline
                  │   │   E: enumeration unavailable ↯escape(fail rather than report partial total)
                  │   └─→ close_process_tree (1)
                  │       E: graceful close timeout ⟳retry×1 → ↯escape(force tree stop; record forced=true)
                  ├─→ restart_same_profile (1)
                  │   R: first launch artifacts + same isolated profile
                  │   E: migration|startup regression ↯escape(fail restart smoke)
                  └─→ run_engine_workload (1) @engine-workload
                      R: extracted sidecar + sibling duckdb.dll, newline JSON protocol
                      E: handshake|query|page|export|cancel timeout ↯escape(shutdown child; retain bounded report)
                      ├─→ execute_large_result → page_edges → release_result (1)
                      ├─→ execute_completed_export → verify_parts (1)
                      ├─→ execute_long_export → cancel_export (1)
                      └─→ verify_no_residue → shutdown_engine (1)

record_automated_evidence (1)
│  R: package, launch, memory, workload observations + budgets
│  E: malformed or incomplete report ☠die(candidate rejected)
└─ 🔒 observations → versioned-schema JSON
   └─→ perform_manual_matrix (N)
       R: clean Windows 10 x64 machine, clean Windows 11 x64 machine, 100/125/150/200% DPI, mixed monitors, keyboard-only reviewer
       E: machine/display unavailable ↯escape(leave row pending; never infer pass from CI)
       └─→ approve_runtime (1)
           R: automated report passed + every required manual row signed
           E: any pending/failing row ↯escape(T4 remains unchecked)
```

CARDINALITY: locate_release_outputs (1) · extract_and_verify (1) · probe_webview2 (1) · create_isolated_profile (1) · launch_portable (N) · observe_startup (T) · sample_process_tree_memory (T) · close_process_tree (1) · restart_same_profile (1) · run_engine_workload (1) · execute_large_result (1) · page_edges (1) · release_result (1) · execute_completed_export (1) · verify_parts (1) · execute_long_export (1) · cancel_export (1) · verify_no_residue (1) · shutdown_engine (1) · record_automated_evidence (1) · perform_manual_matrix (N) · approve_runtime (1)

BOUNDARIES: Release ZIP, manifest, and checksum text are untrusted until exact-content, PE, schema, and digest checks pass; WebView2 discovery output is untrusted until it reports a non-empty native version; CIM process records are admitted only when rooted in the launched Tarik PID tree; JSON sidecar frames are parsed and matched to the requested ID; startup logs are evidence only when read from the freshly cleaned ephemeral runner profile; manual results become trusted only with OS build, DPI/monitor setup, artifact digest, date, and reviewer recorded.

BEHAVIOR: ⛈ fixed deadlines wrap launch, restart, queries, and exports · ⛈ 100 ms sampling wraps the complete desktop process tree and sidecar · ⛈ fail-closed budgets wrap report publication · ⛈ diagnostic retention preserves the report/profile on failure but deletes successful temporary state · ⛈ CI artifact upload wraps evidence generation without changing the runtime graph.

SCOPE: extracted candidate acquire@CI → retain@artifact · isolated profile acquire@runtime smoke → delete@success or retain@failure · desktop process tree acquire@launch_portable → graceful/forced close@finally · sidecar acquire@run_engine_workload → protocol shutdown/kill/await@finally · DuckDB session acquire@workload → close@finally · result pages acquire@query → release@scenario · export stages acquire@engine writer → publish or remove@terminal state.

TEST LAYERS: Pure Node tests provide temporary fake PE files, checksums, manifests, process snapshots, and workload reports; native `windows-latest` provides the extracted release, WebView2, CIM, freshly cleaned ephemeral runner AppData, and real sidecar; manual review provides physical Windows 10/11 machines and display/input configurations. The graph and report schema remain unchanged across layers.

VERDICT: The portable packager already proves archive integrity and sidecar handshake, while Tauri 2.11.5 already presents a release-mode missing-WebView2 dialog with Microsoft installation guidance. T4 still lacks extracted `Tarik.exe` launch/restart evidence, full desktop process-tree memory, a Windows release-sidecar workload report, and a signed manual runtime matrix. Implementation is valid only if CI generates the first four without claiming the manual Windows/DPI/mixed-monitor rows, and T4 remains unchecked until those rows and clean-machine workflows pass.
