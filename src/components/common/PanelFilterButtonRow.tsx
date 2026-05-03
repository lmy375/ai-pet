import type { CSSProperties } from "react";

/**
 * Iter R39 — extracted shared component for the "filter button row" pattern
 * codified in R37 / applied unmodified in R38. Pattern is now used on three
 * timelines (feedback / decision / tool_call), so the duplication crossed
 * the use-3 threshold to warrant abstraction (R32 IDEA "wait until use-3+
 * before extraction" — R39 hits it).
 *
 * Each option renders a button. Buttons share a consistent shape: count
 * embedded in label, active state colored to its accent, inactive white
 * with gray text/border. fontFamily inherits from the parent so the row
 * stays consistent in both monospace (decision_log) and sans-serif
 * (feedback) contexts (R38 IDEA "继承 styling 跟着 context 走").
 *
 * Generic on the value type so each timeline keeps its own narrow union
 * (e.g. `"all" | "Spoke" | "LlmSilent" | "Skip"`) without resorting to
 * stringly-typed.
 */
export interface PanelFilterOption<V extends string> {
  /** Filter discriminator — narrow string-union value the timeline keys on. */
  value: V;
  /** Visible label, count is appended automatically. */
  label: string;
  /** Count to render inside the button (e.g. how many entries match). */
  count: number;
  /** Background hex when active. Tied to the matching pill / kindColor in
   *  the timeline below so visual coupling stays consistent across rows. */
  accent: string;
  /** Optional tooltip on hover. */
  title?: string;
}

interface PanelFilterButtonRowProps<V extends string> {
  options: PanelFilterOption<V>[];
  active: V;
  onChange: (v: V) => void;
  /** Optional outer wrapper style — typically `{ marginBottom: "6px" }`
   *  for the decision-log mono context vs the feedback context's
   *  `paddingTop: "6px"`. */
  rowStyle?: CSSProperties;
}

export function PanelFilterButtonRow<V extends string>({
  options,
  active,
  onChange,
  rowStyle,
}: PanelFilterButtonRowProps<V>) {
  return (
    <div
      style={{
        display: "flex",
        gap: "6px",
        flexWrap: "wrap",
        fontFamily: "inherit",
        ...rowStyle,
      }}
    >
      {options.map((opt) => {
        const isActive = active === opt.value;
        const style: CSSProperties = {
          padding: "2px 8px",
          fontSize: "10px",
          borderRadius: "10px",
          border: `1px solid ${isActive ? opt.accent : "#cbd5e1"}`,
          background: isActive ? opt.accent : "#fff",
          color: isActive ? "#fff" : "#475569",
          cursor: "pointer",
          fontWeight: 600,
          fontFamily: "inherit",
        };
        return (
          <button
            key={opt.value}
            type="button"
            onClick={() => onChange(opt.value)}
            style={style}
            title={opt.title}
          >
            {opt.label} {opt.count}
          </button>
        );
      })}
    </div>
  );
}
