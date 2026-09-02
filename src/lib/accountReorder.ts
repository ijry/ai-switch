export type AccountSortNeighbors = {
  previousAccountId: string | null;
  nextAccountId: string | null;
};

export type DragSortRow = {
  id: string;
  top: number;
  height: number;
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
