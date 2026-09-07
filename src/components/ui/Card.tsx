import type { ReactNode } from "react";

interface Props {
  title?: ReactNode;
  /** Optional content rendered on the right side of the title row (e.g. status, actions). */
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}

/** iOS-style grouped section: a title above a rounded white card. */
export function Card({ title, action, children, className = "" }: Props) {
  return (
    <section className="mb-5">
      {(title || action) && (
        <div className="mb-2 flex items-center justify-between px-1">
          {title && <h4 className="text-body font-semibold text-ink">{title}</h4>}
          {action}
        </div>
      )}
      <div className={`rounded-card border border-line bg-surface p-4 ${className}`}>
        {children}
      </div>
    </section>
  );
}
