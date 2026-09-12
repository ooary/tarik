# E15.1 assisted MCP connection and guarded export review

## Verdict and evidence boundary

**Status:** Linux real-Tauri Agent Access and custom-destination workflow accepted September 12, 2026; native Windows and final cross-platform sign-off remain required. Do not mark E15.1 complete from this packet alone.

This packet consolidates the E15.1 connection assistant, portable workflow guidance, private reusable export destinations, and guarded complete-query CSV/Parquet exports. Automated Linux evidence is recorded below. Native Windows host/configuration/filesystem/runtime evidence and real-Tauri visual/accessibility approval were explicitly postponed and are not claimed.

The local artifacts described here are **historical review artifacts**, not publication artifacts. They were built before the accepted Agent Access UI was committed, so their manifest correctly records `sourceDirty: true`. Final publication requires a clean tagged tree and regenerated artifacts with `sourceDirty: false`.

E15 remains independently in review. E12/E13 Windows, E6/E7/E10, clean-machine, signing, and final release gates are not inherited or closed by E15.1.

## Source and compatibility identity

| Item                                 | Reviewed candidate         |
| ------------------------------------ | -------------------------- |
| Application version                  | `0.1.0`                    |
| E15.1 task/design                    | `5f39af3`, `6c4c799`       |
| Connection setup engine              | `dc544dd`                  |
| Portable MCP guidance                | `ec2bc61`                  |
| Delegated destinations               | `d3862f5`                  |
| Guarded chunked exports              | `8b9fce6`                  |
| Custom external-project-folder fix   | `c0e1cf0`                  |
| Accepted Linux Agent Access UI       | `389345b`                  |
| Metadata schema                      | 11                         |
| Private desktop ↔ `tarik-mcp` bridge | 3                          |
| Desktop ↔ DuckDB engine protocol     | 2                          |
| DuckDB runtime                       | 1.5.5                      |
| MCP handshake revisions              | `2025-06-18`, `2025-11-25` |
| Public MCP tools                     | 25 exact allowlisted tools |

Bridge protocol 3 is intentionally incompatible with an older `tarik-mcp`. Engine protocol 2 is intentionally incompatible with an older sidecar that could ignore the security-relevant delegated byte budget. Packaged binaries must remain from the same build.

## Implemented workflow

### Connection assistant

- Detects reviewed Claude Desktop, Claude Code, and Codex candidates; unsupported hosts and versions fall back to guided setup.
- Invokes Claude Code/Codex official CLIs directly with structured argument arrays, bounded output/timeouts, and no shell.
- Manages Claude Desktop JSON only through ownership/type/reparse checks, parse-before-write, conflict refusal, backup, same-directory atomic replacement, verification, exact removal, and rollback.
- Stores bounded private receipts and supports moved-portable repair only for Tarik-owned exact entries.
- Never installs, updates, authenticates, or starts a third-party host; never pairs a client or grants a project.
- Shows setup/review/repair/remove details in a dedicated nested modal; errors stay modal-scoped and successful apply returns focus.

### Portable guidance

- Publishes concise MCP server instructions and seven bounded static prompts.
- Packages `agent-skills/tarik-mcp/SKILL.md` in Linux and Windows layouts.
- Supports automatic install/remove only for Tarik's exact Pi skill in the verified current-user location.
- Treats rows, names, SQL, errors, profile values, and all tool output as untrusted content.
- Guidance cannot pair, grant, approve, select/reveal a path, or bypass backend policy.

### Destination grants

- Selects paths only through visible Tarik native folder picking.
- Stores canonical path and directory identity privately in schema 11; the path-bearing record has no serialization surface.
- Binds each opaque destination ID to one paired client, one active project, and Analyze authority; caps at eight per client/project.
- Allows only CSV/Parquet, 1–1,000,000 rows per part, and 1 byte–100 GiB total quota.
- Rejects roots, files, symlinks/reparse ancestors, non-user ownership, remote/network filesystem classes, Tarik data/cache/log overlap, and managed-project storage overlap. A custom folder may also contain an externally opened DuckDB file; typed output names and collision controls still govern publication.
- Separates readiness from enabled state and revalidates folder identity before use.
- Deletes grants when Analyze/project authority is removed or the client is revoked.
- MCP can list only ID, label, formats, quota, create-new-only, enabled/readiness, and revision. It has no destination mutation/path tool.

### Complete guarded exports

`tarik_propose_export` accepts only:

