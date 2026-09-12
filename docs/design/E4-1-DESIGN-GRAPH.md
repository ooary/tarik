# E4.1 large-source import lifecycle design graph

PROBLEM: Make large local CSV and Parquet table imports responsive, cancellable, observable, and single-pass without weakening transactional ownership or bounded state.

X → DesignGraph<A, E, R>
│ │ │ │ │
│ │ │ │ └─ R: DuckDB sidecar, import registry, resource snapshot, active project, metadata repository
│ │ │ └──── E: source drift, collision, disk full, DuckDB failure, cancellation, process loss, metadata failure
│ │ └─────── A: ImportRequest → ImportStatus → SourceRecord
│ │
│ └─ nodes = functions, edges = import and status flow
│
└─ the problem: bounded large-source import lifecycle

SHAPES: ImportId, ImportRequest(project, path, expected bytes, options), ImportStage(queued|validating|reading_and_writing|finalizing), ImportState(queued|running|succeeded|failed|cancelled|recovery_required), ImportStatus, EffectiveEngineResources, SourceRecord, ImportError

GRAPH:

```text
start_import (1)
│  R: active project, validated options, ImportRegistry
│  E: busy|invalid source|table collision ↯escape(typed rejection; no work started)
│  └─ 🔒 desktop path/options/inspection bytes → immutable ImportRequest
│
├─→ register_import (1)
│   R: bounded registry, one active import per session/project
│   E: duplicate|capacity ↯escape(no work started)
│
├─→ claim_worker (1)
│   R: cloned session connection, effective resource snapshot
│   E: connection|worker failure ↯escape(failed status)
│
├─→ revalidate_source (1)
│   R: native file metadata, format detector
│   E: missing|changed|unsupported ↯escape(no table created)
│
├─→ build_single_pass_projection (1)
│   R: validated identifiers and closed DuckDB type allow-list
│   E: duplicate|unknown override|invalid type ↯escape(no table created)
│
├─→ execute_atomic_ctas (1)
│   R: DuckDB transaction, interrupt handle
│   E: parse|cast|disk|lock|interrupt ↯escape(rollback → failed|cancelled)
│   └─ CREATE TABLE AS SELECT * REPLACE(CAST(...)) FROM source
│
│   ├─→ observe_status (T)
│   │   R: bounded poll interval, monotonic clock
│   │   E: exact percentage unavailable ↯escape(truthful stage + elapsed time; no fabricated percentage)
│   │
│   └─→ cancel_import (1)
│       R: ImportId, DuckDB InterruptHandle
│       E: already terminal ↯escape(return terminal status)
│       └─ interrupt → rollback → cancelled
│
├─→ finalize_import (1)
│   R: transaction, exact table cardinality, source metadata
│   E: count|commit|rollback ambiguity ↯escape(failed|recovery_required)
│
└─→ persist_source_record (1)
    R: MetadataDb, bounded desktop coordinator
    E: persistence|lost terminal result ↯escape(recovery_required)
```

CARDINALITY: start_import (1) · register_import (1) · claim_worker (1) · revalidate_source (1) · build_single_pass_projection (1) · execute_atomic_ctas (1) · observe_status (T) · cancel_import (1) · finalize_import (1) · persist_source_record (1)

BOUNDARIES: Native picker strings and frontend options become an immutable validated import request at the desktop command; the sidecar revalidates supported format and source byte identity immediately before work; closed type choices become quoted single-pass projection SQL; DuckDB status and effective-resource readback become typed protocol data; no path, SQL, or row values enter diagnostic progress events.

BEHAVIOR: ⛈ bounded 250–500 ms polling wraps status observation · ⛈ stage/elapsed/resource feedback wraps execution without claiming an unavailable exact percentage · ⛈ one active import per project/session prevents conflicting catalog mutation · ⛈ terminal registries are bounded · ⛈ logs record identifiers, stage, duration, and outcome but not row values.

SCOPE: Import connection acquire@claim_worker → release@terminal · interrupt handle acquire@running → release@terminal · DuckDB transaction acquire@execute_atomic_ctas → commit or rollback@finalize/error/cancel · desktop poller acquire@accepted submission → release@terminal · UI poll timer acquire@active dialog → release@terminal/unmount · terminal records acquire@completion → prune@bounded registry.

TEST LAYERS: R = {DuckDB: temporary CSV/Parquet projects and deterministic slow generated scans, EngineImporter: scripted status engine, MetadataDb: in-memory SQLite, Clock/Poll: short test interval}; the graph remains unchanged. Native Windows evidence records effective resources, elapsed time, cancellation rollback, CPU/disk behavior, and a representative large local Parquet import.

VERDICT: The pre-E4.1 implementation does not match this graph: table import is one synchronous protocol request, status is absent, the visible cancel command always returns false, import repeats source inspection, and each type override can trigger a separate full-table rewrite. E4.1 must replace the product path with the bounded asynchronous lifecycle above, retain transactional rollback, apply overrides in the original CTAS projection, and show indeterminate truthful progress when DuckDB cannot expose a reliable percentage. Manual native Windows large-file performance acceptance remains a user-owned review gate.
