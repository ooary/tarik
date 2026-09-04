# E11 local security and filesystem boundary review

**Verdict:** no unresolved high-severity finding in the Linux MVP command/filesystem surface. Deferred E10 manual review remains a release gate, not a security bypass.

## Threat model

Tarik is a local-only desktop app. The WebView is less trusted than Rust services and the DuckDB sidecar. User-selected SQL is intentionally executable only through explicit Run, Actual Flow, and Export actions; pre-run validation remains EXPLAIN-only. The security goal is not to sandbox user SQL from its owner, but to prevent stale/forged WebView messages, generated SQL, cleanup, and project operations from selecting unintended local resources.

## Invoke surface

The handler list and `src/lib/commands.ts` are audited together. Ten generic metadata commands were removed from the public invoke handler because no visible workflow used them and they bypassed higher-level ownership coordinators:

- `upsert_recent_project`
- `touch_recent_project`
- `remove_recent_project`
- `add_query_history`
- `list_query_history`
- `prune_query_history`
- `upsert_source`
- `remove_source`
- `set_source_state`
- `get_source`

Repositories retain these operations for Rust composition/tests. Public workflows use ProjectManager, QueryCoordinator, ExportCoordinator, or project-scoped query-library commands.

## Tauri and WebView policy

- Capability applies only to the `main` window.
- `core:default` and native dialog `allow-open` are enabled.
- The broad `opener:default` permission is removed. The WebView cannot submit arbitrary paths to the opener plugin.
- Logs are revealed by `reveal_log_directory`, which takes no path and uses the backend-owned resolved log file.
- Exports are revealed by `reveal_export_part(exportId, partNumber)`, which resolves a regular existing canonical part from the coordinator's validated immutable options and completed-part record.
- CSP is non-null:

  ```text
  default-src 'self';
  connect-src ipc: http://ipc.localhost;
  img-src 'self' asset: http://asset.localhost data:;
  style-src 'self' 'unsafe-inline';
  script-src 'self'
  ```

  Inline styles remain necessary for CodeMirror/XYFlow geometry. Inline scripts, remote scripts, remote frames, and arbitrary network origins are not allowed.

## Boundary matrix

| Boundary                | Adversarial input                                             | Enforcement                                                                                                                        | Evidence                                   |
| ----------------------- | ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| Active project          | forged project ID for query/export/catalog                    | command compares active project ID; coordinator verifies metadata project exists                                                   | query/export/project tests                 |
| Catalog deletion        | stale object, wrong kind, quoted name                         | fresh engine catalog lookup and exact kind match; central sidecar identifier quoting                                               | project and sidecar tests                  |
| Import/link             | quote, spaces, SQL punctuation in path/name                   | path transported as JSON and DuckDB SQL literal escaping; identifier double-quote escaping and type allowlist                      | source tests; golden fixture               |
| External project remove | managed/external mismatch                                     | ownership persisted by Rust; external remove forgets metadata only                                                                 | outside-file preservation test             |
| Managed project remove  | forged path outside managed root                              | exact parent-of-parent equality with resolved projects root before staging/delete                                                  | unsafe managed-path test                   |
| Export options          | relative/missing directory, traversal basename, mixed options | protocol validator canonicalizes existing absolute dir; portable basename; format-specific options                                 | protocol/engine/desktop tests              |
| Export publication      | late collision, replace failure                               | same-directory hard-link for fail-if-exists; exact backup+rollback for replace                                                     | writer collision/rollback tests            |
| Export reveal           | arbitrary filesystem path                                     | frontend sends only export ID + part; backend derives exact canonical file from immutable validated options and tracked completion | coordinator path tests; typed command test |
| Result paging           | unknown result or huge request                                | sidecar registry identity; max rows clamped to 5,000; desktop uses fixed 500                                                       | sidecar paging tests                       |
| Clear cache             | caller-selected root/symlink                                  | command takes no root; Tauri resolver owns root; direct-child and no-symlink checks                                                | cleanup outside-sentinel/symlink tests     |
| Export crash cleanup    | malicious/malformed manifest                                  | manifest owned under cache, UUID/name/absolute-dir checks, exact hidden UUID+part parsing; no recursive output deletion            | malformed/exact recovery tests             |
| Log reveal              | arbitrary path                                                | command takes no path; logger owns resolved log file                                                                               | typed command test                         |
| Frontend incident       | spoofed ID/kind/message                                       | invalid UUID replaced, tokens sanitized, SQL-like message redacted/bounded                                                         | observability tests                        |
| Shutdown                | path/PID/resource injection                                   | command takes only boolean skip-draft; coordinator owns resources at setup                                                         | shutdown state/typed command tests         |
| SQL diagnostics         | mutating SQL                                                  | EXPLAIN without ANALYZE and mutation sentinels                                                                                     | validation integration tests               |

## Known intentional capabilities

- Native open dialogs let the user select CSV, Parquet, DuckDB files, and export directories. Rust validates format/existence/ownership before use.
- Explicit Run, Actual Flow, and Export execute the user's SQL against the active local DuckDB project. Mutation warnings/confirmations reduce mistakes but are not an authorization boundary.
- Logs and exports can be revealed in the system file manager only through backend-owned paths.

## Deferred and out of scope

- Code signing and publisher identity are E11-T4/E12 concerns. SHA-256 checksums detect transfer corruption but do not establish publisher trust.
- Windows path/locking/UNC hardening is E12.
- Remote database integrations and credentials are out of scope; CSP has no external API origins.
- E10 incident/cleanup/shutdown manual review remains deferred and must close before final release acceptance.