- one-use server-held SafeRead `snapshotId`;
- opaque `destinationId`;
- closed `csv | parquet` format;
- 1–64 byte portable ASCII base name;
- 1–1,000,000 rows per part;
- matching closed CSV delimiter/header or Parquet compression options.

It accepts no SQL, path, URL, S3/network target, raw `COPY`, overwrite flag, dynamic option string, or approval decision.

Before execution Tarik revalidates authentication, Analyze, active project, exact SafeRead classification, catalog/source/function revision, destination owner/revision/identity/readiness, format/chunk policy, canonical collisions, and byte quota.

| Decision                                            | Behavior                                                          |
| --------------------------------------------------- | ----------------------------------------------------------------- |
| Within destination policy, no canonical collision   | `delegated`; create-new execution starts without per-run approval |
| Format or rows-per-part policy exception            | `approval_required`; visible one-use Tarik approval               |
| Existing canonical part family                      | `critical_confirmation`; fresh Tarik-generated typed phrase       |
| Missing/replayed/foreign/stale/unsafe/unknown input | blocked before query/publication                                  |

`tarik_export_status`, `tarik_export_cancel`, and `tarik_export_release` are bound to the authenticated connection. Status returns complete-query truth, exact rows/files/bytes, current part, bounded relative filenames, and stable path/SQL/value-free errors. Release removes transient ownership only; completed user files and persisted aggregate history remain.

The existing E9 one-pass Arrow writer performs the complete query independently of MCP's 5,000-row browse cap. It enforces the delegated byte budget on the hidden stage and authoritatively after close but before publication. The part that crosses quota is never published; earlier complete parts remain truthful. Zero successful rows create zero files. Publication rollback ambiguity is `recovery_required`, retains the startup recovery manifest, and is never reported as success.

One active agent export is allowed per connection and four globally. Agent and shared coordinator terminal registries are bounded at 256. Status manifests are capped at 256 KiB and part summaries at the existing 100-part bound. Disconnect, destination change/revoke, Analyze/grant/client removal, project close/rename/remove, Agent Access disable, and shutdown cancel applicable work.

## Public MCP allowlist delta

E15.1 adds exactly five tools to the original E15 baseline:

1. `tarik_list_export_destinations`
2. `tarik_propose_export`
3. `tarik_export_status`
4. `tarik_export_cancel`
5. `tarik_export_release`

There is still no MCP method to create/edit/repair/revoke a destination, inspect a path, approve an action, provide a critical phrase, or run arbitrary SQL.

## Automated evidence

Final automated run against the local E15.1 candidate:

- `cargo test --workspace --all-targets --no-fail-fast`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- Vitest: 31 files / 226 tests passed.
- Node command/platform/package/runtime contracts: 39 tests passed.
- TypeScript typecheck and production Vite build: passed.
- ESLint: no errors; only the two pre-existing TanStack Virtual and Fast Refresh warnings.
- Rustfmt, Prettier, documentation checks (44 Markdown files), engine check, and `git diff --check`: passed.
- Packaged sidecar handshake: DuckDB engine protocol 2.
- Linux portable tarball, DEB, and AppImage build/content/startup/checksum checks: passed.

Focused real-sidecar/security evidence includes:

- one-use closed export schema rejects path, SQL, overwrite, cross-format options, unsafe names, and excessive rows per part;
- 6,001-row CSV export proves complete-query execution beyond the 5,000-row browse cap;
- typed Zstandard Parquet export creates exact `[2, 2, 1]` row parts with valid Parquet magic;
- zero rows return zero rows/files/bytes and create no file;
- byte quota rejects before first publication and preserves only completed earlier parts;
- canonical collision routes to critical typed approval and preserves the old file until approval;
- format/chunk exception routes to ordinary visible approval; denial runs no query and creates no file;
- snapshot replay, foreign connection, foreign destination, destination revision/content drift, and active release fail closed;
- cancellation, disconnect, destination/grant/client/project invalidation, disable, and shutdown use the existing interrupt/cleanup path;
- returned manifests contain only exact relative generated filenames;
- bounded audit stores client/project/tool/risk/snapshot hash/destination ID/counters/outcome without SQL, values, path, or phrase;
- SQL execution cannot consume an export approval ID;
- engine protocol 2 prevents a v1 sidecar from silently ignoring delegated quota.

## Linux package evidence

Expected portable contents:

