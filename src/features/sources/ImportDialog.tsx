import { XIcon } from "@phosphor-icons/react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { useMemo, useState } from "react";
import type { CsvOptions, ImportOptions, SourceInspection } from "../../lib/commands";
import { formatCompactCount, formatFileSize } from "./format";

export type SourceAction = "import" | "link";

const DUCKDB_TYPE_CHOICES = [
  "BOOLEAN",
  "TINYINT",
  "SMALLINT",
  "INTEGER",
  "BIGINT",
  "HUGEINT",
  "FLOAT",
  "DOUBLE",
  "DECIMAL(18,2)",
  "VARCHAR",
  "DATE",
  "TIME",
  "TIMESTAMP",
  "BLOB",
] as const;

export function ImportDialog({
  inspection,
  busy,
  error,
  onCancel,
  onClose,
  onInspectCsv,
  onSubmit,
}: {
  inspection: SourceInspection;
  busy: boolean;
  error: string | null;
  onCancel: () => void;
  onClose: () => void;
  onInspectCsv: (options: CsvOptions) => Promise<void>;
  onSubmit: (action: SourceAction, options: ImportOptions) => Promise<void>;
}) {
  const [action, setAction] = useState<SourceAction>(
    inspection.format === "parquet" ? "link" : "import",
  );
  const [tableName, setTableName] = useState(inspection.suggestedName);
  const [csv, setCsv] = useState<CsvOptions>(
    inspection.csvOptions ?? {
      delimiter: ",",
      hasHeader: true,
      nullValue: null,
      allVarchar: false,
    },
  );
  const [overrides, setOverrides] = useState<Record<string, string>>({});

  const importOptions = useMemo<ImportOptions>(
    () => ({
      tableName,
      csv: inspection.format === "csv" ? csv : null,
      columnOverrides: Object.entries(overrides)
        .filter(([, dataType]) => dataType.trim().length > 0)
        .map(([column, dataType]) => ({ column, dataType: dataType.trim() })),
    }),
    [csv, inspection.format, overrides, tableName],
  );

  return (
    <DialogPrimitive.Root open onOpenChange={(open) => !open && !busy && onClose()}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="ui-dialog-overlay" />
        <DialogPrimitive.Content className="ui-dialog-content import-dialog">
          <header className="ui-dialog-header">
            <div>
              <DialogPrimitive.Title>Add local source</DialogPrimitive.Title>
              <DialogPrimitive.Description>
                Inspect the inferred schema, then choose how Tarik should add this file.
              </DialogPrimitive.Description>
            </div>
            <DialogPrimitive.Close
              aria-label="Close import"
              className="icon-button"
              disabled={busy}
            >
              <XIcon aria-hidden="true" size={16} weight="bold" />
            </DialogPrimitive.Close>
          </header>

          <div className="import-dialog-body">
            <div className="import-file-summary">
              <strong>{inspection.path.split(/[\\/]/).pop()}</strong>
              <span>{inspection.path}</span>
            </div>

            {inspection.format === "parquet" && (
              <fieldset className="import-mode">
                <legend>Add as</legend>
                <label>
                  <input
                    checked={action === "link"}
                    name="source-action"
                    onChange={() => setAction("link")}
                    type="radio"
                  />
                  <span>
                    <strong>Link file</strong>
                    <small>Recommended. No data copy; keep the original file in place.</small>
                  </span>
                </label>
                <label>
                  <input
                    checked={action === "import"}
                    name="source-action"
                    onChange={() => setAction("import")}
                    type="radio"
                  />
                  <span>
                    <strong>Import as table</strong>
                    <small>Copy rows into the active DuckDB project.</small>
                  </span>
                </label>
              </fieldset>
            )}

            <div className="source-summary" aria-label="Source summary">
              <div>
                <span>{action === "link" ? "View" : "Table"}</span>
                <strong title={tableName}>{tableName || "Not named"}</strong>
              </div>
              <div>
                <span>Columns</span>
                <strong title={`${inspection.columns.length} columns`}>
                  {formatCompactCount(inspection.columns.length)}
                </strong>
              </div>
              <div>
                <span>Rows</span>
                <strong
                  title={`${inspection.rowCountExact ? "Exact" : "Estimated"}: ${inspection.rowCount.toLocaleString("en-US")} rows`}
                >
                  {inspection.rowCountExact ? "" : "~"}
                  {formatCompactCount(inspection.rowCount)}
                </strong>
              </div>
              <div>
                <span>File size</span>
                <strong title={`${inspection.fileSizeBytes.toLocaleString("en-US")} bytes`}>
                  {formatFileSize(inspection.fileSizeBytes)}
                </strong>
              </div>
            </div>

            <label className="ui-field" htmlFor="source-table-name">
              <span className="ui-field-label">
                {action === "link" ? "View name" : "Table name"}
              </span>
              <span className="ui-field-control">
                <input
                  id="source-table-name"
                  onChange={(event) => setTableName(event.currentTarget.value)}
                  value={tableName}
                />
              </span>
            </label>

            {inspection.format === "csv" && (
              <section className="csv-settings" aria-label="CSV parsing">
                <h3>CSV parsing</h3>
                <div className="csv-setting-grid">
                  <label>
                    <span>Delimiter</span>
                    <input
                      aria-label="Delimiter"
                      maxLength={1}
                      onBlur={() => onInspectCsv(csv)}
                      onChange={(event) => setCsv({ ...csv, delimiter: event.currentTarget.value })}
                      value={csv.delimiter}
                    />
                  </label>
                  <label>
                    <span>Null value</span>
                    <input
                      aria-label="Null value"
                      onBlur={() => onInspectCsv(csv)}
                      onChange={(event) =>
                        setCsv({ ...csv, nullValue: event.currentTarget.value || null })
                      }
                      value={csv.nullValue ?? ""}
                    />
                  </label>
                  <label className="checkbox-setting">
                    <input
                      checked={csv.hasHeader}
                      onChange={(event) => {
                        const next = { ...csv, hasHeader: event.currentTarget.checked };
                        setCsv(next);
                        onInspectCsv(next);
                      }}
                      type="checkbox"
                    />
                    First row is header
                  </label>
                  <label className="checkbox-setting">
                    <input
                      checked={csv.allVarchar}
                      onChange={(event) => {
                        const next = { ...csv, allVarchar: event.currentTarget.checked };
                        setCsv(next);
                        onInspectCsv(next);
                      }}
                      type="checkbox"
                    />
                    Read every column as text
                  </label>
                </div>
              </section>
            )}

            <section className="schema-preview" aria-label="Inferred schema">
              <h3>Inferred schema</h3>
              <div className="schema-list">
                {inspection.columns.map((column) => (
                  <div className="schema-row" key={column.name}>
                    <span>{column.name}</span>
                    <code>{column.dataType}</code>
                    {action === "import" && (
                      <select
                        aria-label={`Override type for ${column.name}`}
                        onChange={(event) =>
                          setOverrides({ ...overrides, [column.name]: event.currentTarget.value })
                        }
                        value={overrides[column.name] ?? ""}
                      >
                        <option value="">Keep inferred ({column.dataType})</option>
                        {DUCKDB_TYPE_CHOICES.map((dataType) => (
                          <option key={dataType} value={dataType}>
                            {dataType}
                          </option>
                        ))}
                      </select>
                    )}
                  </div>
                ))}
              </div>
            </section>

            <section className="source-preview" aria-label="Data preview">
              <h3>Preview (first {inspection.previewRows.length} rows)</h3>
              <div className="source-preview-scroll">
                <table>
                  <thead>
                    <tr>
                      {inspection.columns.map((column) => (
                        <th key={column.name}>{column.name}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {inspection.previewRows.map((row, rowIndex) => (
                      <tr key={rowIndex}>
                        {row.map((value, columnIndex) => (
                          <td key={inspection.columns[columnIndex]?.name ?? columnIndex}>
                            {value === null ? (
                              <span className="null-value">NULL</span>
                            ) : (
                              String(value)
                            )}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </section>

            {error && (
              <p className="import-error" role="alert">
                {error}
              </p>
            )}
          </div>

          <footer className="import-dialog-footer">
            {busy ? (
              <button className="toolbar-button" onClick={onCancel} type="button">
                Cancel operation
              </button>
            ) : (
              <button className="toolbar-button" onClick={onClose} type="button">
                Cancel
              </button>
            )}
            <button
              className="run-button"
              disabled={busy || tableName.trim().length === 0}
              onClick={() => onSubmit(action, importOptions)}
              type="button"
            >
              {busy ? "Working..." : action === "link" ? "Link Parquet" : "Import table"}
            </button>
          </footer>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
