export type AccountSortNeighbors = {
  previousAccountId: string | null;
  nextAccountId: string | null;
};

export type DragSortRow = {
  id: string;
  top: number;
  height: number;
  // Only the grid reading is horizontal, so a plain vertical list leaves these out.
  left?: number;
  width?: number;
};

// Outer height of one row (its own height plus the gap to the next one), which is
// exactly the space the row gives back to the list once it is lifted out of flow.
export function dragRowPitch(rows: DragSortRow[], index: number): number {
  const row = rows[index];
  if (!row) return 0;
  const next = rows[index + 1];
  if (next) return next.top - row.top;
  const previous = rows[index - 1];
  if (previous) return row.height + (row.top - (previous.top + previous.height));
  return row.height;
}

// Layout the remaining rows would have with the dragged row gone: everything from
// `boundaryIndex` on moves up by one row pitch. `boundaryIndex` is where the space
// is currently taken — the dragged row's own slot before the lift, the placeholder
// slot afterwards — so the reading stays stable instead of oscillating between two
// neighbours as the placeholder pushes rows around.
export function closeDragGap(
  rows: DragSortRow[],
  boundaryIndex: number,
  pitch: number,
): DragSortRow[] {
  if (!pitch) return rows;
  return rows.map((row, index) =>
    index >= boundaryIndex ? { ...row, top: row.top - pitch } : row,
  );
}

// Insertion index for a pointer position, counted against the gap-closed layout:
// the number of rows whose midpoint the pointer has already passed. The result is
// an index into the list without the dragged row, which is what neighborsForDrop
// and the drop placeholder both expect.
export function insertionIndexFromPointer(rows: DragSortRow[], pointerY: number): number {
  let index = 0;
  for (const row of rows) {
    if (pointerY > row.top + row.height / 2) index += 1;
  }
  return Math.max(0, Math.min(index, rows.length));
}

// Slots the remaining cards fall into once the dragged one is out of flow. A grid
// reflows in reading order, so the card that was k+1 slides into slot k: slot k is
// simply the k-th box as measured while every card was still in place. `rows` has
// to be that pre-lift measurement, in visual order, including the dragged card.
export function gridDragSlots(rows: DragSortRow[], movedId: string): DragSortRow[] {
  const remaining = rows.filter((row) => row.id !== movedId);
  return remaining.map((row, index) => {
    const slot = rows[index] ?? row;
    return {
      id: row.id,
      top: slot.top,
      height: slot.height,
      left: slot.left,
      width: slot.width,
    };
  });
}

// Insertion index for a pointer over a wrapping grid, counted in reading order: a
// slot is behind the pointer once the pointer has cleared its whole row band, or
// sits inside that band past the slot's horizontal midpoint. Pointing at the gap
// between two bands therefore lands at the end of the upper one.
export function gridInsertionIndexFromPointer(
  slots: DragSortRow[],
  pointerX: number,
  pointerY: number,
): number {
  let index = 0;
  for (const slot of slots) {
    const bottom = slot.top + slot.height;
    if (pointerY > bottom) {
      index += 1;
      continue;
    }
    if (pointerY >= slot.top && pointerX > (slot.left ?? 0) + (slot.width ?? 0) / 2) {
      index += 1;
    }
  }
  return Math.max(0, Math.min(index, slots.length));
}

export function neighborsForDrop<T extends { id: string }>(input: {
  items: T[];
  movedId: string;
  targetIndex: number;
  previousPageAccountId?: string | null;
  nextPageAccountId?: string | null;
}): AccountSortNeighbors {
  const previousPageAccountId =
    input.previousPageAccountId && input.previousPageAccountId !== input.movedId
      ? input.previousPageAccountId
      : null;
  const nextPageAccountId =
    input.nextPageAccountId && input.nextPageAccountId !== input.movedId
      ? input.nextPageAccountId
      : null;
  const remaining = input.items.filter((item) => item.id !== input.movedId);
  const index = Math.max(0, Math.min(input.targetIndex, remaining.length));
  return {
    previousAccountId:
      index > 0 ? remaining[index - 1]?.id ?? null : previousPageAccountId,
    nextAccountId:
      index < remaining.length ? remaining[index]?.id ?? null : nextPageAccountId,
  };
}
