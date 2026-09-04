import type { ButtonHTMLAttributes } from "react";
import { clsx } from "clsx";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary";
};

export function Button({ className, variant = "primary", ...props }: ButtonProps) {
  return (
    <button
      className={clsx(
        "inline-flex items-center justify-center gap-2 rounded-lg border px-3 py-2 text-[13px] font-semibold shadow-sm motion-control focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400 disabled:cursor-not-allowed disabled:opacity-50",
        variant === "primary" && "border-slate-900 bg-slate-900 text-white hover:border-slate-800 hover:bg-slate-800",
        variant === "secondary" && "border-slate-200 bg-white text-slate-700 hover:border-slate-300 hover:bg-slate-50",
        className,
      )}
      {...props}
    />
  );
}
