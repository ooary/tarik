# E11.5 desktop correction review

**Status:** APPROVED - user accepted all nine desktop corrections on 2026-09-04.

**Scope:** Dark-theme legibility, Dracula SQL editors, working New table, scoped context menus, bounded result selection/copy/rerun, explorer cleanup, semantic project actions, and truthful DuckDB connection state.

## Implementation evidence

| #   | Reported correction                                             | Where                                                                                                         | Automated evidence                                                                                                                                                                                                                                                            |
| --- | --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Dark result text is unreadable                                  | `src/styles/tokens.css`, `src/App.css`, effective-theme resolver in `src/app/useEffectiveTheme.ts`            | Near-white `--result-text` / distinct `--result-null` tokens apply through one `data-effective-theme` scope covering values, NULL markers, active rows, and selected cells; preference/theme tests pass                                                                       |
| 2   | Dracula editor when system is dark                              | `src/features/editor/SqlEditor.tsx`                                                                           | Canonical Dracula theme plus highlight style reconfigure through a compartment; manual Light, Dark, System, and mocked live OS flips tested without losing text, focus, lint, or completion state                                                                             |
| 3   | New table does nothing                                          | `src/features/sources/NewTableDialog.tsx`, sidecar `catalog.create_table`, `projects::commands::create_table` | Dialog validation, typed command test, sidecar tests for quoted reserved-word creation and duplicate/type rejection without table creation, catalog refresh after exactly one `CREATE TABLE`                                                                                  |
| 4   | Browser Reload/Inspect context menus                            | document-scoped suppression in `src/App.tsx`                                                                  | Header `contextmenu` event is prevented; no browser menu, no custom menu on unsupported chrome                                                                                                                                                                                |
| 5   | Editor right-click Run; result copy/rerun; Shift/Ctrl selection | `SqlEditor.tsx`, `ResultGrid.tsx`, `resultSelection.ts`, `useQueryExecution.ts`                               | Editor menu delegates to shared Run; cell tests cover single, Shift rectangle, Ctrl disjoint, right-click-preserving selection, TSV with NULL/quote/tab/newline escaping, clipboard failure feedback, and immutable snapshot rerun through the shared mutation-confirmed path |
| 6   | No right-click on headers                                       | `ResultGrid.tsx` header rendering plus suppression                                                            | Header test proves no Tarik menu and suppression blocks the browser menu; resize and keyboard resize still pass                                                                                                                                                               |
| 7   | Remove Explorer `+`                                             | `src/App.tsx` panel heading                                                                                   | Button removed with icon import; regression asserts absence; refresh remains automatic                                                                                                                                                                                        |
| 8   | Green New project, red Close project                            | semantic button classes in `src/App.css`                                                                      | Class assertions plus token-based contrast in both themes                                                                                                                                                                                                                     |
| 9   | Bigger, truthful, shiny connection orb                          | `.status-mark-*` states driven by engine-backed transitions in `src/App.tsx`                                  | 10 px reflective orb with connected highlight/failure state tests: failure never shows connected, close returns idle, text always accompanies color, transition disabled under reduced motion                                                                                 |

## Gate results

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- Real sidecar rebuild plus `./scripts/check-engine.sh` handshake
- `cargo test --workspace --no-fail-fast`
- `npm run format:check`
- `npm run docs:check`: 28 Markdown files
- `npm run lint`: zero errors, six pre-existing warnings
- `npm run typecheck`
- `npm test`: 12 typed command tests
- `npm run test:ui`: 151 tests across 25 files
- `npm run build`

Invoke parity at E11.5 review was 55/55 handlers/frontend commands with the intentional `create_table` addition. E12-T0 later removed the runtime-unreferenced `get_app_directories` pair, leaving exact 54/54 parity reflected in `docs/review/E11-RELEASE.md`.

## Manual sign-off

Complete each check in the running desktop (Light, Dark, System, minimum 680x520 viewport):

- [x] Dark theme: result values, NULL markers, active/selected rows, loading, and error states are readable; SQL editor is Dracula in manual Dark and System dark
- [x] OS theme change while running flips editor palette live without losing editor state
- [x] New table: create a table with spaces/reserved words, see it appear in Explorer and completion, then delete it
- [x] New table rejects empty/duplicate columns and an already-existing name with inline guidance
- [x] Right-click on header, headings, empty space, result headers, and unsupported explorer areas shows no browser or Tarik menu
- [x] Editor right-click Run query submits the current SQL with the same confirmation as toolbar Run
- [x] Result right-click: copy cell/selection/row/page, Shift-rectangle, Ctrl/Command disjoint, NULL/tab/newline fidelity in a spreadsheet paste
- [x] Run query again reruns the SQL snapshot that produced the result even after editing SQL, with mutation confirmation
- [x] Explorer `+` is gone; New project is green, Close project is red, both keyboard-focused correctly
- [x] Connection orb is larger, shiny only when genuinely connected, and text matches every state including engine failure

Approval recorded from the user's explicit acceptance on 2026-09-04. E12 may proceed; deferred E6/E7/E10 and final release gates remain separate.
