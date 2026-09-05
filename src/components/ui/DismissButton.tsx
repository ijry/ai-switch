import { clsx } from "clsx";
import { X } from "lucide-react";

type DismissButtonProps = {
  ariaLabel: string;
  className?: string;
  onClick: () => void;
  /** `sm` is for the status bar, where the row is only 8 units tall. */
  size?: "sm" | "md";
  title?: string;
};

/**
 * The × on a notification banner.
 *
 * `text-current` plus a translucent white hover lets one button sit on every
 * tint the banners use — emerald, red, violet, teal, stone — without a variant
 * per colour. The size comes from a prop rather than an override in `className`
 * because UnoCSS decides the order of the generated rules, so two competing
 * `h-*` classes would not reliably resolve in the caller's favour.
 */
export function DismissButton({
  ariaLabel,
  className,
  onClick,
  size = "md",
  title,
}: DismissButtonProps) {
  return (
    <button
      aria-label={ariaLabel}
      className={clsx(
        "grid shrink-0 place-items-center rounded-lg text-current opacity-70 motion-control hover:bg-white/70 hover:opacity-100",
        size === "md" && "h-7 w-7",
        size === "sm" && "h-5 w-5",
        className,
      )}
      onClick={onClick}
      title={title ?? ariaLabel}
      type="button"
    >
      <X aria-hidden="true" className={size === "sm" ? "h-3 w-3" : "h-3.5 w-3.5"} />
    </button>
  );
}
