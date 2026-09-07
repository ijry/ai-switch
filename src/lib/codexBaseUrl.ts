import type { InterfaceFormat, PlatformId } from "./api/types";

export type CodexBaseUrlAdjustment = {
  originalBaseUrl: string;
  adjustedBaseUrl: string;
  action: "remove-v1" | "restore-v1";
};

export function adjustCodexBaseUrlForInterfaceFormat(
  platform: PlatformId,
  baseUrl: string,
  interfaceFormat: InterfaceFormat,
  previousAdjustment: CodexBaseUrlAdjustment | null = null,
): CodexBaseUrlAdjustment | null {
  if (platform !== "codex") {
    return null;
  }

  if (interfaceFormat === "anthropic") {
    const trimmedBaseUrl = baseUrl.trim();
    try {
      const url = new URL(trimmedBaseUrl);
      if ((url.protocol !== "http:" && url.protocol !== "https:") || !/\/v1\/?$/.test(url.pathname)) {
        return null;
      }
    } catch {
      return null;
    }
    const match = trimmedBaseUrl.match(/^([^?#]+)\/v1\/?([?#].*)?$/);
    if (!match) {
      return null;
    }
    return {
      originalBaseUrl: baseUrl,
      adjustedBaseUrl: match[1] + (match[2] ?? ""),
      action: "remove-v1",
    };
  }

  if (
    (interfaceFormat === "openai" || interfaceFormat === "openai-responses") &&
    previousAdjustment?.action === "remove-v1" &&
    baseUrl === previousAdjustment.adjustedBaseUrl
  ) {
    return {
      originalBaseUrl: baseUrl,
      adjustedBaseUrl: previousAdjustment.originalBaseUrl,
      action: "restore-v1",
    };
  }

  return null;
}
