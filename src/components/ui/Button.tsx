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
  sm: "px-3 py-1.5 text-note rounded-field",
  md: "px-4 py-2 text-body rounded-field",
};

const variants: Record<Variant, string> = {
  primary: "bg-accent text-white shadow-card hover:bg-accent-hover",
  secondary: "bg-surface-soft text-ink hover:bg-hover",
  ghost: "border border-line bg-surface text-ink-soft hover:bg-hover hover:text-ink",
  danger: "bg-red-500 text-white hover:bg-red-600",
};

export function Button({ variant = "primary", size = "md", className = "", ...rest }: Props) {
  return <button className={`${base} ${sizes[size]} ${variants[variant]} ${className}`} {...rest} />;
}
