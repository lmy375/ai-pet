import type { ReactNode } from "react";
import { useI18n } from "../../i18n";

/** Inline red alert box for inline error text (MCP/Telegram connection errors). */
export function ErrorBox({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <div className={`rounded-lg border border-red-300 bg-red-50 px-2.5 py-1.5 text-[12px] text-red-600 ${className}`}>
      {children}
    </div>
  );
}

/** Centered muted "loading…" filler that fills its parent. Used while a panel's
 *  initial data is in flight. Defaults to the shared `common.loading` string. */
export function LoadingScreen({ message }: { message?: string }) {
  const { t } = useI18n();
  return (
    <div className="flex h-full items-center justify-center text-[14px] text-slate-400">
      {message ?? t("common.loading")}
    </div>
  );
}

/** Small muted helper text shown under a form field. */
export function HintText({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <p className={`mt-1 text-[11px] text-slate-400 ${className}`}>{children}</p>;
}
