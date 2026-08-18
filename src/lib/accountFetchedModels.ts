import type { FetchedRouteModel } from "./api/types";

function recordFromUnknown(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

export function normalizeFetchedModels(value: unknown): FetchedRouteModel[] {
  if (!Array.isArray(value)) {
    return [];
  }
  const models: FetchedRouteModel[] = [];
  for (const item of value) {
    const record = recordFromUnknown(item);
    const id = typeof record?.id === "string" ? record.id.trim() : "";
    if (!id) {
      continue;
    }
    const ownedBy =
      typeof record?.owned_by === "string" ? record.owned_by.trim() : "";
    models.push({
      id,
      ...(ownedBy ? { owned_by: ownedBy } : {}),
      ...(typeof record?.supports_1m === "boolean"
        ? { supports_1m: record.supports_1m }
        : {}),
    });
  }
  return models;
}

export function parseFetchedModelsFromConfig(configJson: string): FetchedRouteModel[] {
  try {
    const config = recordFromUnknown(JSON.parse(configJson));
    return normalizeFetchedModels(config?.fetched_models);
  } catch {
    return [];
  }
}

export function writeFetchedModelsToConfig(
  config: Record<string, unknown>,
  models: FetchedRouteModel[],
): Record<string, unknown> {
  return { ...config, fetched_models: normalizeFetchedModels(models) };
}
