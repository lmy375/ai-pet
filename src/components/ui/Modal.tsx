import { useEffect, type ReactNode } from "react";

/**
 * Centered dialog card over the dimmed app. Replaces the hand-written backdrop
 * + card wrapper that each dialog used to repeat, and gives every dialog the
 * same Esc-to-close behavior (previously only the task drawer had it).
 */
export function Modal({
  title,
  footer,
  maxWidth = 480,
  onClose,
  children,
}: {
  title: ReactNode;
  footer?: ReactNode;
  maxWidth?: number;
  onClose: () => void;
  children: ReactNode;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/25 px-4">
      <div className="w-full rounded-card border border-line bg-surface p-5 shadow-pop" style={{ maxWidth }}>
        <div className="flex items-center justify-between gap-2 text-title font-semibold text-ink">{title}</div>
        {children}
        {footer && <div className="mt-5 flex items-center justify-end gap-2">{footer}</div>}
      </div>
    </div>
  );
}