import type { FetchedRouteModel } from "./api/types";

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

export function routeProxyModelsUrl(baseUrl: string) {
  const normalized = baseUrl.trim().replace(/\/+$/, "");
  return normalized.toLowerCase().endsWith("/v1")
    ? `${normalized}/models`
    : `${normalized}/v1/models`;
}

export async function fetchRouteProxyModels(
  baseUrl: string,
  apiKey: string,
  platform: string,
): Promise<FetchedRouteModel[]> {
  const response = await fetch(routeProxyModelsUrl(baseUrl), {
    headers: {
      Authorization: `Bearer ${apiKey}`,
      "x-ai-switch-platform": platform,
    },
  });
  const rawBody = await response.text();
  let payload: unknown = null;
  try {
    payload = rawBody ? (JSON.parse(rawBody) as unknown) : null;
  } catch {
    payload = null;
  }

  if (!response.ok) {
    const error = asRecord(asRecord(payload)?.error);
    const message = typeof error?.message === "string" ? error.message.trim() : "";
    throw new Error(message || `模型列表请求失败（HTTP ${response.status}）。`);
  }

  const payloadRecord = asRecord(payload);
  const data = Array.isArray(payloadRecord?.data)
    ? payloadRecord.data
    : Array.isArray(payloadRecord?.models)
      ? payloadRecord.models
      : null;
  if (!data) {
    throw new Error("模型列表响应格式无效。");
  }

  return data
    .map((item): FetchedRouteModel | null => {
      const record = asRecord(item);
      if (!record) {
        return null;
      }
      const id = typeof record.id === "string"
        ? record.id.trim()
        : typeof record.slug === "string"
          ? record.slug.trim()
          : "";
      if (!id) {
        return null;
      }
      const supportedReasoningLevels = Array.isArray(record.supported_reasoning_levels)
        ? record.supported_reasoning_levels
            .map((item) => {
              if (typeof item === "string") {
                const effort = item.trim();
                return effort ? { effort } : null;
              }
              const level = asRecord(item);
              const effort = typeof level?.effort === "string" ? level.effort.trim() : "";
              return effort
                ? {
                    effort,
                    ...(typeof level?.description === "string"
                      ? { description: level.description }
                      : {}),
                  }
                : null;
            })
            .filter(
              (level): level is NonNullable<typeof level> => level !== null,
            )
        : [];
      const defaultReasoningLevel =
        typeof record.default_reasoning_level === "string"
          ? record.default_reasoning_level.trim()
          : "";
      return {
        id,
        owned_by: typeof record.owned_by === "string" ? record.owned_by : null,
        ...(supportedReasoningLevels.length > 0
          ? { supported_reasoning_levels: supportedReasoningLevels }
          : {}),
        ...(defaultReasoningLevel ? { default_reasoning_level: defaultReasoningLevel } : {}),
      };
    })
    .filter((model): model is FetchedRouteModel => model !== null);
}
