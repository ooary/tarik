import { ExportIcon, FolderOpenIcon, XIcon } from "@phosphor-icons/react";
import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useMemo, useState } from "react";
import { Button, Field } from "../../components/ui";
import {
  cancelExport,
  chooseExportDirectory,
  executeExport,
  getExportStatus,
  revealExportPart,
  type ExportFormat,
  type ExportOptions,
  type ExportView,
  type ParquetCompression,
} from "../../lib/commands";
import { isClearlyReadOnlySql } from "../editor/sqlText";
import { formatCompactCount, formatFileSize } from "../sources/format";
import "./export.css";

interface ExportDialogProps {
  projectId: string;
  sql: string;
  suggestedName: string;
}

interface FormErrors {
  outputDirectory?: string;
  baseName?: string;
  rowsPerPart?: string;
  delimiter?: string;
}

function safeSuggestedName(value: string): string {
  const normalized = value
    .trim()
    .replace(/[^A-Za-z0-9_-]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 128);
  return normalized && normalized.toLowerCase() !== "untitled" ? normalized : "query_export";
}

function validateForm(
  outputDirectory: string,
  baseName: string,
  rowsPerPart: string,
  format: ExportFormat,
  delimiter: string,
): FormErrors {
  const errors: FormErrors = {};
  const directory = outputDirectory.trim();
  if (!directory) {
    errors.outputDirectory = "Choose an existing output folder.";
  } else if (!/^(?:\/|[A-Za-z]:[\\/]|\\\\)/.test(directory)) {
    errors.outputDirectory = "Use an absolute output folder path.";
  }
  if (!baseName.trim()) {
    errors.baseName = "Enter a base name.";
  } else if (baseName.length > 128 || !/^[A-Za-z0-9_-]+$/.test(baseName)) {
    errors.baseName = "Use up to 128 letters, digits, hyphens, or underscores.";
  }
  const rows = Number(rowsPerPart);
  if (!Number.isSafeInteger(rows) || rows <= 0) {
    errors.rowsPerPart = "Enter a positive whole number within the safe integer range.";
  }
  if (
    format === "csv" &&
    (new TextEncoder().encode(delimiter).length !== 1 || /[\0"\r\n]/.test(delimiter))
  ) {
    errors.delimiter = "Use one ASCII byte other than quote, NUL, or a line break.";
  }
  return errors;
}

export function ExportDialog({ projectId, sql, suggestedName }: ExportDialogProps) {
  const [open, setOpen] = useState(false);
  const [format, setFormat] = useState<ExportFormat>("parquet");
  const [outputDirectory, setOutputDirectory] = useState("");
  const [baseName, setBaseName] = useState(() => safeSuggestedName(suggestedName));
  const [rowsPerPart, setRowsPerPart] = useState("1000000");
  const [overwrite, setOverwrite] = useState<ExportOptions["overwrite"]>("fail_if_exists");
  const [delimiter, setDelimiter] = useState(",");
  const [includeHeader, setIncludeHeader] = useState(true);
  const [compression, setCompression] = useState<ParquetCompression>("snappy");
  const [errors, setErrors] = useState<FormErrors>({});
  const [requestError, setRequestError] = useState<string | null>(null);
  const [submittedSql, setSubmittedSql] = useState<string | null>(null);
  const [view, setView] = useState<ExportView | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [cancelling, setCancelling] = useState(false);

  const active = view?.state === "queued" || view?.state === "running";
  const activeExportId = active ? view.exportId : null;
  const terminal = view && !active;

  useEffect(() => {
    if (!activeExportId) return;
    let disposed = false;
    let timeout: number;
    const exportId = activeExportId;
    const poll = async () => {
      try {
        const next = await getExportStatus(exportId);
        if (disposed || !next) return;
        setView(next);
        setRequestError(null);
        if (next.state === "queued" || next.state === "running") {
          timeout = window.setTimeout(poll, 180);
        }
      } catch (cause) {
        if (disposed) return;
        setRequestError(String(cause));
        timeout = window.setTimeout(poll, 180);
      }
    };
    timeout = window.setTimeout(poll, 180);
    return () => {
      disposed = true;
      window.clearTimeout(timeout);
    };
  }, [activeExportId]);

  const options = useMemo<ExportOptions>(
    () => ({
      format,
      outputDirectory: outputDirectory.trim(),
      baseName: baseName.trim(),
      rowsPerPart: Number(rowsPerPart),
      overwrite,
      csv: format === "csv" ? { delimiter, includeHeader } : null,
      parquet: format === "parquet" ? { compression } : null,
    }),
    [
      baseName,
      compression,
      delimiter,
      format,
      includeHeader,
      outputDirectory,
      overwrite,
      rowsPerPart,
    ],
  );

  async function chooseFolder() {
    const directory = await chooseExportDirectory();
    if (directory) {
      setOutputDirectory(directory);
      setErrors((current) => ({ ...current, outputDirectory: undefined }));
    }
  }

  async function startExport() {
    const nextErrors = validateForm(outputDirectory, baseName, rowsPerPart, format, delimiter);
    setErrors(nextErrors);
    setRequestError(null);
    if (Object.keys(nextErrors).length > 0) return;
    if (
      !isClearlyReadOnlySql(sql) &&
      !window.confirm(
        "Start export?\n\nExport executes this SQL once before writing files. INSERT, UPDATE, DELETE, CREATE, ALTER, and DROP may modify your project.",
      )
    ) {
      return;
    }
    setSubmitting(true);
    try {
      const snapshot = sql;
      setView(await executeExport(projectId, snapshot, options));
      setSubmittedSql(snapshot);
    } catch (cause) {
      setRequestError(String(cause));
    } finally {
      setSubmitting(false);
    }
  }

  async function requestCancel() {
    if (!view || !active) return;
    setCancelling(true);
    setRequestError(null);
    try {
      const next = await cancelExport(view.exportId);
      setView(next);
    } catch (cause) {
      setRequestError(String(cause));
    } finally {
      setCancelling(false);
    }
  }

  function reset() {
    setView(null);
    setSubmittedSql(null);
    setRequestError(null);
    setErrors({});
    setBaseName(safeSuggestedName(suggestedName));
  }

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Trigger asChild>
        <button
          className="toolbar-button export-trigger"
          disabled={!projectId || !sql.trim()}
          type="button"
        >
          <ExportIcon aria-hidden="true" size={14} />
          Export
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="ui-dialog-overlay" />
        <Dialog.Content className="ui-dialog-content export-dialog">
          <header className="ui-dialog-header">
            <div>
              <Dialog.Title>Export query</Dialog.Title>
              <Dialog.Description>
                Run this SQL once and write exact-row CSV or Parquet parts.
              </Dialog.Description>
            </div>
            <Dialog.Close aria-label="Close export" className="icon-button">
              <XIcon aria-hidden="true" size={16} weight="bold" />
            </Dialog.Close>
          </header>

          {!view ? (
            <form
              className="export-form"
              onSubmit={(event) => {
                event.preventDefault();
                void startExport();
              }}
            >
              <section className="export-snapshot" aria-label="SQL snapshot">
                <span>SQL snapshot</span>
                <code>{sql.trim() || "No SQL"}</code>
              </section>

              <fieldset className="export-format">
                <legend>File format</legend>
                <label>
                  <input
                    checked={format === "parquet"}
                    name="export-format"
                    onChange={() => setFormat("parquet")}
                    type="radio"
                  />
                  <span>
                    <strong>Parquet</strong>
                    <small>Typed, compressed columnar files.</small>
                  </span>
                </label>
                <label>
                  <input
                    checked={format === "csv"}
                    name="export-format"
                    onChange={() => setFormat("csv")}
                    type="radio"
                  />
                  <span>
                    <strong>CSV</strong>
                    <small>Portable text files with optional headers.</small>
                  </span>
                </label>
              </fieldset>

              <Field
                error={errors.outputDirectory}
                label="Output folder"
                onChange={(event) => setOutputDirectory(event.currentTarget.value)}
                trailing={
                  <button
                    aria-label="Choose output folder"
                    className="icon-button export-folder-button"
                    onClick={() => void chooseFolder()}
                    type="button"
                  >
                    <FolderOpenIcon aria-hidden="true" size={16} />
                  </button>
                }
                value={outputDirectory}
              />

              <div className="export-field-grid">
                <Field
                  error={errors.baseName}
                  hint="Part numbering is added automatically."
                  label="Base name"
                  maxLength={128}
                  onChange={(event) => setBaseName(event.currentTarget.value)}
                  value={baseName}
                />
                <Field
                  error={errors.rowsPerPart}
                  label="Rows per part"
                  min={1}
                  onChange={(event) => setRowsPerPart(event.currentTarget.value)}
                  step={1}
                  type="number"
                  value={rowsPerPart}
                />
              </div>

              <label className="export-select-field">
                <span>Existing files</span>
                <select
                  onChange={(event) =>
                    setOverwrite(event.currentTarget.value as ExportOptions["overwrite"])
                  }
                  value={overwrite}
                >
                  <option value="fail_if_exists">Stop without replacing</option>
                  <option value="replace">Replace completed parts</option>
                </select>
              </label>

              {format === "csv" ? (
                <section className="export-format-options" aria-label="CSV options">
                  <Field
                    error={errors.delimiter}
                    label="Delimiter"
                    maxLength={1}
                    onChange={(event) => setDelimiter(event.currentTarget.value)}
                    value={delimiter}
                  />
                  <label className="checkbox-setting">
                    <input
                      checked={includeHeader}
                      onChange={(event) => setIncludeHeader(event.currentTarget.checked)}
                      type="checkbox"
                    />
                    Include column names in every part
                  </label>
                </section>
              ) : (
                <label className="export-select-field">
                  <span>Parquet compression</span>
                  <select
                    onChange={(event) =>
                      setCompression(event.currentTarget.value as ParquetCompression)
                    }
                    value={compression}
                  >
                    <option value="snappy">Snappy</option>
                    <option value="zstd">Zstandard</option>
                    <option value="gzip">Gzip</option>
                    <option value="uncompressed">Uncompressed</option>
                  </select>
                </label>
              )}

              {requestError && (
                <div className="ui-inline-error" role="alert">
                  <strong>Export could not start</strong>
                  <span>{requestError}</span>
                </div>
              )}

              <footer className="export-dialog-footer">
                <Dialog.Close asChild>
                  <Button type="button">Cancel</Button>
                </Dialog.Close>
                <Button disabled={submitting} tone="primary" type="submit">
                  {submitting ? "Starting" : "Start export"}
                </Button>
              </footer>
            </form>
          ) : (
            <div className="export-progress-view">
              <header className="export-state-heading">
                <div>
                  <span>{stateLabel(view)}</span>
                  <strong>{terminalHeading(view)}</strong>
                </div>
                <code>{view.exportId.slice(0, 8)}</code>
              </header>

              <section className="export-snapshot" aria-label="Submitted SQL snapshot">
                <span>Submitted SQL</span>
                <code>{submittedSql?.trim() || "SQL snapshot unavailable"}</code>
              </section>

              <div className="export-metrics" aria-label="Export progress">
                <div>
                  <span>Rows</span>
                  <strong>{formatCompactCount(view.rowsWritten)}</strong>
                </div>
                <div>
                  <span>Files</span>
                  <strong>{view.filesWritten.toLocaleString("en-US")}</strong>
                </div>
                <div>
                  <span>Written</span>
                  <strong>{formatFileSize(view.bytesWritten)}</strong>
                </div>
                <div>
                  <span>Elapsed</span>
                  <strong>{formatElapsed(view.durationMs)}</strong>
                </div>
              </div>

              {active && (
                <p className="export-running-note" role="status">
                  {view.state === "queued"
                    ? "Waiting for earlier export work in this project."
                    : `Writing part ${view.currentPart?.toLocaleString("en-US") ?? "1"}. You can close this dialog without stopping the export.`}
                </p>
              )}

              {view.error && (
                <div className="ui-inline-error" role="alert">
                  <strong>{view.error.code}</strong>
                  <span>{view.error.message}</span>
                </div>
              )}

              <section className="export-parts" aria-label="Completed export parts">
                <h3>Completed parts</h3>
                {view.filesWritten > view.completedParts.length && (
                  <p>
                    Showing the latest {view.completedParts.length.toLocaleString("en-US")} of{" "}
                    {view.filesWritten.toLocaleString("en-US")} completed files.
                  </p>
                )}
                {view.completedParts.length === 0 ? (
                  <p>
                    {view.state === "succeeded"
                      ? "Query returned no rows; no files were created."
                      : "No completed files yet."}
                  </p>
                ) : (
                  <div className="export-part-list">
                    {view.completedParts.map((part) => (
                      <div className="export-part-row" key={part.partNumber}>
                        <div>
                          <strong>Part {part.partNumber.toLocaleString("en-US")}</strong>
                          <span title={part.path}>{part.path}</span>
                        </div>
                        <span>
                          {part.rows.toLocaleString("en-US")} rows
                          <small>{formatFileSize(part.bytes)}</small>
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </section>

              {terminal && view.completedParts.length > 0 && view.state !== "succeeded" && (
                <p className="export-partial-note">
                  Completed files remain valid. The incomplete current part was removed.
                </p>
              )}
              {requestError && (
                <div className="ui-inline-error" role="alert">
                  <strong>Export status unavailable</strong>
                  <span>{requestError}</span>
                </div>
              )}

              <footer className="export-dialog-footer">
                {active ? (
                  <Button disabled={cancelling} onClick={() => void requestCancel()} type="button">
                    {cancelling ? "Cancelling" : "Cancel export"}
                  </Button>
                ) : (
                  <>
                    {view.completedParts[0] && (
                      <Button
                        onClick={() => void revealExportPart(view.completedParts[0].path)}
                        type="button"
                      >
                        Reveal output
                      </Button>
                    )}
                    <Button onClick={reset} type="button">
                      Export again
                    </Button>
                    <Dialog.Close asChild>
                      <Button tone="primary" type="button">
                        Done
                      </Button>
                    </Dialog.Close>
                  </>
                )}
              </footer>
            </div>
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function stateLabel(view: ExportView): string {
  switch (view.state) {
    case "queued":
      return "Queued";
    case "running":
      return "Exporting";
    case "succeeded":
      return "Completed";
    case "failed":
      return "Failed";
    case "cancelled":
      return "Cancelled";
  }
}

function terminalHeading(view: ExportView): string {
  if (view.state === "succeeded") {
    return view.filesWritten === 0 ? "No files needed" : "Export complete";
  }
  if (view.state === "failed") return "Export stopped with an error";
  if (view.state === "cancelled") return "Export was cancelled";
  return view.state === "queued" ? "Waiting to start" : "Writing output files";
}

function formatElapsed(durationMs: number): string {
  if (durationMs >= 60_000)
    return `${Math.floor(durationMs / 60_000)}m ${Math.round((durationMs % 60_000) / 1000)}s`;
  if (durationMs >= 10_000) return `${Math.round(durationMs / 1000)}s`;
  return `${(durationMs / 1000).toFixed(1)}s`;
}
