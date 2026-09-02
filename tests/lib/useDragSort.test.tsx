import { render, screen } from "@testing-library/react";
import { act } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useDragSort } from "../../src/lib/useDragSort";

// 40px rows with the 2px gap the account list renders between them.
const ROW_HEIGHT = 40;
const ROW_PITCH = 42;

function stubRects() {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
    this: HTMLElement,
  ) {
    const top = Number(this.dataset.top ?? 0);
    const height = Number(this.dataset.height ?? 0);
    return {
      top,
      bottom: top + height,
      height,
      left: 0,
      right: 200,
      width: 200,
      x: 0,
      y: top,
      toJSON: () => ({}),
    } as DOMRect;
  });
}

function dispatchPointerEvent(
  target: EventTarget,
  type: "pointerdown" | "pointermove" | "pointerup" | "pointercancel",
  { button = 0, clientX = 0, clientY = 0, pointerId = 1 } = {},
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

function dispatchKeyDown(key: string) {
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key }));
  });
}

// jsdom does no layout, so a scroll container has to be faked outright.
function scrollContainerStub({
  scrollTop,
  scrollHeight,
  clientHeight = 400,
}: {
  scrollTop: number;
  scrollHeight: number;
  clientHeight?: number;
}) {
  const container = document.createElement("div");
  Object.defineProperty(container, "scrollTop", { value: scrollTop, writable: true });
  Object.defineProperty(container, "scrollHeight", { value: scrollHeight });
  Object.defineProperty(container, "clientHeight", { value: clientHeight });
  container.getBoundingClientRect = () =>
    ({
      top: 0,
      bottom: clientHeight,
      height: clientHeight,
      left: 0,
      right: 200,
      width: 200,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    }) as DOMRect;
  return container;
}

function SortProbe({
  ids = ["a", "b", "c"],
  onCommit = () => {},
  onEdgeHold,
  scrollContainer,
}: {
  ids?: string[];
  onCommit?: (movedId: string, insertIndex: number) => void;
  onEdgeHold?: (direction: -1 | 1) => void;
  scrollContainer?: HTMLElement | null;
}) {
  const drag = useDragSort({
    itemIds: ids,
    onCommit,
    onEdgeHold,
    getScrollContainer: () => scrollContainer ?? null,
  });

  return (
    <div>
      <output data-testid="active">{drag.activeId ?? "none"}</output>
      <output data-testid="slot">{drag.insertIndex ?? "none"}</output>
      <output data-testid="placeholder-height">{drag.placeholderHeight}</output>
      {ids.map((id, index) => (
        <div
          data-height={ROW_HEIGHT}
          data-testid={`row-${id}`}
          data-top={index * ROW_PITCH}
          key={id}
          ref={drag.registerItem(id)}
        >
          <button
            data-testid={`handle-${id}`}
            onPointerDown={(event) => drag.startDrag(id, event)}
            type="button"
          >
            {id}
          </button>
        </div>
      ))}
    </div>
  );
}

