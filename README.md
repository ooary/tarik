# Tarik

A local-first desktop SQL workbench for beginner data engineers.

## Current status

The repository is in the E0 foundation phase. The authoritative delivery tracker is [`TASK.md`](./TASK.md).

## Telegram agent status polling

A separate Telegram agent can periodically read the current development status over SSH:

```bash
ssh <host> /home/ooary/Projects/Tarik/.tarik-agent/read-status
```

The command writes nothing, returns one JSON document, and only reads the repository-owned `.tarik-agent/status.json`. It refuses to follow a symlinked status file.

Status changes are represented by `state` values such as `working`, `completed`, `blocked`, `waiting_for_user_review`, and `unknown`. Polling agents should notify only when the state, task, commit, or `needsUserReview` value changes.

The status file is operational state and is intentionally ignored by Git. Do not place secrets, full SQL results, or full logs in it.

## Development

```bash
npm install

# Build the DuckDB engine sidecar first. It links the official prebuilt
# libduckdb (DUCKDB_DOWNLOAD_LIB=1 in .cargo/config.toml), so no bundled
# C++ compilation happens.
cargo build -p tarik-engine-duckdb   # or: ./scripts/build-engine.sh (also places libduckdb beside the binary)

npm run dev
npm run tauri dev

# If an older development process still owns port 1420:
npm run tauri:dev:clean
```

The desktop app does not compile DuckDB or Arrow. All DuckDB work runs in the `tarik-engine-duckdb` sidecar process; build it before `tauri dev`.

See `TASK.md` for the full architecture, task dependencies, manual EPIC review gates, and required atomic Git workflow.
