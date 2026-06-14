import type { ReactNode } from "react";

export type BadgeColor = "sky" | "green" | "purple" | "orange" | "amber" | "slate";

const colors: Record<BadgeColor, string> = {
  sky: "bg-sky-100 text-sky-700",
  green: "bg-green-100 text-green-700",
  purple: "bg-purple-100 text-purple-700",
  orange: "bg-orange-100 text-orange-700",
  amber: "bg-amber-100 text-amber-700",
  slate: "bg-slate-100 text-slate-600",
};

interface Props {
  color?: BadgeColor;
  children: ReactNode;
  className?: string;
}

export function Badge({ color = "slate", children, className = "" }: Props) {
  return (
    <span
      className={`inline-flex items-center gap-1 whitespace-nowrap rounded px-2 py-0.5 text-[11px] font-semibold ${colors[color]} ${className}`}
    >
      {children}
    </span>
  );
}
