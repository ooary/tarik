import type { ReactNode } from "react";

export function InlineError({ children }: { children: ReactNode }) {
  return (
    <div className="ui-inline-error" role="alert">
      <strong>Unable to continue</strong>
      <span>{children}</span>
    </div>
  );
}

export function EmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="ui-empty-state">
      <strong>{title}</strong>
      <span>{description}</span>
      {action}
    </div>
  );
}

export function Skeleton({ label = "Loading", lines = 3 }: { label?: string; lines?: number }) {
  return (
    <div aria-label={label} aria-live="polite" className="ui-skeleton" role="status">
      {Array.from({ length: lines }, (_, index) => (
        <span key={index} style={{ width: `${88 - index * 11}%` }} />
      ))}
    </div>
  );
}
