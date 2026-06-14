import type { ButtonHTMLAttributes } from "react";

type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md";

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
}

const base =
  "inline-flex items-center justify-center gap-1.5 font-medium whitespace-nowrap transition-colors disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none";

const sizes: Record<Size, string> = {
  sm: "px-3 py-1.5 text-[12px] rounded-lg",
  md: "px-4 py-2 text-[13px] rounded-xl",
};

const variants: Record<Variant, string> = {
  primary: "bg-accent text-white hover:bg-accent-hover",
  secondary: "bg-slate-100 text-slate-700 hover:bg-slate-200",
  ghost: "border border-slate-300/70 bg-white text-slate-600 hover:bg-slate-50",
  danger: "bg-red-500 text-white hover:bg-red-600",
};

export function Button({ variant = "primary", size = "md", className = "", ...rest }: Props) {
  return <button className={`${base} ${sizes[size]} ${variants[variant]} ${className}`} {...rest} />;
}