```text
agent-skills/tarik-mcp/SKILL.md
COMPATIBILITY.md
libduckdb.so
LICENSE
README.md
tarik
tarik-engine-duckdb
tarik-mcp
THIRD_PARTY_NOTICES.md
THIRD-PARTY-NPM.txt
THIRD-PARTY-RUST.txt
```

Local review artifact hashes from the final automated package pass:

```text
0b2ee1052cc6fccc38bee3a78bf28f8dc9f15d173077963f8378128da06a064a  Tarik_0.1.0_amd64.AppImage
a95f4a0b0198e3406a26b97e8c824b4cbfe58948f1073fd9f6a02788f38cadb0  Tarik_0.1.0_amd64.deb
4b1fdb8409479327439a5a764d9e74983453c350e93d66df153c2f8e77d3f24e  Tarik-0.1.0-linux-x86_64.tar.gz
```

These hashes are local review evidence only. Regenerate after the UI is approved/committed. The current review manifest records Git revision `8b9fce65e9edbc7831a93afa629f9761aded2005`, schema 11, engine protocol 2, `signed: false`, and `sourceDirty: true`.

## Consolidated manual workflow

Use a disposable project and destination. Start from freshly built/package-matched binaries.

### A. Detect, review, and configure

- [ ] Open **Agent access → Connect an agent**.
- [ ] Confirm installed/not-configured/unsupported/conflict/repair states are truthful for each present host.
- [ ] Open **Show steps**, **Review setup**, **Review repair**, and **Remove**. Confirm each uses a dedicated nested modal, keeps errors inside it, and returns focus.
- [ ] Review exact command/config target and security note before apply.
- [ ] Configure Claude Desktop, Claude Code, and Codex only through their intended adapter; confirm no host starts automatically.
- [ ] Confirm unrelated Claude Desktop JSON survives semantic comparison and exact remove preserves it.
- [ ] Move a portable folder and review explicit receipt-owned repair.
- [ ] Confirm Pi/Cursor/VS Code/generic and unsupported forms remain guided and make no hidden write.

### B. Guidance

- [ ] List and open all seven MCP prompts.
- [ ] Inspect the packaged Agent Skill and optional Pi install/remove review.
- [ ] Confirm install/remove changes no pairing, project grant, destination, or approval.
- [ ] Place hostile instructions in project names, table values, and errors; confirm they cannot alter the workflow or authority.

### C. Pair and grant

- [ ] Restart the configured host and observe one visible pending pairing in Tarik.
- [ ] Deny once; reconnect; then pair.
- [ ] Confirm the new client has zero project grants.
- [ ] Grant Inspect + Analyze only to the active disposable project.
- [ ] Confirm only that project is listed and no project/source/destination path appears.

### D. Bounded browse versus complete export

- [ ] Classify and run a query returning more than 5,000 rows.
- [ ] Confirm browse status is capped/inexact, page what is needed, and release the result.
- [ ] Classify the exact query again for export; do not reuse the consumed browse snapshot.
- [ ] Confirm the export response says `completeQuery: true` and its row count exceeds 5,000 when the source does.

### E. Destination policy

- [ ] Under the paired client and active project, open **Export destinations**.
- [ ] Cancel native folder selection; confirm no grant appears.
- [ ] Create a destination with a non-sensitive label, CSV/Parquet formats, rows-per-part, and byte quota.
- [ ] Confirm no absolute path appears in MCP listing, UI rows, logs, audit, or copied status.
- [ ] Disable and re-enable. Move/replace the folder and confirm **Repair required** before choosing a replacement directly in Tarik.
- [ ] Try root, file, symlink/reparse, non-owned, remote/network, Tarik data/cache/log, managed-project storage, and excessive-entry paths; confirm fail-closed behavior. Confirm a custom folder containing an external DuckDB project remains allowed.

### F. Delegated CSV and Parquet

- [ ] Start an in-policy create-new CSV export; confirm no per-run approval appears.
- [ ] Verify exact part rows, headers/delimiter, complete aggregate counters, relative filenames, and no hidden stages.
- [ ] Repeat with Parquet and each reviewed compression mode.
- [ ] Export a successful zero-row query; confirm no file is created.
- [ ] Release terminal ownership; confirm completed files and persisted aggregate history remain.

### G. Approval and critical replacement

