import type { ButtonHTMLAttributes } from "react";
import { clsx } from "clsx";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary";
};

export function Button({ className, variant = "primary", ...props }: ButtonProps) {
  return (
    <button
      className={clsx(
        "rounded-full px-4 py-2 text-sm font-semibold transition",
        variant === "primary" && "bg-ink text-paper hover:bg-ink/90",
        variant === "secondary" && "border border-ink/15 bg-white/70 text-ink hover:bg-white",
        className,
      )}
      {...props}
    />
  );
}
