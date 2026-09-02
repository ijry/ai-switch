import { describe, expect, it } from "vitest";

import {
  closeDragGap,
  dragRowPitch,
  gridDragSlots,
  gridInsertionIndexFromPointer,
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

// Two columns of 100px cards with an 8px gap, filled in reading order: a b / c d / e.
const cards: DragSortRow[] = [
  { id: "a", top: 0, height: 90, left: 0, width: 100 },
  { id: "b", top: 0, height: 90, left: 108, width: 100 },
  { id: "c", top: 98, height: 90, left: 0, width: 100 },
  { id: "d", top: 98, height: 90, left: 108, width: 100 },
  { id: "e", top: 196, height: 90, left: 0, width: 100 },
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

describe("gridDragSlots", () => {
  it("slides every later card back one slot in reading order", () => {
    expect(gridDragSlots(cards, "b")).toEqual([
      { id: "a", top: 0, height: 90, left: 0, width: 100 },
      // c wraps up into the slot b left behind on the first grid row.
      { id: "c", top: 0, height: 90, left: 108, width: 100 },
      { id: "d", top: 98, height: 90, left: 0, width: 100 },
      { id: "e", top: 98, height: 90, left: 108, width: 100 },
    ]);
  });

  it("leaves the layout alone when the dragged card is the last one", () => {
    expect(gridDragSlots(cards, "e")).toEqual(cards.slice(0, 4));
  });

  it("keeps every slot when the dragged card is no longer on the page", () => {
    expect(gridDragSlots(cards, "gone")).toEqual(cards);
  });
});

describe("gridInsertionIndexFromPointer", () => {
  const slots = gridDragSlots(cards, "e");

  it("targets the first slot above and left of everything", () => {
    expect(gridInsertionIndexFromPointer(slots, 10, -20)).toBe(0);
    expect(gridInsertionIndexFromPointer(slots, 10, 40)).toBe(0);
  });

  it("advances past a card once the pointer clears its horizontal midpoint", () => {
    expect(gridInsertionIndexFromPointer(slots, 60, 40)).toBe(1);
    expect(gridInsertionIndexFromPointer(slots, 170, 40)).toBe(2);
  });

  it("counts whole rows the pointer has left behind", () => {
    expect(gridInsertionIndexFromPointer(slots, 10, 140)).toBe(2);
    expect(gridInsertionIndexFromPointer(slots, 60, 140)).toBe(3);
  });

  it("lands at the end of the upper row when the pointer sits in the gap", () => {
    expect(gridInsertionIndexFromPointer(slots, 10, 94)).toBe(2);
  });

  it("clamps to the end of the grid below the last card", () => {
    expect(gridInsertionIndexFromPointer(slots, 10, 4000)).toBe(4);
  });

  it("moves the last card to the front when dropped on the first card's left half", () => {
    const targetIndex = gridInsertionIndexFromPointer(slots, 10, 40);
    expect(targetIndex).toBe(0);
    expect(
      neighborsForDrop({ items: cards, movedId: "e", targetIndex }),
    ).toEqual({ previousAccountId: null, nextAccountId: "a" });
  });
});
