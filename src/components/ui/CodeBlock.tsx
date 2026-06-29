import type { ReactNode } from "react";

/** Shared style for monospace output blocks (tool args/results, task output,
 *  LLM-log content). The single source of truth — was duplicated across the
 *  chat thread, task panel and LLM log view. */
export const codeBlockClass =
  "max-h-[260px] overflow-y-auto whitespace-pre-wrap break-all rounded-lg border border-slate-100 bg-slate-50 px-2.5 py-2 font-mono text-[12px] leading-relaxed text-slate-700";

/** A `<pre>` styled with {@link codeBlockClass}. Pass extra classes (margins,
 *  width) via `className`. */
export function CodeBlock({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <pre className={`${codeBlockClass} ${className}`}>{children}</pre>;
}
