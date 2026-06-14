import type { ReactNode } from "react";

/** Inline success/error status text — green when ok, red otherwise. */
export function StatusText({ ok, children, className = "" }: { ok: boolean; children: ReactNode; className?: string }) {
  return <div className={`${ok ? "text-green-600" : "text-red-500"} ${className}`}>{children}</div>;
}
