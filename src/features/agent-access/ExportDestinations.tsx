import { FolderOpenIcon, TrashIcon } from "@phosphor-icons/react";
import { useId, useLayoutEffect, useRef, useState, type FormEvent } from "react";
import { Dialog } from "../../components/ui";
import {
  chooseAgentExportDirectory,
  createAgentExportDestination,
  listAgentExportDestinations,
  repairAgentExportDestination,
  revokeAgentExportDestination,
  setAgentExportDestinationEnabled,
  updateAgentExportDestination,
  type ActiveProject,
  type AgentClient,
  type AgentExportDestination,
  type AgentExportDestinationPolicy,
} from "../../lib/commands";

const GIB = 1024 * 1024 * 1024;
const DEFAULT_POLICY: AgentExportDestinationPolicy = {
  displayLabel: "Agent exports",
  allowCsv: true,
  allowParquet: true,
  maximumRowsPerPart: 1_000_000,
  maximumTotalBytes: 10 * GIB,
};

function destinationPolicyErrors(policy: AgentExportDestinationPolicy) {
  const labelBytes = new TextEncoder().encode(policy.displayLabel.trim()).length;
  return {
    displayLabel:
      labelBytes === 0
        ? "Enter a display label."
        : labelBytes > 80
          ? "Display label must be 80 bytes or fewer."
          : null,
    formats: !policy.allowCsv && !policy.allowParquet ? "Select at least one format." : null,
    maximumRowsPerPart:
      Number.isInteger(policy.maximumRowsPerPart) &&
      policy.maximumRowsPerPart >= 1 &&
      policy.maximumRowsPerPart <= 1_000_000
        ? null
        : "Rows per part must be a whole number from 1 to 1,000,000.",
    maximumTotalBytes:
      Number.isFinite(policy.maximumTotalBytes) &&
      policy.maximumTotalBytes >= GIB &&
      policy.maximumTotalBytes <= 100 * GIB
        ? null
        : "Total limit must be from 1 to 100 GiB.",
  };
}

function retainDestination(
  destinations: AgentExportDestination[],
  saved: AgentExportDestination,
): AgentExportDestination[] {
  const existing = destinations.findIndex(
    (destination) => destination.destinationId === saved.destinationId,
  );
  if (existing < 0) return [saved, ...destinations];
  return destinations.map((destination, index) => (index === existing ? saved : destination));
}

