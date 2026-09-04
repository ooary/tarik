# E11 memory and bounded-resource evidence

Tarik runs DuckDB and Arrow in the `tarik-engine-duckdb` sidecar. Memory evidence therefore records the sidecar separately from the Tauri/WebKit desktop. This harness is a regression test for a fixed workload, not a universal memory claim for arbitrary SQL.

## Run

```bash
cd /home/ooary/Projects/Tarik
CARGO_BUILD_PROFILE=release ./scripts/build-engine.sh
./scripts/benchmark-memory.py --record
```

The command writes:

- ephemeral full report: `target/e11/memory-report.json`;
- checked evidence: `docs/performance/E11-LATEST-MEMORY-REPORT.json` with `--record`;
- budgets: `docs/performance/E11-MEMORY-BUDGETS.json`.

Linux `/proc` is required. Each report records the exact git revision, OS, CPU, logical CPU count, total memory, Rust, Node, Python, DuckDB sidecar metadata, dataset dimensions, production bounded defaults, all RSS/disk samples, scenario summaries, and budget verdict.

## Fixed scenarios

1. **Idle sidecar after session open** — baseline RSS/HWM.
2. **Import** — inspect and import a deterministic 50,000-row CSV.
3. **Large result** — execute 100,000 rows with a 128-byte payload, read first/last bounded pages, and release the result.
4. **Repeated query** — twelve 20,000-row run/read/release cycles; measure RSS before/after and require zero result cache bytes after release.
5. **Export cancellation** — stream a large 512-byte payload CSV export, cancel it, and require a cancelled terminal state with zero hidden stages.

Reports contain aggregate counts and sizes only. They do not persist query result rows or full workload SQL.

## Budgets

- Fixed-workload peak sidecar RSS: **512 MiB** maximum.
- Retained RSS growth after twelve release cycles: **64 MiB** maximum.
- Result cache bytes after explicit release: **0**.
- Hidden export stages after cancellation: **0**.

These ceilings are deliberately above the recorded baseline to tolerate allocator and DuckDB variation across Linux machines while still detecting unbounded regressions. Tighten them only after repeated release-build measurements on representative hardware; loosen them only with a recorded report and explanation.

## Recorded 2026-09-04 baseline

Machine: 12th Gen Intel Core i5-1235U, 12 logical CPUs, 15.3 GiB RAM, Arch Linux 7.1.8, Rust 1.91.0, Python 3.14.0, DuckDB sidecar protocol 1.

| Run            | Idle sidecar RSS | Peak sidecar RSS | Post-cycle growth | Result cache after release | Hidden stages after cancel |
| -------------- | ---------------: | ---------------: | ----------------: | -------------------------: | -------------------------: |
| Checked report |         40.5 MiB |        106.7 MiB |           3.5 MiB |                        0 B |                          0 |
| Repeat run     |                — |        117.4 MiB |           7.5 MiB |                        0 B |                          0 |

Both release-profile runs passed. The difference is normal allocator/OS cache variation and remains far below the ceilings.

## Bounded production defaults

| Resource                           |                   Default | Ownership       |
| ---------------------------------- | ------------------------: | --------------- |
| Engine result rows per normal page |                       500 | sidecar         |
| Engine page byte target            |                     4 MiB | sidecar         |
| Maximum decoded pages              |                  12 total | desktop         |
| Query workers                      | 1 FIFO worker per session | sidecar         |
| Export workers                     | 1 FIFO worker per session | sidecar         |
| Parquet row-group byte target      |                     4 MiB | sidecar         |
| Startup result-cache budget        |                   512 MiB | desktop cleanup |
| Startup result-cache age           |                  24 hours | desktop cleanup |

The fixed measurements support retaining these defaults for the MVP. No automatic `COUNT(*)`, full-result IPC, or full export buffering is introduced.

## Desktop/UI observation

A release Tauri process includes WebKit subprocesses whose RSS varies by desktop environment and GPU backend. For manual release evidence:

```bash
npm run tauri build -- --no-bundle
/usr/bin/time -v target/release/tarik
```

Record the parent and WebKit process tree with `ps --forest -o pid,ppid,rss,cmd -C tarik -C WebKitWebProcess` during idle and result browsing. Do not combine WebKit RSS with sidecar RSS into one unexplained number.
