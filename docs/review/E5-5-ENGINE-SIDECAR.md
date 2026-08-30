# E5.5 Engine sidecar review

## Outcome

The Tarik desktop app no longer depends on DuckDB or Arrow crates. All DuckDB work happens in a long-lived `tarik-engine-duckdb` sidecar process linked to the official prebuilt `libduckdb` (no local C++ compilation). The desktop and the engine speak a versioned JSON protocol over stdio.

## Workspace layout

```text
Cargo.toml                    # workspace; default-members = desktop + protocol/client
crates/engine-protocol/       # versioned request/response, capabilities, errors
crates/arrow-page-format/     # bounded result page model (T2)
crates/engine-client/         # EngineProcess spawn + framing + typed requests
engines/duckdb/               # DuckDB adapter binary (built separately)
src-tauri/                    # desktop: no duckdb/arrow dependencies
```

## Protocol

- `engine.handshake` - engine identity, protocol version, capabilities
- `engine.ping` / `engine.shutdown`
- `session.open` / `session.close` (duckdb locator carries the project path)
- `catalog.inspect`
- `source.inspect`
- `duckdb.source.link_parquet` / `import_table` / `repair_link` / `drop_link` / `check_health`

Requests and responses are single-line JSON. Unknown methods and missing sessions return structured error envelopes with stable codes.

## Build-time separation (measured)

| Measurement                                      | Value                                     |
| ------------------------------------------------ | ----------------------------------------- |
| Desktop dependency graph                         | no `duckdb`, `arrow`, or `parquet` crates |
| Warm desktop `cargo check -p tarik`              | ~0.2s                                     |
| Warm engine `cargo build -p tarik-engine-duckdb` | ~0.1s                                     |
| DuckDB C++ compilation                           | none (prebuilt dynamic library)           |
| Engine binary                                    | ~7 MB                                     |
| Prebuilt `libduckdb.so`                          | ~70 MB (downloaded, not compiled)         |

One-time migration costs are expected when the workspace root/profile changes because Cargo re-fingerprints artifacts.

## Build and run

```bash
# Build the engine adapter (downloads official prebuilt libduckdb once)
cargo build -p tarik-engine-duckdb

# Run the desktop; it spawns the engine from target/debug by default
npm run tauri dev
```

The desktop resolves the engine binary as a sibling of the current executable, falling back to `target/debug/tarik-engine-duckdb`.

## Runtime library discovery

`libduckdb.so` is found at build/link time via `DUCKDB_DOWNLOAD_LIB=1` (pinned in `.cargo/config.toml`). At runtime the engine binary needs `libduckdb.so` on the loader path. Development can set `LD_LIBRARY_PATH` to the downloaded lib directory; the E12 portable package must ship `libduckdb.so` next to `tarik-engine-duckdb` with a deterministic loader path.

## Manual verification

1. `cargo build -p tarik-engine-duckdb`
2. `npm run tauri dev`
3. Create or open a DuckDB project. Confirm the header shows `DuckDB ready` (this requires the engine to spawn and handshake; if the engine binary is missing, Tarik returns a clear structured error instead of hanging).
4. Explorer catalog shows real tables/views and column/row metadata from the sidecar.
5. Import a CSV; the resulting table appears in the catalog after refresh.
6. Link a Parquet file; the view appears in the catalog.
7. Close the project; the engine session closes. Quit Tarik; the engine process stops.

## Remaining E5.5 follow-ups

- `duckdb.session.configure` for resource profiles (memory/threads) in a later task.
- Wire `engine.plan.explain`/`query.execute` once E6 starts; `crates/arrow-page-format` is ready for bounded result pages.
- Deterministic runtime library discovery and packaging for Windows portable (E12).
- Engine crash detection/restart policy.
