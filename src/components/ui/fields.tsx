import type {
  InputHTMLAttributes,
  TextareaHTMLAttributes,
  SelectHTMLAttributes,
  ReactNode,
} from "react";

/** Shared field classes — the single source of truth for input styling. */
export const inputClass =
  "w-full rounded-xl border border-slate-200 bg-white px-3 py-2 text-[13px] text-slate-800 outline-none transition-colors focus:border-accent placeholder:text-slate-400 disabled:bg-slate-50 disabled:text-slate-400";

export const labelClass = "mb-1 block text-[12px] font-medium text-slate-500";

export function Label({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <label className={`${labelClass} ${className}`}>{children}</label>;
}

export function TextInput({ className = "", ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={`${inputClass} ${className}`} {...rest} />;
}

export function TextArea({ className = "", ...rest }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea className={`${inputClass} resize-y leading-relaxed ${className}`} {...rest} />;
}

export function Select({ className = "", ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select className={`${inputClass} ${className}`} {...rest} />;
}

/**
 * A `TextInput` that commits on blur and on Enter (Enter blurs the field, which
 * triggers the blur commit). The settings panel saves on commit, so this bakes
 * in the `onBlur` + "Enter blurs" pair every saved field repeats. Supply
 * `onChange` to update local state per keystroke and `onCommit` to persist.
 */
type SavedTextInputProps = Omit<InputHTMLAttributes<HTMLInputElement>, "onBlur" | "onKeyDown"> & {
  onCommit: () => void;
};
export function SavedTextInput({ onCommit, ...rest }: SavedTextInputProps) {
  return (
    <TextInput
      {...rest}
      onBlur={() => onCommit()}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
      }}
    />
  );
}

/**
 * A numeric `SavedTextInput`. `onChange` receives the raw entered number on each
 * keystroke; `onCommit` receives the value clamped to `[min, ∞)` (falling back
 * to `fallback` when the field is empty/0) on blur or Enter.
 */
type NumberFieldProps = Omit<
  InputHTMLAttributes<HTMLInputElement>,
  "onChange" | "onBlur" | "onKeyDown" | "value" | "type"
> & {
  value: number;
  min?: number;
  fallback: number;
  onChange: (v: number) => void;
  onCommit: (clamped: number) => void;
};
export function NumberField({ value, min = 1, fallback, onChange, onCommit, ...rest }: NumberFieldProps) {
  return (
    <TextInput
      type="number"
      min={min}
      value={value}
      onChange={(e) => onChange(Number(e.target.value) || 0)}
      onBlur={() => onCommit(Math.max(min, value || fallback))}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
      }}
      {...rest}
    />
  );
}
