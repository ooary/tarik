import type { InputHTMLAttributes, ReactNode } from "react";

interface FieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label: string;
  hint?: string;
  error?: string;
  trailing?: ReactNode;
}

export function Field({ label, hint, error, trailing, id, className = "", ...props }: FieldProps) {
  const inputId = id ?? `field-${label.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`;
  const descriptionId = `${inputId}-description`;

  return (
    <div className={`ui-field ${className}`.trim()}>
      <label className="ui-field-label" htmlFor={inputId}>
        {label}
      </label>
      <span className="ui-field-control">
        <input
          aria-describedby={hint || error ? descriptionId : undefined}
          aria-invalid={error ? true : undefined}
          id={inputId}
          {...props}
        />
        {trailing}
      </span>
      {(hint || error) && (
        <span className={error ? "ui-field-error" : "ui-field-hint"} id={descriptionId}>
          {error ?? hint}
        </span>
      )}
    </div>
  );
}