export function ExportDestinations({
  client,
  disabled,
  project,
}: {
  client: AgentClient;
  disabled: boolean;
  project: ActiveProject;
}) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const labelInputRef = useRef<HTMLInputElement>(null);
  const refreshRequestRef = useRef(0);
  const fieldId = useId();
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [destinations, setDestinations] = useState<AgentExportDestination[]>([]);
  const [editing, setEditing] = useState<AgentExportDestination | null>(null);
  const [selectedDirectory, setSelectedDirectory] = useState<string | null>(null);
  const [policy, setPolicy] = useState<AgentExportDestinationPolicy>(DEFAULT_POLICY);
  const validation = destinationPolicyErrors(policy);
  const policyValid = Object.values(validation).every((message) => message === null);

  useLayoutEffect(() => {
    if (selectedDirectory && !editing) labelInputRef.current?.focus();
  }, [editing, selectedDirectory]);

  async function refresh(preferred?: AgentExportDestination) {
    const request = ++refreshRequestRef.current;
    setLoading(true);
    try {
      const result = await listAgentExportDestinations(client.id, project.id);
      if (request !== refreshRequestRef.current) return;
      setDestinations(
        preferred ? retainDestination(result.destinations, preferred) : result.destinations,
      );
      setError(null);
    } catch (nextError) {
      if (request !== refreshRequestRef.current) return;
      setError(String(nextError));
    } finally {
      if (request === refreshRequestRef.current) setLoading(false);
    }
  }

  function openDialog() {
    setOpen(true);
    void refresh();
  }

  function resetEditor() {
    setEditing(null);
    setSelectedDirectory(null);
    setPolicy(DEFAULT_POLICY);
  }

  async function chooseFolder(repair: AgentExportDestination | null = null) {
    if (busy) return;
    setError(null);
    try {
      const selected = await chooseAgentExportDirectory();
      if (!selected) return;
      if (repair) {
        setBusy(true);
        await repairAgentExportDestination(client.id, project.id, repair.destinationId, selected);
        await refresh();
      } else {
        setEditing(null);
        setSelectedDirectory(selected);
        setPolicy(DEFAULT_POLICY);
      }
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusy(false);
    }
  }

  function edit(destination: AgentExportDestination) {
    setSelectedDirectory(null);
    setEditing(destination);
    setPolicy({
      displayLabel: destination.label,
      allowCsv: destination.formats.includes("csv"),
      allowParquet: destination.formats.includes("parquet"),
      maximumRowsPerPart: destination.maximumRowsPerPart,
      maximumTotalBytes: destination.maximumTotalBytes,
    });
  }

  async function savePolicy(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy || !policyValid || (!editing && !selectedDirectory)) return;
    setBusy(true);
    setError(null);
    try {
      const saved = editing
        ? await updateAgentExportDestination(client.id, project.id, editing.destinationId, policy)
        : await createAgentExportDestination(client.id, project.id, selectedDirectory!, policy);
      setDestinations((current) => retainDestination(current, saved));
      resetEditor();
      await refresh(saved);
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusy(false);
    }
  }

  async function mutate(operation: () => Promise<unknown>) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await operation();
      resetEditor();
      await refresh();
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <button
        className="toolbar-button agent-destination-trigger"
        disabled={disabled}
        onClick={openDialog}
        ref={triggerRef}
        type="button"
      >
        Export destinations
      </button>
      <Dialog
        busy={busy}
        contentClassName="agent-destination-dialog"
        description={`Manage create-new-only export folders delegated to ${client.displayName} for ${project.name}. Folder paths stay private in Tarik.`}
        onOpenChange={(nextOpen) => {
          setOpen(nextOpen);
          if (!nextOpen) {
            resetEditor();
            setError(null);
          }
        }}
        open={open}
        returnFocusRef={triggerRef}
        title="Export destinations"
      >
        <div className="agent-destination-summary">
          <div>
            <strong>{client.displayName}</strong>
            <span>
              {project.name} · {destinations.length}/8 destinations
            </span>
          </div>
          <button
            className="toolbar-button"
            disabled={busy || destinations.length >= 8}
            onClick={() => void chooseFolder()}
            type="button"
          >
            <FolderOpenIcon aria-hidden="true" size={14} />
            Choose folder
          </button>
        </div>

        <p className="agent-destination-note">
          Agents receive only an opaque destination ID and policy. Delegation creates new files
          only; overwrite is never remembered.
        </p>

        {error && (
          <div className="ui-inline-error" role="alert">
            <strong>Destination could not update</strong>
            <span>{error}</span>
          </div>
        )}

        {(selectedDirectory || editing) && (
          <form className="agent-destination-form" onSubmit={savePolicy}>
            <header>
              <strong>{editing ? `Edit ${editing.label}` : "New export destination"}</strong>
              <span>
                {editing ? "Folder unchanged" : "Folder selected. Review policy, then create."}
              </span>
            </header>
            <label>
              <span>Display label</span>
              <input
                aria-describedby={validation.displayLabel ? `${fieldId}-label-error` : undefined}
                aria-invalid={validation.displayLabel ? true : undefined}
                maxLength={80}
                onChange={(event) => {
                  const value = event.currentTarget.value;
                  setPolicy((current) => ({ ...current, displayLabel: value }));
                }}
                ref={labelInputRef}
                required
                value={policy.displayLabel}
              />
              {validation.displayLabel && (
                <span className="ui-field-error" id={`${fieldId}-label-error`} role="alert">
                  {validation.displayLabel}
                </span>
              )}
            </label>
            <fieldset
              aria-describedby={validation.formats ? `${fieldId}-formats-error` : undefined}
              aria-invalid={validation.formats ? true : undefined}
            >
              <legend>Allowed formats</legend>
              <label>
                <input
                  checked={policy.allowCsv}
                  onChange={(event) => {
                    const checked = event.currentTarget.checked;
                    setPolicy((current) => ({ ...current, allowCsv: checked }));
                  }}
                  type="checkbox"
                />
                CSV
              </label>
              <label>
                <input
                  checked={policy.allowParquet}
                  onChange={(event) => {
                    const checked = event.currentTarget.checked;
                    setPolicy((current) => ({ ...current, allowParquet: checked }));
                  }}
                  type="checkbox"
                />
                Parquet
              </label>
              {validation.formats && (
                <span className="ui-field-error" id={`${fieldId}-formats-error`} role="alert">
                  {validation.formats}
                </span>
              )}
            </fieldset>
            <div className="agent-destination-number-grid">
              <label>
                <span>Rows per part</span>
                <input
                  aria-describedby={
                    validation.maximumRowsPerPart ? `${fieldId}-rows-error` : undefined
                  }
                  aria-invalid={validation.maximumRowsPerPart ? true : undefined}
                  max={1_000_000}
                  min={1}
                  onChange={(event) => {
                    const value = Number(event.currentTarget.value);
                    setPolicy((current) => ({
                      ...current,
                      maximumRowsPerPart: value,
                    }));
                  }}
                  required
                  type="number"
                  value={policy.maximumRowsPerPart}
                />
                {validation.maximumRowsPerPart && (
                  <span className="ui-field-error" id={`${fieldId}-rows-error`} role="alert">
                    {validation.maximumRowsPerPart}
                  </span>
                )}
              </label>
              <label>
                <span>Total limit (GiB)</span>
                <input
                  aria-describedby={
                    validation.maximumTotalBytes ? `${fieldId}-total-error` : undefined
                  }
                  aria-invalid={validation.maximumTotalBytes ? true : undefined}
                  max={100}
                  min={1}
                  onChange={(event) => {
                    const value = Number(event.currentTarget.value) * GIB;
                    setPolicy((current) => ({
                      ...current,
                      maximumTotalBytes: value,
                    }));
                  }}
                  required
                  type="number"
                  value={policy.maximumTotalBytes / GIB}
                />
                {validation.maximumTotalBytes && (
                  <span className="ui-field-error" id={`${fieldId}-total-error`} role="alert">
                    {validation.maximumTotalBytes}
                  </span>
                )}
              </label>
            </div>
            <div className="agent-setup-actions">
              <button className="toolbar-button" onClick={resetEditor} type="button">
                Cancel
              </button>
              <button className="run-button" disabled={busy || !policyValid} type="submit">
                {busy
                  ? editing
                    ? "Saving…"
                    : "Creating…"
                  : editing
                    ? "Save policy"
                    : "Create destination"}
              </button>
            </div>
          </form>
        )}

        {loading && destinations.length === 0 && !selectedDirectory ? (
          <p className="agent-access-empty" role="status">
            Loading destinations…
          </p>
        ) : destinations.length === 0 && !selectedDirectory ? (
          <p className="agent-access-empty">No export destinations for this client and project.</p>
        ) : destinations.length > 0 ? (
          <div className="agent-destination-list">
            {destinations.map((destination) => (
              <article className="agent-destination-row" key={destination.destinationId}>
                <header>
                  <div>
                    <strong>{destination.label}</strong>
                    <span>
                      {!destination.ready
                        ? "Repair required"
                        : destination.enabled
                          ? "Ready"
                          : "Disabled"}
                    </span>
                  </div>
                  <small>rev {destination.revision}</small>
                </header>
                <p>
                  {destination.formats.map((format) => format.toUpperCase()).join(" + ")} · up to{" "}
                  {destination.maximumRowsPerPart.toLocaleString("en-US")} rows/part ·{" "}
                  {Math.round(destination.maximumTotalBytes / GIB)} GiB total
                </p>
                <div className="agent-host-actions">
                  <button
                    className="toolbar-button"
                    disabled={busy}
                    onClick={() => edit(destination)}
                    type="button"
                  >
                    Edit
                  </button>
                  {!destination.ready && (
                    <button
                      className="toolbar-button"
                      disabled={busy}
                      onClick={() => void chooseFolder(destination)}
                      type="button"
                    >
                      Repair
                    </button>
                  )}
                  <button
                    className="toolbar-button"
                    disabled={busy || (!destination.ready && !destination.enabled)}
                    onClick={() =>
                      void mutate(() =>
                        setAgentExportDestinationEnabled(
                          client.id,
                          project.id,
                          destination.destinationId,
                          !destination.enabled,
                        ),
                      )
                    }
                    type="button"
                  >
                    {destination.enabled ? "Disable" : "Enable"}
                  </button>
                  <button
                    aria-label={`Revoke ${destination.label}`}
                    className="icon-button"
                    disabled={busy}
                    onClick={() =>
                      void mutate(() =>
                        revokeAgentExportDestination(
                          client.id,
                          project.id,
                          destination.destinationId,
                        ),
                      )
                    }
                    type="button"
                  >
                    <TrashIcon aria-hidden="true" size={14} />
                  </button>
                </div>
              </article>
            ))}
          </div>
        ) : null}
      </Dialog>
    </>
  );
}
