import { XIcon } from "@phosphor-icons/react";
import * as Dialog from "@radix-ui/react-dialog";
import { useState, type FormEvent } from "react";
import { Button, Field } from "../../components/ui";

interface NewProjectDialogProps {
  existingNames: string[];
  onCreate: (name: string) => Promise<void>;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function NewProjectDialog({ existingNames, onCreate }: NewProjectDialogProps) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("Local analysis");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const trimmedName = name.trim();
  const duplicate = existingNames.some(
    (existingName) => existingName.trim().toLocaleLowerCase() === trimmedName.toLocaleLowerCase(),
  );

  const changeOpen = (nextOpen: boolean) => {
    if (submitting) return;
    setOpen(nextOpen);
    if (nextOpen) {
      setName("Local analysis");
      setError(null);
    }
  };

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!trimmedName || duplicate || submitting) return;
    setSubmitting(true);
    setError(null);
    try {
      await onCreate(trimmedName);
      setOpen(false);
    } catch (submitError) {
      setError(errorMessage(submitError));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Dialog.Root onOpenChange={changeOpen} open={open}>
      <Dialog.Trigger asChild>
        <button className="text-button project-new-button" type="button">
          New project
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="ui-dialog-overlay" />
        <Dialog.Content className="ui-dialog-content new-project-dialog">
          <header className="ui-dialog-header">
            <div>
              <Dialog.Title>New project</Dialog.Title>
              <Dialog.Description>
                Create a managed DuckDB project stored in Tarik application data.
              </Dialog.Description>
            </div>
            <Dialog.Close
              aria-label="Close new project"
              className="icon-button"
              disabled={submitting}
            >
              <XIcon aria-hidden="true" size={16} weight="bold" />
            </Dialog.Close>
          </header>
          <form className="new-project-form" onSubmit={submit}>
            <Field
              autoFocus
              error={
                error ??
                (duplicate
                  ? "A project with this name already exists in Recent projects."
                  : undefined)
              }
              hint={
                error
                  ? undefined
                  : "Tarik manages the database file and keeps it available in Recent projects."
              }
              label="Project name"
              maxLength={120}
              onChange={(event) => {
                setName(event.target.value);
                if (error) setError(null);
              }}
              required
              value={name}
            />
            <footer className="new-project-actions">
              <Dialog.Close asChild>
                <Button disabled={submitting} type="button">
                  Cancel
                </Button>
              </Dialog.Close>
              <Button
                disabled={!trimmedName || duplicate || submitting}
                tone="primary"
                type="submit"
              >
                {submitting ? "Creating project…" : "Create project"}
              </Button>
            </footer>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
