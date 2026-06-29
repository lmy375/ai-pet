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

const actionSize = { sm: "h-6 w-6", md: "h-7 w-7" } as const;
const actionHover = {
  default: "hover:bg-slate-100 hover:text-slate-600",
  danger: "hover:bg-red-50 hover:text-red-500",
} as const;

/**
 * Small borderless square icon button for inline row actions (rename, delete).
 * `variant="danger"` turns the hover red. The single source for the per-row
 * icon-action styling repeated across the chat session list and settings.
 */
export function IconActionButton({
  variant = "default",
  size = "md",
  className = "",
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "default" | "danger";
  size?: "sm" | "md";
  children: ReactNode;
}) {
  return (
    <button
      className={`flex ${actionSize[size]} shrink-0 items-center justify-center rounded-md text-slate-400 transition-colors ${actionHover[variant]} ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}
