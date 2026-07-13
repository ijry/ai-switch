import type { ButtonHTMLAttributes } from "react";
import { clsx } from "clsx";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary";
};

export function Button({ className, variant = "primary", ...props }: ButtonProps) {
  return (
    <button
      className={clsx(
        "rounded-xl px-3 py-2 text-[13px] font-semibold transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400",
        variant === "primary" && "bg-stone-900 text-white hover:bg-stone-800",
        variant === "secondary" && "border border-stone-200 bg-white/80 text-stone-800 hover:bg-white",
        className,
      )}
      {...props}
    />
  );
}
