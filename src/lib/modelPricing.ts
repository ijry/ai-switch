export type ModelPriceConfig = {
  display_name: string;
  input_per_mtok: number;
  output_per_mtok: number;
  cache_read_per_mtok?: number | null;
  cache_write_per_mtok?: number | null;
};

export type ModelPriceRow = {
  model: string;
  display_name: string;
  input_per_mtok: number | null;
  output_per_mtok: number | null;
  cache_read_per_mtok: number | null;
  cache_write_per_mtok: number | null;
};

type ProxyModel = { id: string; [key: string]: unknown };

function canonicalModelId(model: string): string {
  return model.trim().split("/").pop()?.trim() ?? "";
}

export function parsePriceValue(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parsed = Number(trimmed);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
}

export function mergeModelPriceRows(
  models: ProxyModel[],
  prices: Record<string, ModelPriceConfig>,
): ModelPriceRow[] {
  const ids = new Map<string, string>();
  for (const model of models) {
    const id = canonicalModelId(model.id);
    if (id) ids.set(id.toLowerCase(), id);
  }
  for (const model of Object.keys(prices)) {
    const id = canonicalModelId(model);
    if (id) ids.set(id.toLowerCase(), id);
  }

  return Array.from(ids.values())
    .sort((a, b) => a.localeCompare(b))
    .map((model) => {
      const price = Object.entries(prices).find(
        ([key]) => canonicalModelId(key).toLowerCase() === model.toLowerCase(),
      )?.[1];
      return {
        model,
        display_name: price?.display_name ?? "",
        input_per_mtok: price?.input_per_mtok ?? null,
        output_per_mtok: price?.output_per_mtok ?? null,
        cache_read_per_mtok: price?.cache_read_per_mtok ?? null,
        cache_write_per_mtok: price?.cache_write_per_mtok ?? null,
      };
    });
}
