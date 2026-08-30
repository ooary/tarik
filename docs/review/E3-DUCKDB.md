# E3 DuckDB lifecycle review

## Engine decision

Tarik pins the maintained `duckdb` Rust binding `1.10505.0` with its bundled engine. The desktop build does not require a separately installed DuckDB library.

## Active project lifecycle

- Tarik starts with no analytical project open.
- New project creates a managed directory and `project.duckdb` below Tarik app data.
- Open accepts an existing local `.duckdb` file path.
- Exactly one project may be active.
- Close shuts down the connection-owning worker before clearing active state.
- Reopening the same path preserves its SQLite operational project identity.
- A failed managed-project creation removes its incomplete directory.

## Worker boundary

- One dedicated `tarik-duckdb-worker` thread owns the active connection.
- A bounded channel accepts at most 16 waiting jobs.
- Queue saturation is a structured error instead of unbounded memory growth.
- Tauri commands dispatch blocking waits through `tauri::async_runtime::spawn_blocking`.
- Shutdown sends a structural stop message and joins the worker thread.

## Resource profiles

| Profile    | Memory | Threads |
| ---------- | -----: | ------: |
| Low memory | 512 MB |       1 |
| Balanced   |   2 GB |       2 |
| Fast       |   8 GB |       4 |

Custom values validate memory, threads, and temporary directory before application. DuckDB settings use parameter binding instead of interpolating user paths or values into SQL.

## Catalog

The live catalog returns:

- Database
- Schema
- Table or view name
- Object kind
- Column name
- DuckDB data type
- Column position
- Nullable state

The shell source explorer now uses this live catalog. A new empty project truthfully shows no tables or views.

## Manual review

```bash
npm run tauri:dev:clean
```

1. Confirm Tarik opens with `No project open` and `DuckDB idle`.
2. Select **New project**, enter a name, and confirm the header changes to `DuckDB ready`.
3. Confirm the explorer shows the project and an empty catalog.
4. Close the project and confirm the header returns to idle.
5. Use **Open** and provide an absolute path to an existing `.duckdb` file.
6. Confirm its real tables and views appear.
7. Try opening another project while one is active through the command boundary. It must return `a DuckDB project is already open`.

SQL editing and query execution are intentionally not part of E3. They arrive in E5 and E6.
