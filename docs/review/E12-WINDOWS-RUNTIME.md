# E12 Windows portable runtime review

**Status:** BLOCKED — automation is implemented, but no native Windows artifact/report or clean-machine manual evidence has been produced.

**Scope:** Extracted portable integrity, WebView2 prerequisite behavior, fresh-profile launch/restart, complete desktop process-tree memory, large-result paging, completed and cancelled exports, Windows 10/11 clean-machine workflows, DPI/mixed-monitor rendering, light/dark/system themes, and keyboard focus.

## Candidate identity

Complete these fields from `target/release-artifacts/windows/release-manifest.json` and the uploaded Actions artifacts. Do not review an archive whose values differ.

| Field           | Required value                                              | Observed  |
| --------------- | ----------------------------------------------------------- | --------- |
| Version         | `0.1.0`                                                     | _pending_ |
| Target          | `x86_64-pc-windows-msvc`                                    | _pending_ |
| Signed          | `false` unless an approved signing job changes the manifest | _pending_ |
| Git revision    | Exact reviewed commit                                       | _pending_ |
| ZIP             | `Tarik-0.1.0-windows-x64-portable.zip`                      | _pending_ |
| ZIP bytes       | Recorded manifest value                                     | _pending_ |
| ZIP SHA-256     | Same in manifest and outer `SHA256SUMS`                     | _pending_ |
| DuckDB          | `1.5.5`                                                     | _pending_ |
| Engine protocol | `1`                                                         | _pending_ |
| Metadata schema | `7`                                                         | _pending_ |

## Implemented automated boundary

Commits under review:

- `4112b8c` — native x64/MSVC portable packager, exact contents, PE checks, inner/outer checksums, ZIP extraction, sidecar handshake, CI upload;
- `39495a4` — runtime verification design graph;
- `3b48b5f` — handshake child processes must exit cleanly;
- `f480499` — extracted desktop launch/restart, WebView2/process/memory observation, packaged-sidecar workload, bounded report, CI evidence upload.

On `windows-latest`, `npm run release:windows` must finish before `npm run verify:windows-runtime`. The latter is intentionally restricted to a native x64 ephemeral GitHub Actions runner because it deletes Tarik's application-specific Roaming and Local AppData roots before launch.

Expected evidence artifact:

```text
Tarik-windows-x64-runtime-evidence/
└── runtime-report.json
```

The report must have `schemaVersion: 1` and `verdict.automatedPassed: true`. A missing report is not a pass. A failure report is retained when the verifier reaches the native CI boundary.

### Automated acceptance

- [ ] Outer checksum and release manifest identify the same ZIP and SHA-256.
- [ ] Extracted package has the exact allow-listed files; executable/DLL files are Windows PE; internal checksums pass.
- [ ] First launch starts extracted `Tarik.exe` from its package directory after Tarik's AppData roots were absent.
- [ ] A responding main window, `tarik.sqlite`, and structured startup log appear within the fixed 60-second deadline.
- [ ] Closing the native main window completes Tarik's frontend-coordinated graceful shutdown with exit code 0 and a structured `app/graceful_shutdown` event; force-kill is not counted as success.
- [ ] Second launch uses the same profile, exposes a responding window, records a second startup and coordinated-shutdown event, and exits gracefully.
- [ ] Both desktop launches observe at least one descendant `msedgewebview2.exe`; its product version is recorded when Windows exposes the executable path.
- [ ] Peak working set for the full Tarik/WebView2 descendant tree is sampled and remains at or below 768 MiB for the fixed idle launch/restart smoke. This is a conservative regression ceiling, not an idle-memory claim.
- [ ] The extracted sidecar handshakes as DuckDB protocol 1 with its sibling `duckdb.dll`.
- [ ] A 100,000-row result publishes bounded pages; first and last 500-row pages are readable; result cache is zero bytes after release.
- [ ] A 250,000-row CSV export succeeds with exact part rows `100,000 / 100,000 / 50,000`.
- [ ] A one-billion-row requested export reaches running, is cancelled, and leaves no hidden `.tarik-export-*` stage.
- [ ] Sidecar peak working set is sampled and remains at or below 512 MiB for the fixed workload.
- [ ] The sidecar closes its session and exits cleanly after stdin closes.
- [ ] `runtime-report.json` records OS version, architecture, Node version, runner image, artifact identity, fixed dataset, budgets, process samples, workload results, and pending manual gates without SQL/result payloads.

## Missing-WebView2 prerequisite review

Tauri 2.11.5 checks WebView2 before creating the webview in release builds. When unavailable, its runtime path displays a blocking error containing:

```text
Could not find the WebView2 Runtime.

Make sure it is installed or download it from
https://developer.microsoft.com/en-us/microsoft-edge/webview2
```

Source inspection is not clean-machine evidence. Use a disposable Windows VM/sandbox where WebView2 is genuinely unavailable; do not uninstall a managed runtime from a daily-use machine.

- [ ] On missing runtime, double-clicking `Tarik.exe` shows prerequisite guidance rather than a blank or silent process exit.
- [ ] Guidance names WebView2, links to Microsoft's official download, and does not claim DuckDB or Tarik must be reinstalled.
- [ ] After installing Evergreen WebView2 for the same user, the unchanged extracted package launches normally.
- [ ] Tarik never downloads or executes a WebView2 installer itself.

Record VM image/build, whether the runtime was absent for the current user, screenshot, installation source, and retest result.

## Physical Windows review matrix

