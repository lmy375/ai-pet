import type { ButtonHTMLAttributes, ReactNode } from "react";

const base =
  "flex h-9 w-9 items-center justify-center rounded-xl border backdrop-blur-md transition-colors";
const idle = "border-slate-300/60 bg-white/80 text-slate-600 hover:bg-white";
const active = "border-accent bg-accent text-white";

/**
 * Square glassy icon button used by the pet-window overlays (pin, settings,
 * collapse). `active` swaps the idle white look for the accent fill. Pass
 * positioning/extra classes via `className`.
 */
export function FloatingIconButton({
  active: isActive = false,
  className = "",
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { active?: boolean; children: ReactNode }) {
  return (
    <button className={`${base} ${isActive ? active : idle} ${className}`} {...rest}>
      {children}
    </button>
  );
}
