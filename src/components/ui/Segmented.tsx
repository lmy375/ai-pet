import type { ReactNode } from "react";

interface Option<T extends string> {
  value: T;
  label: ReactNode;
}

interface Props<T extends string> {
  value: T;
  /** Each option carries a stable `value` and a (translatable) display `label`. */
  options: readonly Option<T>[];
  onChange: (value: T) => void;
  className?: string;
}

/** iOS-style segmented control. */
export function Segmented<T extends string>({ value, options, onChange, className = "" }: Props<T>) {
  return (
    <div className={`inline-flex rounded-field bg-hover p-0.5 ${className}`}>
      {options.map((opt) => {
        const active = opt.value === value;
        return (
          <button
            key={opt.value}
            onClick={() => onChange(opt.value)}
            className={`rounded-[7px] px-4 py-1 text-body font-medium transition-colors ${
              active ? "bg-surface text-ink shadow-card" : "text-ink-soft hover:text-ink"
            }`}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}
