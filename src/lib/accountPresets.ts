import type { InterfaceFormat, ModelMapping, PlatformId } from "./api/types";

export type AccountPreset = {
  id: string;
  platform: PlatformId;
  label: string;
  defaultName: string;
  baseUrl: string;
  interfaceFormat: InterfaceFormat;
  modelMappings: ModelMapping[];
};

export const ACCOUNT_PRESETS: AccountPreset[] = [
  {
    id: "agentrouter-primary",
    platform: "codex",
    label: "AgentRouter (agentrouter.org)",
    defaultName: "AgentRouter",
    baseUrl: "https://agentrouter.org/v1",
    interfaceFormat: "openai",
    modelMappings: [{ from: "gpt-5.6-sol", to: "gpt-5.6-sol" }],
  },
  {
    id: "agentrouter-backup",
    platform: "codex",
    label: "AgentRouter (ps.air-outer.com)",
    defaultName: "AgentRouter 备用",
    baseUrl: "https://ps.air-outer.com/v1",
    interfaceFormat: "openai",
    modelMappings: [{ from: "gpt-5.6-sol", to: "gpt-5.6-sol" }],
  },
];

export function presetsForPlatform(platform: PlatformId): AccountPreset[] {
  return ACCOUNT_PRESETS.filter((preset) => preset.platform === platform);
}

function normalizeBaseUrl(value: string) {
  return value.trim().replace(/\/+$/, "").toLowerCase();
}

export function matchPresetByBaseUrl(
  platform: PlatformId,
  baseUrl: string,
): AccountPreset | null {
  const normalized = normalizeBaseUrl(baseUrl);
  if (!normalized) {
    return null;
  }
  return (
    presetsForPlatform(platform).find(
      (preset) => normalizeBaseUrl(preset.baseUrl) === normalized,
    ) ?? null
  );
}
