# E13 desktop UX and engine lifecycle

Date: September 5, 2026

## Scope

E13 replaces only the New project browser prompt, fixes same-tab Results state loss, applies the supplied Tarik desktop logo, and makes the DuckDB sidecar lazy, reusable, recoverable, invisible, and coordinated with shutdown.

## New project state graph

| State      | Entry                                              | Exit                                         |
| ---------- | -------------------------------------------------- | -------------------------------------------- |
| Closed     | Initial state, Cancel, Escape, successful creation | New project trigger                          |
| Editing    | Dialog opens with focused name input               | Validation, submit, Cancel, Escape           |
| Invalid    | Empty, whitespace-only, or duplicate recent name   | User edits to a valid unique name            |
| Submitting | Valid form submit                                  | Backend success or failure                   |
| Failed     | Backend create/session/startup error               | User edits or resubmits                      |
| Succeeded  | Managed project and DuckDB session are open        | Dialog closes and focus returns to the shell |

Radix Dialog owns focus trapping, Escape dismissal, accessible title/description, and trigger focus restoration. Submission remains locked while the backend operation is pending, and failure keeps the typed name available.

## Engine state graph

| State         | Meaning                                                             | Transition                                                                        |
| ------------- | ------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Stopped       | No sidecar process exists                                           | Create, open, reopen, or another explicit engine operation starts it              |
| Starting      | One serialized caller starts the process and performs the handshake | Ready, connected, or failed                                                       |
| Standby       | Healthy process exists without a project session                    | Open a project reuses it; Tarik shutdown stops it                                 |
| Opening       | A project locator is sent to the healthy process                    | Connected or failed                                                               |
| Connected     | One remembered project session belongs to the current process ID    | Close, process exit, project switch, or shutdown                                  |
| Recovering    | The remembered session belongs to a dead/previous process           | Next safe session operation starts one process and reopens the remembered project |
| Failed        | Start, handshake, reopen, or operation failed                       | User retry or next safe operation                                                 |
| Shutting down | Coordinated shutdown releases work and owns final process exit      | Stopped                                                                           |

`EngineManager` serializes process ownership with one mutex. A session record stores its ID, DuckDB path, and owning process ID. A dead process is removed before reuse; the remembered session is reopened only when a later session operation needs it. Closing a project clears the session but intentionally keeps a healthy process in standby. Closing Tarik clears the session and shuts down the process.

## Ownership

| Owner                | Responsibility                                                                                 |
| -------------------- | ---------------------------------------------------------------------------------------------- |
| React project shell  | Modal state, explicit connecting/failure feedback, and read-only lifecycle polling             |
| `ProjectManager`     | Active project metadata and create/open/reopen/close serialization                             |
| `EngineManager`      | Exactly one process, handshake, standby, session reopen, health detection, and shutdown        |
| `EngineProcess`      | Windowless Windows creation flags, stdio protocol, stderr capture, child termination           |
| `QueryCoordinator`   | Durable in-process execution registry and latest execution lookup per project/tab              |
| `useQueryExecution`  | Startup/restoration UI, status polling, and lightweight active-tab rendering state             |
| Shutdown coordinator | Query/export cancellation, result release, session close, metadata checkpoint, engine shutdown |

## Browser dialog inventory

E13 initially replaced only the New project prompt. E13.5 subsequently replaced the remaining five `window.prompt` and twelve `window.confirm` calls with feature-owned accessible Tarik dialogs. A repository guard now rejects production browser dialogs while retaining native operating-system file/folder pickers. See `docs/review/E13-5-INTERACTIONS-RESOURCES.md`.

## Results regression

The initial empty state was mutually exclusive inside `ResultPanel`, but execution ownership was only in React memory and no state existed while `execute_query` was pending. A workspace recreation could therefore lose the active tab result, and slow startup could leave `No results yet` visible after Run was clicked.

The coordinator now returns the latest execution for a project/tab, including its immutable SQL snapshot. The hook restores it when an editor tab appears without local execution state and renders explicit Starting or Restoring states before the execution is available. The valid first-use empty state remains unchanged.

## Windows visibility contract

Windows process creation uses `CREATE_NO_WINDOW` while keeping stdin, stdout, and stderr piped. The sidecar remains visible in Task Manager for observability but must have no main window, taskbar entry, Alt+Tab entry, tray icon, or console flash. The Windows runtime verifier now rejects a sidecar during no-project desktop launch and rejects any sidecar main-window handle during packaged engine workload testing.

## Branding contract

`TarikLogo-transparent.png` remains the supplied canonical artwork. `TarikLogo-square.png` is the normalized 1024px app-icon master: transparent outer corners, a white rounded plate for dark-surface legibility, and undistorted centered artwork. Tauri-generated PNG, ICO, ICNS, Windows square/store, Android, and iOS outputs derive from that master. `verify:icons` runs before Windows packaging.
