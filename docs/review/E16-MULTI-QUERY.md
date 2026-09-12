# E16 bounded multi-query MCP sessions review

**Status:** Automated implementation candidate. Manual real-Tauri, real Claude Desktop, accessibility, and native Windows process evidence are not approved.

## Implemented scope

- Bridge protocol v4 and 26 MCP tools, including bounded `tarik_list_active`.
- SafeRead query/result recovery authority is paired `clientProfileId + projectId`; connection ID remains origin attribution. Approval, mutation, and guarded export remain connection-bound.
- Four outstanding SafeReads and eight retained results per profile by default, with global count and cache budgets, immutable per-admission limit snapshots, exact cache-byte publication, row-cap truth, and fail-without-publication byte limits.
- Explicit snapshot reservation, engine acceptance, consumption/restoration, fair paired-profile queueing, serial MCP heavy execution, claim-time grant/catalog/source revalidation, claim-time DuckDB session connection cloning, and independent queue/execution watchdogs.
- Thirty-second adapter heartbeat, 120-second connection lease, resume-safe grace, 10-minute idle and 30-minute absolute result expiry, page-read leases, cleanup backpressure, and bounded file-lock retry.
- Header-adjacent Activity right workspace with Queries and Agents views, bounded visible-only summary polling, on-demand SQL detail, cancellation, result release, scoped Release all, Copy SQL, and non-executing editor drafts.
- Desktop-owned Conservative/Balanced/Large/Custom policy with hard combination validation and optional desktop-authorized execution deadline up to 300 seconds.
- MCP prompts, tool descriptions, packaged Agent Skill, and user documentation for multi-result recovery and truthful capped-result handoff.

## Process topology and duplicate-process investigation

Expected topology is one host-owned `tarik-mcp` stdio child per configured host transport instance. Tarik Desktop starts only its private bridge listener; it does not spawn `tarik-mcp`. Separate Claude Desktop, Claude Code, Pi, Codex, Cursor, VS Code, or duplicated host transport configurations can therefore legitimately create more than one adapter process.

Tarik's reviewed setup manager owns exactly one named `tarik` entry per supported host. It compares the complete command/argument shape and a private ownership receipt, refuses same-name foreign conflicts, verifies managed changes, and rolls them back on failed verification. It does not enumerate all system processes, impose a machine-wide singleton, or kill by executable name.

Activity → Agents now distinguishes paired-but-disconnected clients from each authenticated connection. Two live connections for one paired profile appear as two rows. PID remains **Unavailable** because the current local socket/named-pipe contract does not independently establish host child process identity. A self-reported PID would not be trusted as termination authority.

The user-reported Windows sequence (Tarik/workspace first, Claude Desktop second, sometimes two `tarik-mcp` processes) is **not reproduced or diagnosed on this Linux development host**. It must remain open until native Windows evidence records command lines, parent PIDs, transport/profile mapping, timing, configuration entries, and residual processes after host/Tarik shutdown.

## Automated evidence

Run after the E16 implementation units:

- `cargo test --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `npm run test:ui`
- `npm test`
- `npm run typecheck`
- `npm run build`
- `npm run lint` (two pre-existing warnings only)
- `npm run docs:check`
- `npm run engine:check`
- `npm run format:check`
- `git diff --check`

The review commit reruns and records final outcomes. Automated checks are not a substitute for native process, WebView2, keyboard, focus, accessibility, DPI, mixed-monitor, or screen-reader review.

## Required real-Tauri and Claude Desktop review

1. Launch packaged or development Tarik, open a project, and enable Agent access.
2. Open Activity. Confirm it is beside Agent access, switches only the right workspace, preserves Explorer, and Return restores editor tabs, draft text, results, Profile/Quality state, and focus.
3. Start a desktop query. Confirm Queries shows immutable SQL only after selection, meaningful elapsed/resource data, `Progress unavailable`, targeted cancellation, and a truthful terminal state.
4. Start Claude Desktop with one reviewed `tarik` configuration. Pair/grant if needed. Confirm Agents shows the paired profile and exactly the authenticated connection(s), not pairing alone as connectivity.
5. Ask Claude to inspect schema and run selected-column `LIMIT 100` raw exploration. Confirm queue/status/page/release behavior and no bulk paging guidance.
6. Queue multiple queries and retain multiple results. Restart the host transport, call `tarik_list_active`, recover same-profile IDs, compare pages, and release one result without affecting another.
7. Produce a browse-row-capped query. Confirm Claude says it is incomplete/non-exact and offers refine/aggregate, Open in editor, then guarded export. Confirm Open in editor creates a draft and never runs it.
8. Test queued and running cancellation from both MCP and Activity. Confirm cancellation-requested remains visible until terminal and cached paging/monitoring remain responsive.
9. Change analysis preset/custom limits in Activity. Confirm invalid combinations are rejected and existing work retains its admitted limits while new work uses the saved values.
10. Release a result and scoped Release all. Confirm result access ends while tables, sources, projects, editor drafts, and completed exports remain.
11. Disable/re-enable Agent access, remove Analyze, revoke client, close/reopen project, restart Tarik, and test sleep/resume. Confirm authority, heartbeat, expiry, cleanup, and startup cache behavior match the lifecycle contract.
12. On native Windows, execute the duplicate-process matrix below and retain evidence before approving E16-T7/T5.

## Native Windows duplicate-process matrix

For each scenario, capture timestamp, configured host entries, adapter command line, adapter PID, parent PID/process, Activity profile/connection ID, and residual processes after shutdown:

- Tarik first → workspace → Claude Desktop first launch.
- Claude Desktop first → Tarik → workspace/Agent access.
- Claude Desktop quit/relaunch and update/restart.
- Bridge disconnect/reconnect and Tarik disable/re-enable.
- Workspace close/open and Tarik app restart.
- Deliberately configure two host transports, then distinguish expected multiplicity.
- Remove duplicate/same-path host entries and repeat.
- Move the portable package and repair the managed entry.
- Close Claude Desktop, then Tarik; verify no unintended orphan remains.

Do not approve based only on Task Manager process count. Do not kill all `tarik-mcp.exe` processes by name.

## Acceptance checklist

- [x] Automated Rust, MCP, UI, Node, type, build, docs, engine, format, and diff gates pass for implementation candidate.
- [x] Multiple bounded results, recovery, expiry, cleanup, scheduling, watchdogs, guidance, Activity, and desktop-owned limits are implemented.
- [x] Agent Monitor distinguishes paired clients and authenticated connections; unknown PID is labelled unavailable.
- [ ] Real Tauri desktop workflow approved.
- [ ] Real Claude Desktop multi-query/reconnect/capped-result workflow approved.
- [ ] Keyboard/focus, minimum viewport, light/dark, reduced-motion, and screen-reader review approved.
- [ ] Native Windows x64 MSVC/WebView2/DPI/mixed-monitor responsiveness approved.
- [ ] Reported duplicate-process sequence reproduced or closed with recorded parentage/configuration evidence.
- [ ] User approves E16 product increment.
