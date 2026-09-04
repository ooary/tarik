import { PlusIcon, TrashIcon, XIcon } from "@phosphor-icons/react";
import * as Dialog from "@radix-ui/react-dialog";
import { useState } from "react";
import {
  createTable,
  type CreateTableColumn,
  type CreateTableColumnType,
  type CreateTableDefinition,
} from "../../lib/commands";

const CREATE_TABLE_TYPES: CreateTableColumnType[] = [
  "VARCHAR",
  "BIGINT",
  "INTEGER",
  "DOUBLE",
  "DECIMAL",
  "BOOLEAN",
  "DATE",
  "TIMESTAMP",
];

const emptyColumn = (): CreateTableColumn => ({ name: "", dataType: "VARCHAR", nullable: true });

export function validateTableDefinition(definition: CreateTableDefinition): string | null {
  if (!definition.name.trim()) return "Enter a table name.";
  if (definition.columns.length === 0) return "Add at least one column.";
  const names = new Set<string>();
  for (const column of definition.columns) {
    const name = column.name.trim();
    if (!name) return "Every column needs a name.";
    const folded = name.toLocaleLowerCase();
    if (names.has(folded)) return `Column name “${name}” is duplicated.`;
    names.add(folded);
    if (!CREATE_TABLE_TYPES.includes(column.dataType)) return "Choose a supported column type.";
  }
  return null;
}

export function NewTableDialog({
  projectId,
  onCreated,
}: {
  projectId: string;
  onCreated: () => void | Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [columns, setColumns] = useState<CreateTableColumn[]>([emptyColumn()]);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  function reset() {
    setName("");
    setColumns([emptyColumn()]);
    setError(null);
  }

  function updateColumn(index: number, patch: Partial<CreateTableColumn>) {
    setColumns((current) =>
      current.map((column, columnIndex) =>
        columnIndex === index ? { ...column, ...patch } : column,
      ),
    );
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    const definition: CreateTableDefinition = {
      name: name.trim(),
      columns: columns.map((column) => ({ ...column, name: column.name.trim() })),
    };
    const validation = validateTableDefinition(definition);
    if (validation) {
      setError(validation);
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      await createTable(projectId, definition);
      await onCreated();
      setOpen(false);
      reset();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Dialog.Root
      onOpenChange={(next) => {
        setOpen(next);
        if (!next && !submitting) reset();
      }}
      open={open}
    >
      <Dialog.Trigger asChild>
        <button className="footer-action" disabled={!projectId} type="button">
          New table
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="ui-dialog-overlay" />
        <Dialog.Content className="ui-dialog-content new-table-dialog">
          <header className="ui-dialog-header">
            <div>
              <Dialog.Title>New table</Dialog.Title>
              <Dialog.Description>
                Create an empty DuckDB table, then insert or import rows with SQL.
              </Dialog.Description>
            </div>
            <Dialog.Close aria-label="Close dialog" className="icon-button" disabled={submitting}>
              <XIcon aria-hidden="true" size={16} weight="bold" />
            </Dialog.Close>
          </header>
          <form className="ui-dialog-body new-table-form" onSubmit={submit}>
            <div className="ui-field">
              <label className="ui-field-label" htmlFor="new-table-name">
                Table name
              </label>
              <span className="ui-field-control">
                <input
                  autoFocus
                  disabled={submitting}
                  id="new-table-name"
                  onChange={(event) => setName(event.target.value)}
                  placeholder="customers"
                  value={name}
                />
              </span>
              <span className="ui-field-hint">
                Spaces and reserved words are supported and safely quoted.
              </span>
            </div>

            <fieldset className="new-table-columns" disabled={submitting}>
              <legend>Columns</legend>
              {columns.map((column, index) => (
                <div className="new-table-column" key={index}>
                  <label>
                    <span>Name</span>
                    <input
                      aria-label={`Column ${index + 1} name`}
                      onChange={(event) => updateColumn(index, { name: event.target.value })}
                      placeholder={index === 0 ? "id" : "column_name"}
                      value={column.name}
                    />
                  </label>
                  <label>
                    <span>Type</span>
                    <select
                      aria-label={`Column ${index + 1} type`}
                      onChange={(event) =>
                        updateColumn(index, {
                          dataType: event.target.value as CreateTableColumnType,
                        })
                      }
                      value={column.dataType}
                    >
                      {CREATE_TABLE_TYPES.map((type) => (
                        <option key={type} value={type}>
                          {type}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="new-table-nullable">
                    <input
                      aria-label={`Column ${index + 1} allows NULL`}
                      checked={column.nullable}
                      onChange={(event) => updateColumn(index, { nullable: event.target.checked })}
                      type="checkbox"
                    />
                    <span>Allow NULL</span>
                  </label>
                  <button
                    aria-label={`Remove column ${index + 1}`}
                    className="icon-button"
                    disabled={columns.length === 1}
                    onClick={() =>
                      setColumns((current) =>
                        current.filter((_, columnIndex) => columnIndex !== index),
                      )
                    }
                    type="button"
                  >
                    <TrashIcon aria-hidden="true" size={15} />
                  </button>
                </div>
              ))}
              <button
                className="subtle-button new-table-add-column"
                onClick={() => setColumns((current) => [...current, emptyColumn()])}
                type="button"
              >
                <PlusIcon aria-hidden="true" size={14} weight="bold" /> Add column
              </button>
            </fieldset>

            {error && (
              <div className="ui-inline-error" role="alert">
                <strong>Table was not created</strong>
                <span>{error}</span>
              </div>
            )}
            <footer className="new-table-actions">
              <Dialog.Close className="text-button" disabled={submitting} type="button">
                Cancel
              </Dialog.Close>
              <button className="run-button" disabled={submitting} type="submit">
                {submitting ? "Creating table" : "Create table"}
              </button>
            </footer>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
