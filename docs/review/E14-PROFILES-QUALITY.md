# E14 profiles and quality checks review

## Scope and acceptance state

This packet reviews the complete local beginner data-trust loop:

1. Profile a table without an implicit scan.
2. Interpret Exact, Approximate, and Sampled measurements.
3. Create a guided, unexecuted quality-check draft.
4. Inspect backend-compiled SQL evidence.
5. Save an immutable revision.
6. Run one check or the enabled suite explicitly.
7. Distinguish a valid data failure from an execution error.
8. Inspect a bounded current-data failure preview.
9. Repair data or a definition explicitly.
10. Rerun the intended revision and verify a later pass.
11. Restart Tarik and reopen aggregate history plus immutable revision evidence.

Implementation and automated gates did not constitute visual acceptance. The user reviewed the combined E14-T6/T7 workflow in the real Tauri app, requested iterative navigation, scrolling, startup, query-response, and result-state fixes, and explicitly approved commit on September 6, 2026. E13-T6 native Windows package acceptance remains separately open, so this packet does not claim final cross-platform acceptance.

## Deterministic fixture

- Setup and explicit repair SQL: `tests/fixtures/e14/data-trust.sql`
- Expected profile metrics, definitions, outcomes, and lifecycle: `tests/fixtures/e14/expected-results.md`

Use a disposable project. Run only the setup section first. Do not run the repair section until after reviewing the failed run evidence.

## Manual real-Tauri checklist

Start from a current local engine and clean development process:

```bash
npm run engine:build
npm run tauri:dev:clean
```

### Profile and handoff

- Open `main.orders` Profile from the Explorer. Confirm opening does not scan.
- Use exact distinct mode and run explicitly.
- Confirm the facts in `expected-results.md`, including 1 NULL customer ID, 4 distinct order IDs, and amount range -7 through 1500.
- Confirm every value says Exact, Approximate, or Sampled and SQL evidence can be copied/opened without running.
- Create a check from one observation. Confirm it opens as an unsaved, unexecuted draft.

### Definition authoring

- Confirm `Definitions | Runs` remains inside Quality Checks.
- At wide width, verify Saved checks, Definition, and SQL evidence own non-overlapping panes.
- Narrow the workspace and verify explicit Checks, Definition, and SQL tabs replace the panes. No SQL panel may overlay fields.
- Review not-empty, not-null, unique, accepted-values, range, relationship, freshness, and custom SQL fields.
- Confirm NULL behavior and failure semantics are visible and input survives validation errors.
- Confirm Save and Run are distinct. A dirty draft cannot run as though it were saved.
- Confirm Copy SQL and Open SQL never execute.
- Confirm custom SQL and suites containing custom SQL require a separate confirmation.

### Runs and recovery

- Run one failed check. Confirm the workspace switches to Runs automatically.
- Confirm queued/running states include text, elapsed time, and Cancel.
- Run the enabled suite. Confirm each check remains visible and counts cover queued, running, passed, failed, error, and cancelled.
- Select a failed run. Confirm the detail says the check ran successfully but the data did not meet the expectation.
- Select an errored run if available. Confirm it says the expectation was not evaluated and does not call it a data failure.
- Confirm observed and expected facts use beginner language and exact failure counts.
- Open a failure preview. Confirm the label says current-data preview using the revision and explicitly denies historical row retention.
- Page through enough data to exercise the existing result grid, NULL markers, truncation, selection, Copy page, and close/release.
- Open immutable SQL and confirm it does not execute.
- Edit a check to create a newer revision. Reopen the older run and verify both revision numbers are visible.
- Rerun the historical run. Confirm it uses its historical revision, not the current definition. Reconfirm custom SQL when applicable.
- Follow Edit check, Profile target, source repair, or engine recovery guidance. Confirm no recovery action mutates or runs automatically.
- Run the explicit fixture repairs, rerun, and verify new passes while old failed aggregates remain.
- Clear history and confirm definitions, data, sources, and exports remain.

### Restart and resource lifecycle

- Leave aggregate history, restart Tarik, reopen the project, and inspect a historical run.
- Confirm immutable revision detail and SQL return after restart.
- Confirm a historical failure preview runs against current data and closes cleanly.
- Start and cancel a long check, a suite member, and a failure preview. Confirm the session remains usable.
- Close the project and app with active work. Confirm no result pages, count artifacts, preview artifacts, pollers, or sidecar processes remain.
- Repeat suites beyond the 100-run per-check retention limit and verify history stays bounded.
- Inspect logs and confirm there is no generated/custom SQL, accepted value, profile value, or failing-row data.

### Visual and accessibility matrix

Review all relevant Profile, Definitions, Runs, empty, loading, pass, fail, error, cancelled, and preview states:

- Light, dark, and system themes.
- 680x520 minimum viewport.
- Desktop and medium workspace widths.
- 100%, 125%, 150%, and 200% scaling where the host allows it.
- Keyboard-only navigation, visible focus, confirmation focus return, result-grid controls, and screen-reader labels.
- Reduced motion, confirming spinners become static while status text remains complete.

Windows WebView2, mixed-monitor DPI, process invisibility, and portable package behavior remain owned by E13-T6/E12-T4 rather than being inferred from Linux review.

## Automated evidence required before sign-off

- Full Cargo workspace tests and clippy with warnings denied.
- Node command/platform/package tests.
- Full UI suite, TypeScript, ESLint, Prettier, documentation, production build, sidecar build/check.
- Fresh and schema-7 metadata migration coverage.
- Deterministic quality compiler matrix and real-sidecar fail, bounded preview, release, repair, pass workflow.
- Restart-safe run-detail/preview and historical-revision rerun tests.
- Coordinator terminal and preview bounds, cancellation, retention, cleanup, and redaction checks.
- Existing query/results/export cancellation and shutdown regressions.

## Review verdict

Approved for the reviewed real-Tauri environment on September 6, 2026. The approved candidate includes the complete Profile-to-check workflow, Definitions and Runs presentation, immutable historical evidence, current-data failure previews, deterministic recovery, responsive failure-grid scrolling, direct Checks-to-Profile navigation, visible phased startup, responsive query submission/page decoding, and mutually exclusive result states. E14 implementation is complete, but final cross-platform acceptance remains blocked on E13-T6/E12-T4 Windows package, clean-machine, DPI, mixed-monitor, and process-lifecycle evidence.