describe("useDragSort", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("drops the last row into the first slot", () => {
    stubRects();
    const onCommit = vi.fn();
    render(<SortProbe onCommit={onCommit} />);

    dispatchPointerEvent(screen.getByTestId("handle-c"), "pointerdown", { clientY: 100 });
    dispatchPointerEvent(document, "pointermove", { clientY: 10 });

    expect(screen.getByTestId("active")).toHaveTextContent("c");
    expect(screen.getByTestId("slot")).toHaveTextContent("0");
    expect(screen.getByTestId("placeholder-height")).toHaveTextContent(String(ROW_HEIGHT));

    dispatchPointerEvent(document, "pointerup", { clientY: 10 });
    expect(onCommit).toHaveBeenCalledWith("c", 0);
    expect(screen.getByTestId("active")).toHaveTextContent("none");
  });

  it("drops the first row into the last slot", () => {
    stubRects();
    const onCommit = vi.fn();
    render(<SortProbe onCommit={onCommit} />);

    dispatchPointerEvent(screen.getByTestId("handle-a"), "pointerdown", { clientY: 10 });
    dispatchPointerEvent(document, "pointermove", { clientY: 70 });

    expect(screen.getByTestId("slot")).toHaveTextContent("2");

    dispatchPointerEvent(document, "pointerup", { clientY: 70 });
    expect(onCommit).toHaveBeenCalledWith("a", 2);
  });

  it("lifts the row under the cursor and puts it back on drop", () => {
    stubRects();
    render(<SortProbe />);
    const row = screen.getByTestId("row-c");

    dispatchPointerEvent(screen.getByTestId("handle-c"), "pointerdown", { clientY: 100 });
    dispatchPointerEvent(document, "pointermove", { clientY: 40 });

    expect(row.style.position).toBe("fixed");
    expect(row.style.transform).toBe("translate3d(0, -60px, 0)");
    expect(row.style.width).toBe("200px");

    dispatchPointerEvent(document, "pointerup", { clientY: 40 });
    expect(row.style.position).toBe("");
    expect(row.style.transform).toBe("");
  });

  it("keeps a click on the handle from lifting the row", () => {
    stubRects();
    const onCommit = vi.fn();
    render(<SortProbe onCommit={onCommit} />);

    dispatchPointerEvent(screen.getByTestId("handle-b"), "pointerdown", { clientY: 50 });
    dispatchPointerEvent(document, "pointermove", { clientY: 52 });
    expect(screen.getByTestId("active")).toHaveTextContent("none");

    dispatchPointerEvent(document, "pointerup", { clientY: 52 });
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("skips the commit when the row is released in its own slot", () => {
    stubRects();
    const onCommit = vi.fn();
    render(<SortProbe onCommit={onCommit} />);

    dispatchPointerEvent(screen.getByTestId("handle-a"), "pointerdown", { clientY: 10 });
    dispatchPointerEvent(document, "pointermove", { clientY: 18 });
    expect(screen.getByTestId("slot")).toHaveTextContent("0");

    dispatchPointerEvent(document, "pointerup", { clientY: 18 });
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("cancels the drag on Escape and on pointercancel", () => {
    stubRects();
    const onCommit = vi.fn();
    render(<SortProbe onCommit={onCommit} />);

    dispatchPointerEvent(screen.getByTestId("handle-c"), "pointerdown", { clientY: 100 });
    dispatchPointerEvent(document, "pointermove", { clientY: 10 });
    dispatchKeyDown("Escape");

    expect(screen.getByTestId("active")).toHaveTextContent("none");
    expect(screen.getByTestId("row-c").style.position).toBe("");
    dispatchPointerEvent(document, "pointerup", { clientY: 10 });
    expect(onCommit).not.toHaveBeenCalled();

    dispatchPointerEvent(screen.getByTestId("handle-c"), "pointerdown", { clientY: 100 });
    dispatchPointerEvent(document, "pointermove", { clientY: 10 });
    dispatchPointerEvent(document, "pointercancel", { clientY: 10 });
    expect(screen.getByTestId("active")).toHaveTextContent("none");
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("ignores presses that are not the primary button", () => {
    stubRects();
    render(<SortProbe />);

    dispatchPointerEvent(screen.getByTestId("handle-a"), "pointerdown", {
      button: 2,
      clientY: 10,
    });
    dispatchPointerEvent(document, "pointermove", { clientY: 90 });

    expect(screen.getByTestId("active")).toHaveTextContent("none");
  });

  it("releases the drag when the list unmounts mid-drag", () => {
    stubRects();
    const onCommit = vi.fn();
    const { unmount } = render(<SortProbe onCommit={onCommit} />);

    dispatchPointerEvent(screen.getByTestId("handle-c"), "pointerdown", { clientY: 100 });
    dispatchPointerEvent(document, "pointermove", { clientY: 10 });
    unmount();
    dispatchPointerEvent(document, "pointerup", { clientY: 10 });

    expect(onCommit).not.toHaveBeenCalled();
  });

  it("scrolls the list while the pointer sits against an edge", () => {
    stubRects();
    const container = scrollContainerStub({ scrollTop: 200, scrollHeight: 1000 });
    render(<SortProbe scrollContainer={container} />);

    dispatchPointerEvent(screen.getByTestId("handle-c"), "pointerdown", { clientY: 300 });
    dispatchPointerEvent(document, "pointermove", { clientY: 10 });

    expect(container.scrollTop).toBeLessThan(200);
  });

  it("asks for the neighbouring page once the list cannot scroll further", () => {
    stubRects();
    const onEdgeHold = vi.fn();
    const container = scrollContainerStub({ scrollTop: 0, scrollHeight: 400 });
    render(<SortProbe onEdgeHold={onEdgeHold} scrollContainer={container} />);

    dispatchPointerEvent(screen.getByTestId("handle-c"), "pointerdown", { clientY: 300 });
    dispatchPointerEvent(document, "pointermove", { clientY: 10 });

    expect(onEdgeHold).toHaveBeenCalledWith(-1);
    expect(container.scrollTop).toBe(0);
  });

  it("waits for a press that starts against an edge to leave it before scrolling", () => {
    stubRects();
    const container = scrollContainerStub({ scrollTop: 200, scrollHeight: 1000 });
    render(<SortProbe scrollContainer={container} />);

    // 400px tall container, so the bottom edge zone starts at 352.
    dispatchPointerEvent(screen.getByTestId("handle-c"), "pointerdown", { clientY: 380 });
    dispatchPointerEvent(document, "pointermove", { clientY: 386 });
    expect(container.scrollTop).toBe(200);

    dispatchPointerEvent(document, "pointermove", { clientY: 300 });
    dispatchPointerEvent(document, "pointermove", { clientY: 386 });
    expect(container.scrollTop).toBeGreaterThan(200);
  });
});
