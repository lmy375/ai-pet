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
