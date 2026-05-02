import type { CSSProperties } from "react";

/**
 * Compact labeled <input type="number"> used in settings forms. Styles are passed in by
 * the caller because the two settings panels (small modal / full panel window) use
 * different label and input visual treatments — sharing this widget lets the input-handling
 * logic (NaN guard, min clamp) live in one place without forcing a unified look.
 */
interface NumberFieldProps {
  label: string;
  value: number;
  min?: number;
  onChange: (v: number) => void;
  labelStyle?: CSSProperties;
  inputStyle?: CSSProperties;
}

export function NumberField({
  label,
  value,
  min,
  onChange,
  labelStyle,
  inputStyle,
}: NumberFieldProps) {
  return (
    <div style={{ flex: 1 }}>
      <label style={labelStyle}>{label}</label>
      <input
        type="number"
        value={value}
        min={min}
        onChange={(e) => {
          const n = Number(e.target.value);
          if (!Number.isNaN(n)) onChange(n);
        }}
        style={inputStyle}
      />
    </div>
  );
}