- [ ] Request a format or rows-per-part exception; confirm **Export policy exception**, exact immutable SQL, redacted destination/file family, snapshot hash, countdown, Deny, and Approve once.
- [ ] Deny once; confirm no query/file. Approve a fresh request; confirm one execution.
- [ ] Pre-create a canonical part; confirm **Critical export replacement** and that the old file remains unchanged before approval.
- [ ] Confirm paste is blocked and approval remains disabled until Tarik's exact generated phrase is typed.
- [ ] Deny one critical request. Approve a fresh one and verify replacement/recovery behavior.
- [ ] Confirm MCP-host confirmation cannot approve and no approval method exists.

### H. Cancellation, revocation, and recovery

- [ ] Cancel while queued, during the first part, and after a later completed part; poll to terminal and verify truthful partial counters/stage cleanup.
- [ ] During active/awaiting work, test destination edit/disable/revoke, Analyze removal, grant removal, client revoke, project close/rename/remove, Agent Access disable, host disconnect, and app shutdown.
- [ ] Confirm stale/replayed/cross-client export/snapshot/approval IDs fail.
- [ ] Inject publication/rollback/audit failures where supported; confirm `recovery_required`, no false success, and visible Tarik recovery guidance.
- [ ] Restart and confirm startup recovery removes exact stages/restores exact backups without touching unrelated files.

## Required native Windows evidence — not run

Run natively on Windows 10/11 x64 MSVC; Linux cross-compilation is not accepted.

- [ ] Real Claude Desktop managed configuration under `%APPDATA%`.
- [ ] Real Claude Code and Codex CLI add/get/list/remove contracts with spaces and Unicode paths.
- [ ] Current-user executable/config/destination ACL ownership checks.
- [ ] File, directory, symlink, junction, mount point, and reparse-point adversarial matrix.
- [ ] Local versus UNC/network/mapped/remote filesystem classification.
- [ ] Same-directory atomic replace under Windows file locking; backup/rollback/repair/remove.
- [ ] Portable-folder movement and exact receipt-owned repair.
- [ ] `tarik-mcp.exe` and sidecar process startup/shutdown with no console window/orphans.
- [ ] WebView2 installed and missing-runtime behavior.
- [ ] 100/125/150/200% DPI and mixed-monitor transitions.
- [ ] Windows package manifest/checksum/contents with `sourceDirty: false` for publication.

## Real-Tauri visual/accessibility evidence — postponed

Review light/dark/system themes, minimum 680x520 viewport, keyboard-only navigation, screen-reader names/descriptions, reduced motion, and focus trap/return for:

- [ ] Agent Access overview, pairing, grant, and empty/error states.
- [ ] Connection host rows and nested setup/review/repair/remove modal.
- [ ] Optional Pi guidance and its nested modal.
- [ ] Destination list/create/edit/disable/enable/repair/revoke flow.
- [ ] Ordinary export approval and critical replacement phrase flow.
- [ ] Long SQL, labels, paths shown only where desktop-authorized, errors, and responsive stacking.

## Known truthful limitations

- Tarik Desktop must be running with Agent Access enabled and the relevant project active.
- `tarik-mcp` is a host-managed stdio child; there is no remote/HTTP listener.
- `tarik-mcp` never opens the project DuckDB file independently.
- One project is active at a time; one agent export per connection and four globally.
- Destination grants are local reusable authority; they are not cloud destinations or credential stores.
- Replacement is never delegated or remembered and always requires a fresh critical decision.
- Empty successful export creates no file.
- macOS delegated destination policy is not declared; non-Linux Unix fails closed until reviewed.
- Same-user malware remains outside this boundary.
- Artifacts are unsigned.

## Sign-off

- [ ] Connection assistant behavior approved.
- [ ] MCP prompts/instructions and optional Agent Skill approved.
- [ ] Pairing/project-grant authority and path redaction approved.
- [x] Linux native-picker destination creation, custom external-project-parent folder policy, validation presentation, and destination UI approved September 12, 2026. Native Windows repair/revoke/filesystem evidence remains separately open below.
- [ ] Complete CSV/Parquet export, quota, cancellation, release, and recovery approved.
- [ ] Ordinary export approval and critical replacement UX approved.
- [ ] Light/dark/system, minimum viewport, keyboard, screen reader, and reduced motion approved.
- [ ] Native Windows hosts, ACL/reparse/network/filesystem, WebView2, DPI, and process lifecycle approved.
- [ ] Clean publication artifacts regenerated with `sourceDirty: false` and checksums approved.
- [ ] E15.1 accepted by the user.
