import type { ReactNode } from "react";

/**
 * macOS-style settings building blocks, shared by every settings screen.
 *
 * The look: a page of grouped white cards. Each `SettingsSection` is one card;
 * its children are `SettingsRow`s separated by hairlines (not nested cards, not
 * per-field boxes). A row either puts a compact control on the right of its
 * label (`control`) or stacks full-width content below it (`children`).
 */

interface SectionProps {
  title?: ReactNode;
  description?: ReactNode;
  /** Rendered on the right of the section header (e.g. an action button). */
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}

/** One titled white card. Rows inside are separated by hairlines. */
export function SettingsSection({ title, description, action, children, className = "" }: SectionProps) {
  return (
    <section className={`mb-6 ${className}`}>
      {(title || description || action) && (
        <div className="mb-2 flex items-end justify-between gap-3 px-1">
          <div className="min-w-0">
            {title && <h3 className="text-note font-semibold text-ink-soft">{title}</h3>}
            {description && <p className="mt-0.5 text-meta leading-relaxed text-ink-faint">{description}</p>}
          </div>
          {action && <div className="shrink-0">{action}</div>}
        </div>
      )}
      <div className="overflow-hidden rounded-card border border-line bg-surface shadow-card">{children}</div>
    </section>
  );
}

interface RowProps {
  label?: ReactNode;
  description?: ReactNode;
  /** A compact control shown on the right of the label (switches, short selects). */
  control?: ReactNode;
  /** Full-width content shown below the label when there is no `control`. */
  children?: ReactNode;
  /** Top-align label and control (for tall controls). Defaults to centered. */
  align?: "center" | "start";
  className?: string;
}

/** One row inside a `SettingsSection`. */
export function SettingsRow({ label, description, control, children, align = "center", className = "" }: RowProps) {
  const base = `border-b border-line/70 px-4 py-3.5 last:border-b-0 ${className}`;
  const alignCls = align === "start" ? "items-start" : "items-center";

  if (control !== undefined) {
    return (
      <div className={`flex gap-4 ${alignCls} ${base}`}>
        <div className="min-w-0 flex-1">
          {label && <div className="text-body font-medium text-ink">{label}</div>}
          {description && <p className="mt-0.5 text-note leading-relaxed text-ink-soft">{description}</p>}
        </div>
        <div className="shrink-0">{control}</div>
      </div>
    );
  }

  return (
    <div className={base}>
      {label && <div className="text-body font-medium text-ink">{label}</div>}
      {description && <p className="mt-0.5 text-note leading-relaxed text-ink-soft">{description}</p>}
      {children && <div className="mt-2">{children}</div>}
    </div>
  );
}

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  title?: string;
}

/** iOS-style toggle. Uses the app accent for the "on" track. */
export function Switch({ checked, onChange, disabled = false, title }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      title={title}
      onClick={() => onChange(!checked)}
      className={`relative inline-flex h-[26px] w-[44px] shrink-0 items-center rounded-full transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
        checked ? "bg-accent" : "bg-line"
      }`}
    >
      <span
        className={`pointer-events-none inline-block h-[22px] w-[22px] transform rounded-full bg-white shadow transition-transform ${
          checked ? "translate-x-[20px]" : "translate-x-[2px]"
        }`}
      />
    </button>
  );
}

