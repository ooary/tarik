# Tarik for Windows — portable edition

Tarik is a local-first DuckDB SQL workbench. This package contains the desktop application and its complete DuckDB engine; it does not download or require a separate DuckDB installation.

## Start Tarik

1. Extract the entire ZIP to a normal writable folder, such as `Documents\Tarik`.
2. Keep `Tarik.exe`, `tarik-mcp.exe`, `tarik-engine-duckdb.exe`, and `duckdb.dll` together in that folder.
3. Double-click `Tarik.exe`.

Do not launch Tarik from inside the ZIP preview. No installer, administrator access, Node.js, Rust, or terminal command is required.

Tarik requires Windows 10 or Windows 11 x64 and Microsoft Edge WebView2 Runtime. Supported Windows installations normally include WebView2. If Windows reports a missing runtime, install the Evergreen WebView2 Runtime from Microsoft and launch `Tarik.exe` again.

## Offline and local data

Tarik does not download DuckDB and does not require an account. The bundled `tarik-engine-duckdb.exe` starts in the background when you create or open a project. MCP hosts start `tarik-mcp.exe` as a stdio child only when configured; Tarik must already be running with Agent Access enabled.

Tarik operational data is stored under your Windows application-data directories. DuckDB projects, CSV/Parquet sources, and completed exports remain in locations you choose. Deleting the extracted application folder does not automatically delete those files or your AppData metadata.

## Package integrity

`SHA256SUMS` inside the extracted folder covers every packaged file except itself. The release download also has an outer checksum next to the ZIP. This candidate is unsigned unless its release manifest says otherwise; Windows SmartScreen may therefore show an unknown-publisher warning.

See `COMPATIBILITY.md` before upgrades, downgrades, backups, or removal. Preserve `LICENSE`, `THIRD_PARTY_NOTICES.md`, and the generated dependency inventories when redistributing this folder.
