# E12 Windows portable runtime review

**Status:** IN REVIEW — a native portable candidate can be verified locally; clean-machine, DPI, mixed-monitor, and missing-WebView2 manual evidence remains pending.

**Scope:** Extracted portable integrity, WebView2 prerequisite behavior, fresh-profile launch/restart, complete desktop process-tree memory, large-result paging, completed and cancelled exports, Windows 10/11 clean-machine workflows, DPI/mixed-monitor rendering, light/dark/system themes, and keyboard focus.

## Candidate identity

Complete these fields from `target/release-artifacts/windows/release-manifest.json`. Do not review an archive whose values differ. GitHub Actions artifacts are optional supplemental evidence.

| Field           | Required value                                              | Observed                                                           |
| --------------- | ----------------------------------------------------------- | ------------------------------------------------------------------ |
| Version         | `0.1.0`                                                     | `0.1.0`                                                            |
| Target          | `x86_64-pc-windows-msvc`                                    | `x86_64-pc-windows-msvc`                                           |
| Signed          | `false` unless an approved signing job changes the manifest | `false`                                                            |
| Git revision    | Exact reviewed commit                                       | `370e1bb7a625b35b6ddd7db3d8532959ee363928`                         |
| ZIP             | `Tarik-0.1.0-windows-x64-portable.zip`                      | `Tarik-0.1.0-windows-x64-portable.zip`                             |
| ZIP bytes       | Recorded manifest value                                     | `19,555,559`                                                       |
| ZIP SHA-256     | Same in manifest and outer `SHA256SUMS`                     | `245bf7e6479b17634dec2b311de173581d2f2dfef0dc04abc63404bded2b5017` |
| DuckDB          | `1.5.5`                                                     | `1.5.5`                                                            |
| Engine protocol | `1`                                                         | `1`                                                                |
| Metadata schema | `7`                                                         | `7`                                                                |

## Implemented automated boundary

Commits under review:

- `4112b8c` — native x64/MSVC portable packager, exact contents, PE checks, inner/outer checksums, ZIP extraction, sidecar handshake, CI upload;
- `39495a4` — runtime verification design graph;
- `3b48b5f` — handshake child processes must exit cleanly;
- `f480499` — extracted desktop launch/restart, WebView2/process/memory observation, packaged-sidecar workload, bounded report, CI evidence upload.

Run `npm run release:windows`, then `npm run verify:windows-manual` on the review machine. Manual mode preserves Tarik's application-specific Roaming and Local AppData roots, measures only events produced by its own two launches, and writes evidence under `target/windows-manual-evidence/`.

`npm run verify:windows-runtime` remains an optional CI-only fresh-profile check. It intentionally deletes Tarik's AppData roots and must never be run by spoofing GitHub Actions variables on a normal workstation.

Expected evidence artifact:

```text
target/windows-manual-evidence/
└── runtime-report.json
```

The report must have `schemaVersion: 1`, `evidenceMode: "manual"`, and `verdict.automatedPassed: true`. A missing or failed report is not a pass. `profile.preserved` must be true; pre-existing AppData is disclosed rather than removed.

### Automated acceptance

- [x] Outer checksum and release manifest identify the same ZIP and SHA-256.
- [x] Extracted package has the exact allow-listed files; executable/DLL files are Windows PE; internal checksums pass.
- [x] First launch starts extracted `Tarik.exe` from its package directory; the report truthfully records whether Tarik AppData already existed and preserves it.
- [x] A responding main window, `tarik.sqlite`, and structured startup log appear within the fixed 60-second deadline.
- [x] Closing the native main window completes Tarik's frontend-coordinated graceful shutdown with exit code 0 and a structured `app/graceful_shutdown` event; force-kill is not counted as success.
- [x] Second launch uses the same profile, exposes a responding window, records a second startup and coordinated-shutdown event, and exits gracefully.
- [x] Both desktop launches observe at least one descendant `msedgewebview2.exe`; its product version is recorded when Windows exposes the executable path.
- [x] Peak working set for the full Tarik/WebView2 descendant tree is sampled and remains at or below 768 MiB for the fixed idle launch/restart smoke. This is a conservative regression ceiling, not an idle-memory claim.
- [x] The extracted sidecar handshakes as DuckDB protocol 1 with its sibling `duckdb.dll`.
- [x] A 100,000-row result publishes bounded pages; first and last 500-row pages are readable; result cache is zero bytes after release.
- [x] A 250,000-row CSV export succeeds with exact part rows `100,000 / 100,000 / 50,000`.
- [x] A one-billion-row requested export reaches running, is cancelled, and leaves no hidden `.tarik-export-*` stage.
- [x] Sidecar peak working set is sampled and remains at or below 512 MiB for the fixed workload.
- [x] The sidecar closes its session and exits cleanly after stdin closes.
- [x] `runtime-report.json` records evidence mode, profile preservation, OS version, architecture, Node version, artifact identity, fixed dataset, budgets, process samples, workload results, and pending manual gates without SQL/result payloads.

### Local automated result

Recorded September 5, 2026 on Windows 10 Enterprise 10.0.19045 x64 with Node 22.17.1:

- manual evidence passed with the existing Roaming and Local AppData roots preserved;
- both launches were responsive, observed six WebView2 152.0.4191.62 processes at device scale factor 1.5, exited with code 0, and recorded coordinated shutdown;
- desktop/WebView2 process-tree peak was 374,861,824 bytes against the 768 MiB ceiling;
- the 100,000-row result returned exact 500-row first/last pages and released its cache to zero bytes;
- the 250,000-row export produced exact `100,000 / 100,000 / 50,000` parts;
- the one-billion-row requested export cancelled with zero files, bytes, or hidden stages;
- sidecar peak was 20,901,888 bytes against the 512 MiB ceiling and it exited cleanly.

The captured 1.5 device scale factor proves the WebView2 process received 150% scaling; it does not replace visual sharpness, layout, pointer-target, or keyboard-focus review.

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

Baseline implementation evidence recorded after `f480499`:

- format and documentation checks pass;
- ESLint: zero errors and three pre-existing warnings;
- TypeScript typecheck passes;
- 26 Node command/platform/package/runtime tests pass;
- 150 UI tests across 25 files pass;
- production frontend build passes;
- Rust format and workspace Clippy with warnings denied pass;
- real DuckDB sidecar build/handshake passes;
- Rust workspace passes: desktop 75 tests plus all engine protocol, sidecar, fixture, and documentation suites;
- both runtime modes fail closed off native Windows x64; CI mode additionally requires an ephemeral GitHub Actions runner.

## Sign-off

T4 remains unchecked in `TASK.md` until:

- [x] `npm run release:windows` succeeds for the exact reviewed commit and the ZIP/checksum/manifest identities are recorded above.
- [x] `npm run verify:windows-manual` passes and the automated checklist above is checked against `target/windows-manual-evidence/runtime-report.json`.
- [ ] Windows 10 and Windows 11 clean-machine rows pass.
- [ ] All 100/125/150/200% and mixed-monitor rows pass.
- [ ] Missing-WebView2 behavior passes on a disposable machine.
- [ ] Light/dark/system, keyboard focus, large-result memory, and long-running export behavior are accepted.
- [ ] The user explicitly approves E12-T4.

Do not begin E12-T5 or call the Windows artifact release-ready while this review is blocked.
