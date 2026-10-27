import type { ButtonHTMLAttributes } from "react";

type Variant = "primary" | "secondary" | "ghost";
type Size = "sm" | "md" | "lg";

const base =
  "inline-flex items-center justify-center gap-2 rounded-default font-display font-semibold text-xs uppercase " +
  "transition-[filter,box-shadow,background-color,border-color,color,letter-spacing] duration-(--duration-base) ease-(--ease-classical) " +
  "disabled:opacity-50 disabled:pointer-events-none cursor-pointer";

const variants: Record<Variant, string> = {
  // Brass: dark text on the metallic gradient, with a light emboss below the letters.
  primary:
    "tracking-[0.15em] text-accent-foreground [text-shadow:0_1px_0_rgb(255_255_255/0.25)] " +
    "bg-linear-to-b from-accent-bright via-accent to-accent-deep shadow-(--shadow-engraved) " +
    "hover:brightness-110 hover:shadow-[var(--shadow-engraved),var(--shadow-brass-glow)] " +
    "active:brightness-95 active:shadow-[inset_0_2px_6px_rgb(0_0_0/0.35)]",
  // Outlined brass that turns crimson on hover.
  secondary:
    "engraved tracking-[0.15em] border-2 border-accent text-accent bg-transparent " +
    "hover:border-accent-secondary hover:bg-accent-secondary hover:text-foreground",
  ghost:
    "tracking-[0.15em] text-accent underline-offset-4 decoration-1 hover:text-accent-bright hover:underline hover:tracking-[0.2em]",
};

const sizes: Record<Size, string> = {
  sm: "h-10 px-6",
  md: "h-12 px-8",
  lg: "h-14 px-10",
};

type Props = ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: Size };

export function Button({ variant = "primary", size = "md", className = "", type = "button", ...rest }: Props) {
  const sizing = variant === "ghost" ? "h-10 px-2" : sizes[size];
  return <button type={type} className={`${base} ${variants[variant]} ${sizing} ${className}`} {...rest} />;
}
