import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";

export type DragAxis = "x" | "y";
export type DragResizeLimit = number | (() => number);

export type DragResizeState = {
  startX: number;
  startY: number;
  startValue: number;
};

export type DragResizeOptions = {
  axis: DragAxis;
  min: DragResizeLimit;
  max: DragResizeLimit;
  getInitialValue: () => number;
  onChange: (value: number) => void;
  cursor?: string;
  getValueFromPointer?: (event: PointerEvent, state: DragResizeState) => number;
  onStart?: (state: DragResizeState, event: PointerEvent) => void;
  onEnd?: () => void;
};

type ActiveDrag = {
  state: DragResizeState;
  target: HTMLElement | null;
  pointerId: number;
  handlePointerMove: (event: PointerEvent) => void;
  stopDragging: () => void;
};

const resolveLimit = (value: DragResizeLimit) => (typeof value === "function" ? value() : value);

const clamp = (value: number, min: number, max: number) => {
  const lower = Math.min(min, max);
  const upper = Math.max(min, max);
  return Math.min(Math.max(value, lower), upper);
};

export function useDragResize(options: DragResizeOptions) {
  const [dragging, setDragging] = useState(false);
  const optionsRef = useRef(options);
  const activeDragRef = useRef<ActiveDrag | null>(null);
  const previousCursorRef = useRef("");

  optionsRef.current = options;

  const stopDragging = useCallback(() => {
    const activeDrag = activeDragRef.current;
    if (!activeDrag) {
      return;
    }

    activeDragRef.current = null;
    document.body.style.cursor = previousCursorRef.current;
    document.removeEventListener("pointermove", activeDrag.handlePointerMove);
    document.removeEventListener("pointerup", activeDrag.stopDragging);
    document.removeEventListener("pointercancel", activeDrag.stopDragging);
    document.removeEventListener("lostpointercapture", activeDrag.stopDragging, true);

    if (activeDrag.target?.hasPointerCapture?.(activeDrag.pointerId)) {
      try {
        activeDrag.target.releasePointerCapture?.(activeDrag.pointerId);
      } catch {
        // Pointer capture may already be released by the browser.
      }
    }

    setDragging(false);
    optionsRef.current.onEnd?.();
  }, []);

  const startDragging = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      const nativeEvent = event.nativeEvent;
      if (nativeEvent.button !== 0 || activeDragRef.current) {
        return;
      }

      const state: DragResizeState = {
        startX: nativeEvent.clientX,
        startY: nativeEvent.clientY,
        startValue: optionsRef.current.getInitialValue(),
      };
      const target = event.currentTarget;
      const pointerId = nativeEvent.pointerId;

      const handlePointerMove = (moveEvent: PointerEvent) => {
        const currentOptions = optionsRef.current;
        const nextValue = currentOptions.getValueFromPointer
          ? currentOptions.getValueFromPointer(moveEvent, state)
          : state.startValue +
            (currentOptions.axis === "x"
              ? moveEvent.clientX - state.startX
              : moveEvent.clientY - state.startY);

        currentOptions.onChange(
          clamp(
            nextValue,
            resolveLimit(currentOptions.min),
            resolveLimit(currentOptions.max),
          ),
        );
      };

      const activeDrag: ActiveDrag = {
        state,
        target,
        pointerId,
        handlePointerMove,
        stopDragging,
      };
      activeDragRef.current = activeDrag;
      previousCursorRef.current = document.body.style.cursor;
      document.body.style.cursor = optionsRef.current.cursor ?? (optionsRef.current.axis === "x" ? "col-resize" : "row-resize");
      document.addEventListener("pointermove", handlePointerMove);
      document.addEventListener("pointerup", stopDragging);
      document.addEventListener("pointercancel", stopDragging);
      document.addEventListener("lostpointercapture", stopDragging, true);

      try {
        target.setPointerCapture?.(pointerId);
      } catch {
        // Pointer capture is not available in every host, including jsdom.
      }

      setDragging(true);
      optionsRef.current.onStart?.(state, nativeEvent);
      event.preventDefault();
    },
    [stopDragging],
  );

  useEffect(() => stopDragging, [stopDragging]);

  return {
    dragging,
    startDragging,
  };
}
