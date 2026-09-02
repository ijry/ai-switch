import { describe, expect, it } from "vitest";

import {
  closeDragGap,
  dragRowPitch,
  insertionIndexFromPointer,
  neighborsForDrop,
  type DragSortRow,
} from "../../src/lib/accountReorder";

const items = [{ id: "a" }, { id: "b" }, { id: "c" }];

// Three 40px rows with the 2px gap the account list renders between them.
const rows: DragSortRow[] = [
  { id: "a", top: 0, height: 40 },
  { id: "b", top: 42, height: 40 },
  { id: "c", top: 84, height: 40 },
];

describe("neighborsForDrop", () => {
  it("reads the target index as a slot in the list without the dragged account", () => {
    expect(neighborsForDrop({ items, movedId: "a", targetIndex: 1 })).toEqual({
      previousAccountId: "b",
      nextAccountId: "c",
    });
  });

  it("moves an account to the very front when the slot is 0", () => {
    expect(neighborsForDrop({ items, movedId: "c", targetIndex: 0 })).toEqual({
      previousAccountId: null,
      nextAccountId: "a",
    });
  });

  it("falls back to the neighbouring pages at the list edges", () => {
    expect(
      neighborsForDrop({
        items,
        movedId: "b",
        targetIndex: 0,
        previousPageAccountId: "prev",
        nextPageAccountId: "next",
      }),
    ).toEqual({ previousAccountId: "prev", nextAccountId: "a" });

    expect(
      neighborsForDrop({
        items,
        movedId: "b",
        targetIndex: 2,
        previousPageAccountId: "prev",
        nextPageAccountId: "next",
      }),
    ).toEqual({ previousAccountId: "c", nextAccountId: "next" });
  });

  it("keeps the dragged account out of its own neighbours across pages", () => {
    expect(
      neighborsForDrop({
        items,
        movedId: "b",
        targetIndex: 0,
        previousPageAccountId: "b",
      }),
    ).toEqual({ previousAccountId: null, nextAccountId: "a" });
  });
});

describe("dragRowPitch", () => {
  it("measures the row height plus the gap to the next row", () => {
    expect(dragRowPitch(rows, 0)).toBe(42);
    expect(dragRowPitch(rows, 1)).toBe(42);
  });

  it("reuses the gap above when the row is the last one", () => {
    expect(dragRowPitch(rows, 2)).toBe(42);
  });

  it("falls back to the bare height for a single row", () => {
    expect(dragRowPitch([{ id: "a", top: 10, height: 40 }], 0)).toBe(40);
  });

  it("returns zero for a row that is not measured", () => {
    expect(dragRowPitch(rows, 9)).toBe(0);
  });
});

describe("closeDragGap", () => {
  it("pulls the rows below the freed slot up by one pitch", () => {
    const remaining = rows.filter((row) => row.id !== "b");
    expect(closeDragGap(remaining, 1, 42)).toEqual([
      { id: "a", top: 0, height: 40 },
      { id: "c", top: 42, height: 40 },
    ]);
  });

  it("leaves the layout alone when the freed slot is below every row", () => {
    const remaining = rows.filter((row) => row.id !== "c");
    expect(closeDragGap(remaining, 2, 42)).toEqual(remaining);
  });
});

describe("insertionIndexFromPointer", () => {
  const gapClosed = closeDragGap(
    rows.filter((row) => row.id !== "c"),
    2,
    42,
  );

  it("targets the first slot while the pointer sits above the first midpoint", () => {
    expect(insertionIndexFromPointer(gapClosed, -30)).toBe(0);
    expect(insertionIndexFromPointer(gapClosed, 19)).toBe(0);
  });

  it("advances one slot per midpoint the pointer passes", () => {
    expect(insertionIndexFromPointer(gapClosed, 21)).toBe(1);
    expect(insertionIndexFromPointer(gapClosed, 63)).toBe(2);
  });

  it("clamps to the end of the list far below the last row", () => {
    expect(insertionIndexFromPointer(gapClosed, 4000)).toBe(2);
  });

  it("sends the last account to the front when dropped over the first row", () => {
    const pitch = dragRowPitch(rows, 2);
    const remaining = closeDragGap(
      rows.filter((row) => row.id !== "c"),
      2,
      pitch,
    );
    const targetIndex = insertionIndexFromPointer(remaining, 8);
    expect(targetIndex).toBe(0);
    expect(neighborsForDrop({ items, movedId: "c", targetIndex })).toEqual({
      previousAccountId: null,
      nextAccountId: "a",
    });
  });
});
