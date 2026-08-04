export type AccountSortNeighbors = {
  previousAccountId: string | null;
  nextAccountId: string | null;
};

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
