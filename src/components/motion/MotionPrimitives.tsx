import {
  AnimatePresence,
  LayoutGroup,
  MotionConfig,
  motion,
  useReducedMotion,
  type HTMLMotionProps,
  type Variants,
} from "motion/react";
import type { ReactNode } from "react";

export type MotionDirection = "forward" | "backward" | "neutral";
export type MotionPlacement = "center" | "left" | "right" | "top" | "bottom";

export function MotionPresence({ children, mode = "sync" }: { children: ReactNode; mode?: "sync" | "wait" | "popLayout" }) {
  return <AnimatePresence initial={false} mode={mode}>{children}</AnimatePresence>;
}

type MotionProviderProps = { children: ReactNode };

const isJSDOM = typeof navigator !== "undefined" && /jsdom/i.test(navigator.userAgent);
const enableLayoutProjection = !isJSDOM;

/** App-wide motion defaults. MotionConfig also follows prefers-reduced-motion. */
export function MotionProvider({ children }: MotionProviderProps) {
  return (
    <MotionConfig reducedMotion="user" transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}>
      <LayoutGroup>{children}</LayoutGroup>
    </MotionConfig>
  );
}

const pageVariants: Variants = {
  initial: (direction: MotionDirection) => ({
    opacity: 0,
    y: direction === "neutral" ? 8 : 5,
    x: direction === "backward" ? -8 : direction === "forward" ? 8 : 0,
    scale: 0.995,
  }),
  animate: { opacity: 1, x: 0, y: 0, scale: 1 },
  exit: (direction: MotionDirection) => ({
    opacity: 0,
    y: 4,
    x: direction === "backward" ? 8 : direction === "forward" ? -8 : 0,
    scale: 0.995,
  }),
};

export function MotionPage({
  children,
  direction = "neutral",
  className,
  ...props
}: HTMLMotionProps<"div"> & { direction?: MotionDirection }) {
  return (
    <motion.div
      {...props}
      className={className ? `motion-page ${className}` : "motion-page"}
      custom={direction}
      variants={pageVariants}
      initial="initial"
      animate="animate"
      exit="exit"
    >
      {children}
    </motion.div>
  );
}

const placementOffset: Record<MotionPlacement, { x: number; y: number; scale: number }> = {
  center: { x: 0, y: 12, scale: 0.98 },
  left: { x: -18, y: 0, scale: 0.985 },
  right: { x: 18, y: 0, scale: 0.985 },
  top: { x: 0, y: -12, scale: 0.985 },
  bottom: { x: 0, y: 12, scale: 0.985 },
};

export type MotionOverlayProps = {
  open: boolean;
  onClose?: () => void;
  placement?: MotionPlacement;
  ariaLabel?: string;
  className?: string;
  children: ReactNode;
};

/** Overlay with a reversible, origin-aware entrance and exit. */
export function MotionOverlay({
  open,
  onClose,
  placement = "center",
  ariaLabel,
  className,
  children,
}: MotionOverlayProps) {
  const reducedMotion = useReducedMotion();
  const offset = placementOffset[placement];
  const contentInitial = reducedMotion ? { opacity: 0 } : { opacity: 0, ...offset };
  const contentExit = reducedMotion ? { opacity: 0 } : { opacity: 0, ...offset };
  const placementClassName = {
    center: "place-items-center",
    left: "place-items-center justify-items-start",
    right: "place-items-center justify-items-end",
    top: "place-items-start justify-items-center",
    bottom: "place-items-end justify-items-center",
  }[placement];

  return (
    <AnimatePresence>
      {open ? (
        <motion.div
          className={`motion-overlay motion-runtime-overlay fixed inset-0 z-[80] grid ${placementClassName} bg-stone-950/35 p-4 backdrop-blur-sm`}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          onClick={onClose}
          role="presentation"
        >
          <motion.div
            aria-label={ariaLabel}
            aria-modal={ariaLabel ? "true" : undefined}
            className={`motion-dialog ${className ?? "w-full max-w-lg rounded-2xl border border-stone-200 bg-white p-5 shadow-2xl"}`.trim()}
            initial={contentInitial}
            role={ariaLabel ? "dialog" : undefined}
            animate={{ opacity: 1, x: 0, y: 0, scale: 1 }}
            exit={contentExit}
            onClick={(event) => event.stopPropagation()}
          >
            {children}
          </motion.div>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

export const MotionDialog = MotionOverlay;

export type MotionMenuProps = {
  open: boolean;
  origin?: "top-left" | "top-right" | "bottom-left" | "bottom-right";
  className?: string;
  role?: "menu" | "dialog" | "presentation";
  ariaLabel?: string;
  children: ReactNode;
};

export function MotionMenu({
  open,
  origin = "top-right",
  className = "",
  role,
  ariaLabel,
  children,
}: MotionMenuProps) {
  const reducedMotion = useReducedMotion();
  const originOffset = origin.includes("left") ? -8 : 8;
  const yOffset = origin.startsWith("top") ? -4 : 4;
  return (
    <AnimatePresence>
      {open ? (
        <motion.div
          aria-label={ariaLabel}
          className={`motion-menu ${className}`.trim()}
          role={role}
          initial={reducedMotion ? { opacity: 0 } : { opacity: 0, x: originOffset, y: yOffset, scale: 0.98 }}
          animate={{ opacity: 1, x: 0, y: 0, scale: 1 }}
          exit={isJSDOM ? undefined : reducedMotion ? { opacity: 0 } : { opacity: 0, x: originOffset, y: yOffset, scale: 0.98 }}
          style={{ transformOrigin: origin.replace("-", " ") }}
        >
          {children}
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

export const MotionPopover = MotionMenu;

export function MotionCollapse({ open, children, className = "" }: { open: boolean; children: ReactNode; className?: string }) {
  const reducedMotion = useReducedMotion();
  return (
    <AnimatePresence initial={false}>
      {open ? (
        <motion.div
          className={`motion-collapse ${className}`.trim()}
          initial={reducedMotion || !enableLayoutProjection ? { opacity: 0 } : { height: 0, opacity: 0 }}
          animate={reducedMotion || !enableLayoutProjection ? { opacity: 1 } : { height: "auto", opacity: 1 }}
          exit={reducedMotion || !enableLayoutProjection ? { opacity: 0 } : { height: 0, opacity: 0 }}
          transition={{ duration: reducedMotion || !enableLayoutProjection ? 0.01 : 0.24, ease: [0.22, 1, 0.36, 1] }}
          style={{ overflow: "hidden" }}
        >
          {children}
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

export function MotionListItem({ itemKey, children, className = "" }: { itemKey: string; children: ReactNode; className?: string }) {
  return (
    <motion.div
      layout={enableLayoutProjection}
      key={itemKey}
      className={className}
      initial={{ opacity: 0, y: 8, scale: 0.99 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, y: -6, scale: 0.985 }}
      transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
    >
      {children}
    </motion.div>
  );
}

export function MotionNumber({ value, className = "" }: { value: string | number; className?: string }) {
  return (
    <AnimatePresence initial={false} mode="popLayout">
      <motion.span
        key={String(value)}
        className={`inline-block ${className}`.trim()}
        initial={{ opacity: 0, y: 5 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: -5 }}
        transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
      >
        {value}
      </motion.span>
    </AnimatePresence>
  );
}
