interface Props<T extends string> {
  value: T;
  options: readonly T[];
  onChange: (value: T) => void;
  className?: string;
}

/** iOS-style segmented control. */
export function Segmented<T extends string>({ value, options, onChange, className = "" }: Props<T>) {
  return (
    <div className={`inline-flex rounded-lg bg-slate-200/70 p-0.5 ${className}`}>
      {options.map((opt) => {
        const active = opt === value;
        return (
          <button
            key={opt}
            onClick={() => onChange(opt)}
            className={`rounded-md px-4 py-1 text-[13px] font-medium transition-colors ${
              active ? "bg-white text-slate-800 shadow-sm" : "text-slate-500 hover:text-slate-700"
            }`}
          >
            {opt}
          </button>
        );
      })}
    </div>
  );
}