Use the exact checksummed ZIP above. Extract it to a normal writable folder with a space and non-ASCII character in its path. Do not launch from the ZIP preview and do not install Node, Rust, or DuckDB.

Every row must record reviewer, date, Windows edition/build, GPU/display arrangement, effective DPI, theme, artifact SHA-256, result, and screenshot filenames. `windows-latest` cannot satisfy these rows.

| ID      | Machine                             | Display setup                                            | Theme/input                 | Required observations                                                                                                   | Result    |
| ------- | ----------------------------------- | -------------------------------------------------------- | --------------------------- | ----------------------------------------------------------------------------------------------------------------------- | --------- |
| W10-100 | Clean Windows 10 x64                | Single display, 100%                                     | Light, mouse + keyboard     | Direct launch; no admin/install; full short workflow; restart/reopen                                                    | _pending_ |
| W10-150 | Clean Windows 10 x64                | Single display, 150%                                     | Dark, keyboard-only         | No clipping/overlap; focus visible; menus/dialogs/grid/flow usable                                                      | _pending_ |
| W11-125 | Clean Windows 11 x64                | Single display, 125%                                     | System light then live dark | Theme/editor/results switch live; text and selection remain legible                                                     | _pending_ |
| W11-200 | Clean Windows 11 x64                | Single display, 200%                                     | Dark, keyboard-only         | Header/explorer/editor/results/dialogs fit or scroll; no inaccessible action                                            | _pending_ |
| MIXED   | Windows 11 x64 recommended          | Two physical monitors, different 100/125/150/200% scales | System, mouse + keyboard    | Move window both directions; resize/maximize/restore; no blur, jump, lost focus, clipped menu, or broken pointer target | _pending_ |
| WEBVIEW | Disposable supported Windows x64 VM | Any                                                      | Default                     | Missing-runtime guidance and post-install recovery described above                                                      | _pending_ |

Across the matrix, cover all four effective scales: 100%, 125%, 150%, and 200%. A host zoom setting or browser screenshot resize is not DPI evidence.

### Short clean-machine workflow

For both W10-100 and at least one Windows 11 row:

1. Verify outer SHA-256, extract the ZIP, and confirm the exact package files remain together.
2. Double-click `Tarik.exe`; confirm no installer, administrator prompt, terminal window, or separate DuckDB prerequisite appears.
3. Create a managed project; import a CSV; link a Parquet file; run a grouped join.
4. Browse the first and later result pages; resize columns; copy selected cells; rerun the immutable result SQL.
5. Open Estimate and Actual Flow; inspect at least one node using keyboard and pointer.
6. Save SQL, complete a multi-part export, cancel a long query/export, and run another query afterward.
7. Close through the window button, launch the same `Tarik.exe` again, reopen the recent project, and verify catalog, drafts, saved SQL, history, and completed exports without rerunning SQL.
8. Move a linked Parquet file, verify Missing, repair it, and query again.
9. Confirm Task Manager shows no residual `Tarik.exe` or `tarik-engine-duckdb.exe` after final close.

## DPI, theme, and keyboard observations

Check each applicable item at its matrix row:

- [ ] Header project context and DuckDB status text remain visible; status is not color-only.
- [ ] Explorer resize handle remains reachable; long Unicode source/table names truncate without covering actions.
- [ ] SQL tabs, CodeMirror caret/selection/completion/lint, and Dracula effective-dark colors are sharp and aligned.
- [ ] Results headers/cells stay aligned while scrolling both axes; column resize and cell selection target the pointer location.
- [ ] Context menus open within the visible work area and remain associated with editor/result/explorer scope.
- [ ] New table, Import, Export, Query Library, Settings, Estimate, and Actual Flow dialogs/workspaces fit or provide bounded scrolling.
- [ ] Query-flow nodes/edges and inspector stay legible; moving monitors does not misplace selection or popovers.
- [ ] Tab/Shift+Tab traverses actionable controls in order; focus indication is always visible in light and dark.
- [ ] Escape closes the current menu/dialog without losing editor state; Enter/Space activates the focused action once.
- [ ] At 200%, no required action is available only through an off-screen pointer target.

## Current automated regression evidence

Recorded on Linux after `f480499` (implementation validation only, not Windows acceptance):

- format and documentation checks pass;
- ESLint: zero errors and three pre-existing warnings;
- TypeScript typecheck passes;
- 26 Node command/platform/package/runtime tests pass;
- 150 UI tests across 25 files pass;
- production frontend build passes;
- Rust format and workspace Clippy with warnings denied pass;
- real DuckDB sidecar build/handshake passes;
- Rust workspace passes: desktop 75 tests plus all engine protocol, sidecar, fixture, and documentation suites;
- `npm run verify:windows-runtime` fails closed on Linux with `Windows runtime verification must run on Windows`.

## Sign-off

T4 remains unchecked in `TASK.md` until:

- [ ] Native `windows-latest` package and runtime jobs pass and their ZIP/checksum/manifest/report are attached.
- [ ] The automated checklist above is checked against `runtime-report.json`.
- [ ] Windows 10 and Windows 11 clean-machine rows pass.
- [ ] All 100/125/150/200% and mixed-monitor rows pass.
- [ ] Missing-WebView2 behavior passes on a disposable machine.
- [ ] Light/dark/system, keyboard focus, large-result memory, and long-running export behavior are accepted.
- [ ] The user explicitly approves E12-T4.

Do not begin E12-T5 or call the Windows artifact release-ready while this review is blocked.
