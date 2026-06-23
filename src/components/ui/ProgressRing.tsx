interface ProgressRingProps {
  /** Fill fraction, 0–1. Values outside the range are clamped. */
  value: number;
  /** Diameter in pixels. */
  size?: number;
  /** Native tooltip text. */
  title?: string;
  className?: string;
}

/**
 * A small circular progress ring drawn with two SVG arcs (a faint track plus a
 * progress arc via stroke-dasharray). Used in the chat header to show how full
 * the model's context window is. The arc shifts color as it fills: accent blue,
 * amber past 80%, red past 95% — a functional warning, not decoration.
 */
export function ProgressRing({ value, size = 18, title, className }: ProgressRingProps) {
  const v = Math.min(1, Math.max(0, value));
  const r = 9;
  const circumference = 2 * Math.PI * r;
  const dashoffset = circumference * (1 - v);
  const color = v >= 0.95 ? "text-red-500" : v >= 0.8 ? "text-amber-500" : "text-accent";

  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      className={`shrink-0 ${color} ${className ?? ""}`}
      role="img"
      aria-label={title}
    >
      {title && <title>{title}</title>}
      <circle cx="12" cy="12" r={r} stroke="currentColor" strokeWidth="2.5" strokeOpacity="0.15" />
      <circle
        cx="12"
        cy="12"
        r={r}
        stroke="currentColor"
        strokeWidth="2.5"
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={dashoffset}
        transform="rotate(-90 12 12)"
      />
    </svg>
  );
}
