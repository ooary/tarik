import type { HTMLAttributes, ReactNode } from "react";

interface SurfaceProps extends HTMLAttributes<HTMLDivElement> {
  title?: string;
  actions?: ReactNode;
}

export function Surface({ title, actions, children, className = "", ...props }: SurfaceProps) {
  const headingId = title
    ? `surface-${title.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`
    : undefined;

  return (
    <section aria-labelledby={headingId} className={`ui-surface ${className}`.trim()} {...props}>
      {(title || actions) && (
        <header className="ui-surface-header">
          {title && <h2 id={headingId}>{title}</h2>}
          {actions}
        </header>
      )}
      <div className="ui-surface-body">{children}</div>
    </section>
  );
}
