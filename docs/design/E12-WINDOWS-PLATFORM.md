# E12 Windows platform and filesystem design graph

PROBLEM: Make Tarik development, project/source paths, and export/recovery lifecycles work natively on Windows without weakening Linux ownership or atomicity guarantees.

X → DesignGraph<A, E, R>
│ │ │ │ │
│ │ │ │ └─ R: Node 24, Rust PathBuf, Tauri path resolver, DuckDB sidecar, native filesystem
│ │ │ └──── E: locked file, read-only path, invalid Unicode, long path, UNC unavailability, occupied dev port
│ │ └─────── A: NativePath, DevProcess, EngineArtifact, ProjectMutation, ExportPublication
│ │
│ └─ nodes = functions, edges = path/process/resource flow
│
└─ the problem: native Windows development and local-file behavior

SHAPES: NativePath(PathBuf), WirePath(UTF-8 string), DevProcess(pid, executable, commandLine), EngineArtifact(executable, runtimeLibrary), ProjectMutation(rename|remove), ExportPublication(stage, backup, final), FileFailure(notFound|permissionDenied|locked|collision), Platform(windows|linux|macOS)

GRAPH:

```text
resolve_dev_processes (1)
│  R: /proc on Linux | CIM/PowerShell on Windows | ps on macOS
│  E: enumeration unavailable ↯escape(clear error; kill nothing)
│  └─ 🔒 OS process records → project-scoped DevProcess[]
├─→ stop_project_processes (N)
│   R: exact normalized project root, PID, native kill API
│   E: access denied ↯escape(report PID; continue bounded wait)
└─→ wait_for_port_1420 (T)
    R: Node TCP socket + deadline
    E: still occupied ↯escape(non-zero exit; never start a second dev server)

build_engine (1)
│  R: Cargo, target triple, pinned libduckdb download
│  E: build/download/link failure ↯escape(non-zero direct exit)
├─→ find_runtime_library (1)
│   R: target/duckdb-download/<triple>/1.5.5
│   E: missing DLL/SO ↯escape(clear error)
├─→ stage_sibling_runtime (1)
│   R: native copyFile, profile target directory
│   E: locked destination ⟳retry×3 → ↯escape(clear error)
└─→ handshake_engine (1)
    R: sibling duckdb.dll/libduckdb.so, newline protocol
    E: spawn/load/protocol failure ↯escape(stderr + non-zero exit)
    └─ 🔒 engine JSON → EngineInfo

choose_native_file (1)
│  R: Tauri native dialog
│  E: cancel ↯escape(None)
└─→ PathBuf → wire_path (1)
    R: UTF-8 boundary required by JSON/DuckDB API
    E: unpaired native path ↯escape(InvalidPath; no mutation)
    └─ 🔒 dialog string → NativePath → WirePath

managed_project_mutation (1)
│  R: exact projects root, metadata ownership, engine session
│  E: active session close failure ↯escape(no rename/delete)
├─→ validate_direct_managed_child (1)
│   E: path outside root ☠die(request rejected)
├─→ rename_or_stage (1)
│   E: collision|permission|Windows lock ↯escape(ProjectError; metadata unchanged)
├─→ metadata_commit (1)
│   E: SQLite failure ↯escape(rename rollback)
└─→ remove_staged_tree (1)
    E: Windows lock|read-only ↯escape(DeleteStaged; staged ownership remains explicit)

publish_export_part (N)
│  R: canonical output directory, same-directory hidden stage, immutable options
│  E: read-only|locked|collision ↯escape(typed failure; final preserved)
├─ fail_if_exists → hard_link_stage_to_final (1)
└─ replace → final_to_backup → stage_to_final → remove_backup (1)
    E: second rename failure ↯escape(restore backup; stage cleanup by Drop)
```

CARDINALITY: resolve_dev_processes (1) · stop_project_processes (N) · wait_for_port_1420 (T) · build_engine (1) · find_runtime_library (1) · stage_sibling_runtime (1) · handshake_engine (1) · choose_native_file (1) · wire_path (1) · managed_project_mutation (1) · validate_direct_managed_child (1) · rename_or_stage (1) · metadata_commit (1) · remove_staged_tree (1) · publish_export_part (N)

BOUNDARIES: OS process listings are untrusted and must match the normalized repository root plus an exact Tarik dev role before termination; dialog/IPC strings become native paths at the Rust command boundary; paths become DuckDB SQL only through JSON transport and escaped SQL literals; persisted project/source/export paths retain their selected absolute spelling; output options become trusted only after absolute/existing/directory canonicalization and portable basename validation.

BEHAVIOR: ⛈ bounded retry wraps only runtime-library staging where antivirus/indexers may transiently hold a DLL · ⛈ structured errors wrap filesystem operations without changing happy-path ownership · ⛈ CI supplies drive-letter, spaces, Unicode, long-path, read-only, and locked-file cases · ⛈ Linux regression wraps every Windows boundary change.

SCOPE: Dev process records acquire@enumerate → discard@stop/wait · child engine acquire@handshake → kill/await@finally · DuckDB session acquire@project open → close@project close/rename/remove/shutdown · export stage acquire@PartWriter::create → publish or remove@Drop · export backup acquire@replace → remove or restore@publish join · temporary path fixtures acquire@test → remove@test RAII/best effort.

TEST LAYERS: Pure Node tests inject Windows/Linux process records and path environments; Rust filesystem tests use temp roots with spaces, Unicode, >260-character paths, and read-only files; native Windows tests use `OpenOptionsExt::share_mode(0)` to prove lock failures preserve originals/metadata; sidecar tests use sibling DLL/SO; CI substitutes native `windows-latest` and Linux runners without changing the graph.

VERDICT: The existing production graph already uses `PathBuf`, Tauri-owned app directories, same-directory export staging, exact managed-child checks, and session close before managed mutation. Mismatches are Bash-only dev/reset/build/check entry points, unconditional Unix test environment, missing Windows executable suffixes (fixed in E12-T1), and missing native Windows lock/path evidence. T2 is valid only when Node-native commands replace those entry points and both native Windows CI and full Linux gates preserve the graph.
