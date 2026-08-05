import { render, screen } from "@testing-library/react";
import { act, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { useDragResize } from "../src/lib/useDragResize";

function dispatchPointerEvent(
  target: EventTarget,
  type: "pointerdown" | "pointermove" | "pointerup",
  {
    button = 0,
    clientX = 0,
    clientY = 0,
    pointerId = 1,
  }: { button?: number; clientX?: number; clientY?: number; pointerId?: number } = {},
) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperties(event, {
    button: { configurable: true, value: button },
    clientX: { configurable: true, value: clientX },
    clientY: { configurable: true, value: clientY },
    pointerId: { configurable: true, value: pointerId },
  });
  act(() => {
    target.dispatchEvent(event);
  });
}

function DragProbe({
  initial = 100,
  min = 80,
  max = 220,
  onEnd,
}: {
  initial?: number;
  min?: number;
  max?: number;
  onEnd?: () => void;
}) {
  const [value, setValue] = useState(initial);
  const { dragging, startDragging } = useDragResize({
    axis: "x",
    min,
    max,
    getInitialValue: () => value,
    onChange: setValue,
    onEnd,
  });

  return (
    <>
      <button data-testid="drag-handle" onPointerDown={startDragging} type="button">
        resize
      </button>
      <output data-testid="value">{value}</output>
      <output data-testid="dragging">{String(dragging)}</output>
    </>
  );
}

describe("useDragResize", () => {
  it("updates the value from pointer movement and restores the drag state", () => {
    const onEnd = vi.fn();
    render(<DragProbe onEnd={onEnd} />);
    const handle = screen.getByTestId("drag-handle");

    dispatchPointerEvent(handle, "pointerdown", { clientX: 100, clientY: 20, pointerId: 7 });
    expect(screen.getByTestId("dragging")).toHaveTextContent("true");

    dispatchPointerEvent(document, "pointermove", { clientX: 160, clientY: 20, pointerId: 7 });
    expect(screen.getByTestId("value")).toHaveTextContent("160");

    dispatchPointerEvent(document, "pointerup", { pointerId: 7 });
    expect(screen.getByTestId("dragging")).toHaveTextContent("false");
    expect(onEnd).toHaveBeenCalledTimes(1);
  });

  it("clamps pointer movement to the configured limits", () => {
    render(<DragProbe initial={150} min={120} max={180} />);
    const handle = screen.getByTestId("drag-handle");

    dispatchPointerEvent(handle, "pointerdown", { clientX: 100, pointerId: 1 });
    dispatchPointerEvent(document, "pointermove", { clientX: 20, pointerId: 1 });
    expect(screen.getByTestId("value")).toHaveTextContent("120");

    dispatchPointerEvent(document, "pointermove", { clientX: 260, pointerId: 1 });
    expect(screen.getByTestId("value")).toHaveTextContent("180");
  });

  it("ignores non-primary pointer buttons", () => {
    render(<DragProbe />);
    const handle = screen.getByTestId("drag-handle");

    dispatchPointerEvent(handle, "pointerdown", { button: 2, clientX: 100, pointerId: 3 });
    dispatchPointerEvent(document, "pointermove", { clientX: 160, pointerId: 3 });

    expect(screen.getByTestId("value")).toHaveTextContent("100");
    expect(screen.getByTestId("dragging")).toHaveTextContent("false");
  });

  it("cleans up the active drag when the component unmounts", () => {
    const onEnd = vi.fn();
    const { unmount } = render(<DragProbe onEnd={onEnd} />);
    dispatchPointerEvent(screen.getByTestId("drag-handle"), "pointerdown", {
      clientX: 100,
      pointerId: 4,
    });

    unmount();
    dispatchPointerEvent(document, "pointermove", { clientX: 160, pointerId: 4 });

    expect(onEnd).toHaveBeenCalledTimes(1);
  });
});
