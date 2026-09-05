import * as DialogPrimitive from "@radix-ui/react-dialog";
import { XIcon } from "@phosphor-icons/react";
import { useRef, type FormEvent, type ReactNode, type RefObject } from "react";
import { Field } from "./Field";

interface DialogProps {
  trigger?: ReactNode;
  title: string;
  description?: string;
  children: ReactNode;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  busy?: boolean;
  closeLabel?: string;
  contentClassName?: string;
  initialFocusRef?: RefObject<HTMLElement | null>;
  returnFocusRef?: RefObject<HTMLElement | null>;
}

export function Dialog({
  trigger,
  title,
  description,
  children,
  open,
  onOpenChange,
  busy = false,
  closeLabel = "Close dialog",
  contentClassName = "",
  initialFocusRef,
  returnFocusRef,
}: DialogProps) {
  const controlled = open === undefined ? {} : { open };

  return (
    <DialogPrimitive.Root
      {...controlled}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && busy) return;
        onOpenChange?.(nextOpen);
      }}
    >
      {trigger && <DialogPrimitive.Trigger asChild>{trigger}</DialogPrimitive.Trigger>}
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="ui-dialog-overlay" />
        <DialogPrimitive.Content
          className={`ui-dialog-content ${contentClassName}`.trim()}
          onEscapeKeyDown={(event) => {
            if (busy) event.preventDefault();
          }}
          onInteractOutside={(event) => {
            if (busy) event.preventDefault();
          }}
          onOpenAutoFocus={(event) => {
            if (!initialFocusRef?.current) return;
            event.preventDefault();
            initialFocusRef.current.focus();
          }}
          onCloseAutoFocus={(event) => {
            if (!returnFocusRef?.current) return;
            event.preventDefault();
            returnFocusRef.current.focus();
          }}
        >
          <header className="ui-dialog-header">
            <div>
              <DialogPrimitive.Title>{title}</DialogPrimitive.Title>
              {description && (
                <DialogPrimitive.Description>{description}</DialogPrimitive.Description>
              )}
            </div>
            <DialogPrimitive.Close aria-label={closeLabel} className="icon-button" disabled={busy}>
              <XIcon aria-hidden="true" size={16} weight="bold" />
            </DialogPrimitive.Close>
          </header>
          <div className="ui-dialog-body">{children}</div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

interface TextEntryDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: () => void | Promise<void>;
  title: string;
  description: string;
  label: string;
  value: string;
  onValueChange: (value: string) => void;
  submitLabel: string;
  busy?: boolean;
  hint?: string;
  validationError?: string;
  operationError?: string | null;
  children?: ReactNode;
  returnFocusRef?: RefObject<HTMLElement | null>;
}

export function TextEntryDialog({
  open,
  onOpenChange,
  onSubmit,
  title,
  description,
  label,
  value,
  onValueChange,
  submitLabel,
  busy = false,
  hint,
  validationError,
  operationError,
  children,
  returnFocusRef,
}: TextEntryDialogProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const valid = value.trim().length > 0 && !validationError;

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!valid || busy) return;
    void onSubmit();
  }

  return (
    <Dialog
      busy={busy}
      description={description}
      initialFocusRef={inputRef}
      onOpenChange={onOpenChange}
      open={open}
      returnFocusRef={returnFocusRef}
      title={title}
    >
      <form className="ui-dialog-form" onSubmit={submit}>
        <Field
          disabled={busy}
          error={validationError}
          hint={hint}
          label={label}
          onChange={(event) => onValueChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            if (valid && !busy) void onSubmit();
          }}
          ref={inputRef}
          value={value}
        />
        {children}
        {operationError && (
          <div className="ui-inline-error" role="alert">
            <strong>{title} failed</strong>
            <span>{operationError}</span>
          </div>
        )}
        <div className="ui-dialog-actions">
          <button
            className="toolbar-button"
            disabled={busy}
            onClick={() => onOpenChange(false)}
            type="button"
          >
            Cancel
          </button>
          <button className="run-button" disabled={!valid || busy} type="submit">
            {busy ? `${submitLabel}…` : submitLabel}
          </button>
        </div>
      </form>
    </Dialog>
  );
}

export type ConfirmationTone = "normal" | "warning" | "destructive";

interface ConfirmationDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void | Promise<void>;
  title: string;
  description: string;
  confirmLabel: string;
  busy?: boolean;
  error?: string | null;
  tone?: ConfirmationTone;
  detail?: ReactNode;
  returnFocusRef?: RefObject<HTMLElement | null>;
}

export function ConfirmationDialog({
  open,
  onOpenChange,
  onConfirm,
  title,
  description,
  confirmLabel,
  busy = false,
  error,
  tone = "normal",
  detail,
  returnFocusRef,
}: ConfirmationDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);

  return (
    <Dialog
      busy={busy}
      description={description}
      initialFocusRef={cancelRef}
      onOpenChange={onOpenChange}
      open={open}
      returnFocusRef={returnFocusRef}
      title={title}
    >
      <div className={`ui-confirmation ui-confirmation-${tone}`}>
        {detail && <div className="ui-confirmation-detail">{detail}</div>}
        {error && (
          <div className="ui-inline-error" role="alert">
            <strong>{title} failed</strong>
            <span>{error}</span>
          </div>
        )}
        <div className="ui-dialog-actions">
          <button
            className="toolbar-button"
            disabled={busy}
            onClick={() => onOpenChange(false)}
            ref={cancelRef}
            type="button"
          >
            Cancel
          </button>
          <button
            className={tone === "destructive" ? "danger-button" : "run-button"}
            disabled={busy}
            onClick={() => void onConfirm()}
            type="button"
          >
            {busy ? `${confirmLabel}…` : confirmLabel}
          </button>
        </div>
      </div>
    </Dialog>
  );
}
