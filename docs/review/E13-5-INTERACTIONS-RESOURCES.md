# E13.5 desktop interactions and DuckDB resources review

Date: September 5, 2026
Status: **IMPLEMENTED / REVIEW — manual and native Windows approval required**

## What changed

### Consistent Tarik dialogs

- Replaced all five `window.prompt` and twelve `window.confirm` production calls.
- Text-entry and confirmation dialogs are controlled by the feature that owns each operation.
- Immutable IDs, SQL, export options, and destructive targets are captured when a dialog opens.
- Destructive actions focus Cancel first; valid naming forms submit with Enter; Escape/outside close only while idle.
- Backend failures keep inputs/targets and remain in the dialog for retry.
- Native Tauri file/folder pickers remain unchanged.
- `tests/dialog-guard.test.mjs` prevents browser dialog APIs from returning.

### Saved-query correctness

- Query Library renders folder records independently from saved-query count.
- Empty folders are visible and keep Rename/Delete actions.
- Searches filter query rows without hiding folder identity.
- **Save query** now appears immediately before **Query library** in the editor toolbar.
- The direct modal captures an immutable SQL snapshot, requires a name, supports Unfiled/folder choice and folder creation, and never executes or changes a tab.
- Save/folder failures retain the draft; success announces the exact saved name and folder.

### Verified DuckDB resources

- Added Low memory (`512 MiB / 1 thread`), Balanced (`2 GiB / 2 threads`), Fast (`8 GiB / 4 threads`), and Custom (`128–262,144 MiB / 1–256 threads`).
- One application-wide request is persisted in SQLite.
- Session open, project switching, and sidecar recovery apply the request before publishing a connected session.
- DuckDB settings are bound parameters, then read through `current_setting`; only readback becomes effective status.
- Query and export connection clones are proven to inherit configured memory/thread values.
- Resource changes are refused by desktop coordinators and the sidecar while query/Actual Flow/export work is queued or running. Nothing is auto-cancelled.
- A partial apply restores the previous verified pair before returning failure.
- The app-owned temporary/spill boundary remains unchanged.
- Hardware warnings compare Custom values with detected physical memory and logical CPUs but do not silently clamp them.

## UI contract

Footer states:

- `DuckDB resources: Checking`
- `DuckDB resources: <preset> · Pending` when saved but no project session has verified it
- `DuckDB resources: <preset> · <memory> · <threads>` only after DuckDB readback
- `DuckDB resources: Unavailable` when status cannot be read

The compact modal contains preset rows, Custom memory/unit/thread fields, verified current/requested context, Apply/Cancel, inline validation/failures, and this required wording:

> Limits DuckDB working memory. Total Tarik process memory can be higher.

## Automated evidence

Passed locally on Linux:

```text
npm run typecheck
npm run lint                       # zero errors; two documented pre-existing warnings
npm run build                      # production build; existing chunk-size advisory only
npm test                           # 31/31 Node tests
npm run test:ui                    # 166/166 UI tests
npm run docs:check
npm run format:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p tarik-engine-protocol
cargo test -p tarik-engine-duckdb --bin tarik-engine-duckdb
npm run engine:build
cargo test -p tarik-engine-duckdb --test engine_protocol resource_protocol_applies_reads_back_and_survives_connection_clones
cargo test -p tarik-engine-duckdb --test engine_protocol resource_change_is_rejected_while_session_work_is_active
cargo test -p tarik --lib engine_resources
cargo test -p tarik --lib engine_manager::tests
cargo test --workspace
npm run engine:check
LD_LIBRARY_PATH="$PWD/target/release" python3 scripts/benchmark-memory.py --engine target/release/tarik-engine-duckdb --report target/e13-5-memory-report.json
git diff --check
```

Focused assertions include:

- zero production alert/prompt/confirm calls;
- native picker preservation;
- focus, Enter/Escape, busy, error, and destructive-Cancel behavior;
- direct immutable save and no editor execution/tab mutation;
- empty-folder visibility and in-flow folder creation;
- preset/custom validation and hardware warnings;
- truthful pending/effective footer states;
- active-work refusal;
- typed protocol serde and hard bounds;
- real DuckDB readback through primary, query clone, and export clone connections;
- crash recovery reopens with the requested settings.

Linux release memory evidence (`target/e13-5-memory-report.json`) passed the versioned E11 budgets: 112,676 KiB peak sidecar RSS, 1,456 KiB post-cycle growth, zero result cache after release, and zero hidden export stages after cancellation.

## Manual review checklist

### Dialogs and saved queries

- [ ] Light, dark, and system themes have readable title, body, field, error, warning, and button contrast.
- [ ] Keyboard-only: Tab stays in each modal; Shift+Tab reverses; naming Enter submits once; idle Escape cancels; destructive Cancel starts focused; focus returns to the initiating control.
- [ ] Pointer: outside click cancels only while idle; double-clicking a submit cannot duplicate the operation.
- [ ] Minimum supported window contains every dialog without clipped actions.
- [ ] First empty Query Library folder appears immediately and survives close/reopen and app restart.
- [ ] Direct Save query is immediately before Query library, saves the shown snapshot, and does not run/open/change SQL.
- [ ] Every destructive/mutation modal names the operation, target, and preservation consequence accurately.

### DuckDB resources

- [ ] Footer shows Pending with no open project and verified values after opening one.
- [ ] Low memory, Balanced, Fast, and representative Custom MiB/GiB/thread values read back exactly.
- [ ] Applying while a query is queued/running leaves it running and keeps the modal open with guidance.
- [ ] Applying while Actual Flow runs leaves it running and refuses the change.
- [ ] Applying while export is queued/running leaves it running and refuses the change.
- [ ] Settings survive project close/reopen, project switch, app restart, and forced sidecar recovery.
- [ ] Invalid/failed apply never changes the footer to an unverified value.
- [ ] Warning text appears above detected RAM/CPU without calling the setting total Tarik memory.

### Native Windows package

- [ ] Review at 100/125/150/200 percent DPI and mixed-monitor movement.
- [ ] Confirm no sidecar console flash, extra taskbar/Alt+Tab window, or duplicate sidecar.
- [ ] Run packaged query/result/export/cancel/restart lifecycle after each preset.
- [ ] Capture updated `runtime-report.json` and process-tree peak/growth/residue evidence.
- [ ] Complete the remaining E13-T6 branding/Results/Windows 10/11 checklist against the same candidate.

## Remaining boundary

No native Windows package was built or manually inspected in this Linux session. The code is review-ready, but E13.5-T5 and E13-T6 must remain open until the user reviews the UI and the packaged Windows matrix passes. E14 remains blocked.
