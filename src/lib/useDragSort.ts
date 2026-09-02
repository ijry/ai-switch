import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";

import {
  closeDragGap,
  dragRowPitch,
  insertionIndexFromPointer,
  type DragSortRow,
} from "./accountReorder";

// How close to the scroll container's edge the pointer has to get before the list
// scrolls itself, and how far it scrolls per frame right at the edge.
const EDGE_ZONE = 48;
const MAX_SCROLL_STEP = 24;
// Slack before a press becomes a drag, so clicking the handle does not lift a row.
const ACTIVATION_DISTANCE = 4;
const LIFTED_PROPERTIES = [
  "position",
  "top",
  "left",
  "width",
  "height",
  "margin",
  "z-index",
  "pointer-events",
  "transform",
];

export type DragSortOptions = {
  // Ids in visual order. Only ids that registered a node take part in the drag.
  itemIds: string[];
  // `insertIndex` is a slot in the list with the dragged item taken out.
  onCommit: (movedId: string, insertIndex: number) => void;
  getScrollContainer?: () => HTMLElement | null;
  // Fires while the pointer rests against an edge the container cannot scroll past.
  onEdgeHold?: (direction: -1 | 1) => void;
  onEdgeLeave?: () => void;
  disabled?: boolean;
};

export type DragSortHandle = {
  activeId: string | null;
  // Slot the placeholder currently occupies, in the same coordinates as onCommit.
  insertIndex: number | null;
  placeholderHeight: number;
  registerItem: (id: string) => (node: HTMLElement | null) => void;
  startDrag: (id: string, event: ReactPointerEvent<HTMLElement>) => void;
  cancel: () => void;
};

type ActiveDrag = {
  id: string;
  pointerId: number;
  startClientY: number;
  pointerClientY: number;
  // Gap-closed layout of the other rows, in viewport coordinates as measured.
  rows: DragSortRow[];
  pitch: number;
  scrollTop: number;
  insertIndex: number;
  // Slot the row came from — dropping there is a no-op. -1 once the row has left
  // the page, because then every slot is a real move.
  originIndex: number;
  // Whether the pointer has been clear of the container edges at least once.
  edgeArmed: boolean;
  node: HTMLElement | null;
  frame: number | null;
};

function liftNode(node: HTMLElement, rect: DOMRect) {
  node.style.position = "fixed";
  node.style.zIndex = "40";
  node.style.margin = "0";
  node.style.pointerEvents = "none";
  node.style.width = `${rect.width}px`;
  node.style.height = `${rect.height}px`;
  node.style.top = `${rect.top}px`;
  node.style.left = `${rect.left}px`;
  // A transformed or filtered ancestor becomes the containing block for a fixed
  // element and offsets it from the rect we just measured. Measure once more and
  // pay the difference back.
  const lifted = node.getBoundingClientRect();
  if (Math.abs(lifted.top - rect.top) > 0.5) {
    node.style.top = `${rect.top + (rect.top - lifted.top)}px`;
  }
  if (Math.abs(lifted.left - rect.left) > 0.5) {
    node.style.left = `${rect.left + (rect.left - lifted.left)}px`;
  }
}

function resetNode(node: HTMLElement) {
  for (const property of LIFTED_PROPERTIES) {
    node.style.removeProperty(property);
  }
}

// Vertical sort driven by pointer events: the pressed row is lifted out of flow and
// follows the cursor, a placeholder holds the slot it would land in, and the drop
// commits that slot. Native HTML5 drag and drop only reports the element under the
// cursor, so a release over a gap between rows — or over a row whose dragover never
// called preventDefault — silently lost the drop.
export function useDragSort(options: DragSortOptions): DragSortHandle {
  const optionsRef = useRef(options);
  optionsRef.current = options;

  const nodesRef = useRef(new Map<string, HTMLElement>());
  const itemRefsRef = useRef(new Map<string, (node: HTMLElement | null) => void>());
  const dragRef = useRef<ActiveDrag | null>(null);
  const stopRef = useRef<((commit: boolean) => void) | null>(null);

  const [activeId, setActiveId] = useState<string | null>(null);
  const [insertIndex, setInsertIndex] = useState<number | null>(null);
  const [placeholderHeight, setPlaceholderHeight] = useState(0);

  const registerItem = useCallback((id: string) => {
    const existing = itemRefsRef.current.get(id);
    if (existing) return existing;
    const attach = (node: HTMLElement | null) => {
      if (node) nodesRef.current.set(id, node);
      else nodesRef.current.delete(id);
    };
    itemRefsRef.current.set(id, attach);
    return attach;
  }, []);

  const scrollTopOf = () => optionsRef.current.getScrollContainer?.()?.scrollTop ?? 0;

  const measure = (id: string): DragSortRow | null => {
    const node = nodesRef.current.get(id);
    if (!node) return null;
    const rect = node.getBoundingClientRect();
    return { id, top: rect.top, height: rect.height };
  };

  // Rows other than the dragged one, pulled back into the layout they would have
  // without it. `boundaryIndex` is the slot that currently holds the freed space:
  // the dragged row's own slot before the lift, the placeholder's slot afterwards.
  const snapshotRows = (movedId: string, boundaryIndex: number, pitch: number) => {
    const rows: DragSortRow[] = [];
    for (const id of optionsRef.current.itemIds) {
      if (id === movedId) continue;
      const row = measure(id);
      if (row) rows.push(row);
    }
    return closeDragGap(rows, boundaryIndex, pitch);
  };

  const syncDrag = (drag: ActiveDrag) => {
    // Rows were measured in viewport coordinates, so anything the container has
    // scrolled since then has to be added back onto the pointer.
    const pointerY = drag.pointerClientY + (scrollTopOf() - drag.scrollTop);
    const next = insertionIndexFromPointer(drag.rows, pointerY);
    if (next !== drag.insertIndex) {
      drag.insertIndex = next;
      setInsertIndex(next);
    }
    if (drag.node) {
      const offset = Math.round(drag.pointerClientY - drag.startClientY);
      drag.node.style.transform = `translate3d(0, ${offset}px, 0)`;
    }
  };

  const cancelFrame = (drag: ActiveDrag) => {
    if (drag.frame == null) return;
    if (typeof window !== "undefined" && typeof window.cancelAnimationFrame === "function") {
      window.cancelAnimationFrame(drag.frame);
    }
    drag.frame = null;
  };

  // How hard the list should scroll for a pointer this close to a container edge.
  const edgeAt = (pointerY: number) => {
    const container = optionsRef.current.getScrollContainer?.();
    if (!container) return { direction: null as -1 | 1 | null, intensity: 0 };
    const rect = container.getBoundingClientRect();
    if (pointerY < rect.top + EDGE_ZONE) {
      return { direction: -1 as const, intensity: (rect.top + EDGE_ZONE - pointerY) / EDGE_ZONE };
    }
    if (pointerY > rect.bottom - EDGE_ZONE) {
      return { direction: 1 as const, intensity: (pointerY - (rect.bottom - EDGE_ZONE)) / EDGE_ZONE };
    }
    return { direction: null as -1 | 1 | null, intensity: 0 };
  };

  const autoScroll = (drag: ActiveDrag) => {
    const container = optionsRef.current.getScrollContainer?.();
    if (!container) return;
    const { direction, intensity } = edgeAt(drag.pointerClientY);
    if (direction == null) {
      drag.edgeArmed = true;
      optionsRef.current.onEdgeLeave?.();
      return;
    }
    // A press that starts inside an edge zone has to leave it once before the list
    // runs, so grabbing the bottom row does not scroll away under the cursor.
    if (!drag.edgeArmed) return;
    const maxScrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
    const spent = direction < 0 ? container.scrollTop <= 0 : container.scrollTop >= maxScrollTop - 1;
    if (spent) {
      // Nothing left to scroll, so the neighbouring page is what the pointer wants.
      optionsRef.current.onEdgeHold?.(direction);
      return;
    }
    const step = Math.max(4, Math.round(Math.min(1, intensity) * MAX_SCROLL_STEP));
    container.scrollTop += direction * step;
    syncDrag(drag);
  };

  const scheduleFrame = () => {
    if (typeof window === "undefined" || typeof window.requestAnimationFrame !== "function") return;
    const drag = dragRef.current;
    if (!drag) return;
    drag.frame = window.requestAnimationFrame(() => {
      const current = dragRef.current;
      if (!current) return;
      current.frame = null;
      // Re-read every frame so a wheel scroll during the drag still moves the
      // placeholder, not just pointer movement.
      syncDrag(current);
      autoScroll(current);
      scheduleFrame();
    });
  };

  const startDrag = useCallback((id: string, event: ReactPointerEvent<HTMLElement>) => {
    const native = event.nativeEvent;
    if (optionsRef.current.disabled || native.button !== 0 || dragRef.current || stopRef.current) {
      return;
    }

    const pointerId = native.pointerId;
    const pressX = native.clientX;
    const pressY = native.clientY;
    const handle = event.currentTarget;
    const previousCursor = document.body.style.cursor;
    const previousUserSelect = document.body.style.userSelect;
    document.body.style.userSelect = "none";

    function begin(): boolean {
      const node = nodesRef.current.get(id);
      if (!node) return false;
      const rows: DragSortRow[] = [];
      for (const rowId of optionsRef.current.itemIds) {
        const row = measure(rowId);
        if (row) rows.push(row);
      }
      const originIndex = rows.findIndex((row) => row.id === id);
      if (originIndex < 0) return false;
      const pitch = dragRowPitch(rows, originIndex);
      const rect = node.getBoundingClientRect();
      dragRef.current = {
        id,
        pointerId,
        startClientY: pressY,
        pointerClientY: pressY,
        rows: closeDragGap(rows.filter((row) => row.id !== id), originIndex, pitch),
        pitch,
        scrollTop: scrollTopOf(),
        insertIndex: originIndex,
        originIndex,
        edgeArmed: edgeAt(pressY).direction == null,
        node,
        frame: null,
      };
      document.body.style.cursor = "grabbing";
      liftNode(node, rect);
      try {
        // Keeps the release reaching us even if the pointer leaves the window.
        handle.setPointerCapture?.(pointerId);
      } catch {
        // Pointer capture is not available in every host, including jsdom.
      }
      setActiveId(id);
      setInsertIndex(originIndex);
      setPlaceholderHeight(rect.height);
      scheduleFrame();
      return true;
    }

    function stop(commit: boolean) {
      const drag = dragRef.current;
      document.removeEventListener("pointermove", onPointerMove);
      document.removeEventListener("pointerup", onPointerUp);
      document.removeEventListener("pointercancel", onPointerCancel);
      document.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("blur", onPointerCancel);
      dragRef.current = null;
      stopRef.current = null;
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousUserSelect;
      if (handle.hasPointerCapture?.(pointerId)) {
        try {
          handle.releasePointerCapture?.(pointerId);
        } catch {
          // The host may have released the capture already.
        }
      }
      if (!drag) return;
      cancelFrame(drag);
      if (drag.node) resetNode(drag.node);
      optionsRef.current.onEdgeLeave?.();
      setActiveId(null);
      setInsertIndex(null);
      setPlaceholderHeight(0);
      // The slot the placeholder was holding is the answer — a pointerup carries no
      // new information about where the row should land.
      if (commit && drag.insertIndex !== drag.originIndex) {
        optionsRef.current.onCommit(drag.id, drag.insertIndex);
      }
    }

    function onPointerMove(moveEvent: PointerEvent) {
      if (moveEvent.pointerId !== pointerId) return;
      if (!dragRef.current) {
        const movedFar =
          Math.abs(moveEvent.clientY - pressY) >= ACTIVATION_DISTANCE ||
          Math.abs(moveEvent.clientX - pressX) >= ACTIVATION_DISTANCE;
        if (!movedFar) return;
        if (!begin()) {
          stop(false);
          return;
        }
      }
      const drag = dragRef.current;
      if (!drag) return;
      drag.pointerClientY = moveEvent.clientY;
      syncDrag(drag);
      // Also react to the edges on the move itself, so entering the edge zone acts
      // at once instead of waiting for the next frame.
      autoScroll(drag);
    }

    function onPointerUp(upEvent: PointerEvent) {
      if (upEvent.pointerId !== pointerId) return;
      stop(true);
    }

    function onPointerCancel() {
      stop(false);
    }

    function onKeyDown(keyEvent: KeyboardEvent) {
      if (keyEvent.key !== "Escape") return;
      keyEvent.preventDefault();
      stop(false);
    }

    stopRef.current = stop;
    document.addEventListener("pointermove", onPointerMove);
    document.addEventListener("pointerup", onPointerUp);
    document.addEventListener("pointercancel", onPointerCancel);
    document.addEventListener("keydown", onKeyDown, true);
    // Alt-tabbing away and releasing the button there would otherwise leave the row
    // stuck to the cursor.
    window.addEventListener("blur", onPointerCancel);
  }, []);

  const cancel = useCallback(() => stopRef.current?.(false), []);

  const itemKey = options.itemIds.join(" ");
  useEffect(() => {
    const drag = dragRef.current;
    if (!drag) return;
    // The list changed under the pointer — a refetch, or an edge hold that flipped
    // to another page. Re-measure so the placeholder keeps tracking real rows; if
    // the dragged row itself left the page, drop the lifted copy and treat every
    // slot as a move, so it can still land on the page the pointer ended up on.
    if (!optionsRef.current.itemIds.includes(drag.id)) {
      drag.node = null;
      drag.originIndex = -1;
    }
    drag.rows = snapshotRows(drag.id, drag.insertIndex, drag.pitch);
    drag.scrollTop = scrollTopOf();
    syncDrag(drag);
  }, [itemKey]);

  useEffect(() => () => stopRef.current?.(false), []);

  return { activeId, insertIndex, placeholderHeight, registerItem, startDrag, cancel };
}
