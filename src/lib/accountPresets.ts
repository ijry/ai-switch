import { CLAUDE_ROLES } from "./claude-roles";
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

/**
 * Every Claude role pointed at one upstream model.
 *
 * Derived from the role table rather than spelled out so a role added there
 * cannot silently end up unmapped in this preset. `supports_1m` is deliberately
 * left unset: the proxy only sends the `context-1m` beta marker when a mapping
 * declares it, and an upstream that lacks the tier answers 503 rather than
 * ignoring the marker — so this stays a deliberate tick by the user.
 */
function claudeRolesMappedTo(model: string): ModelMapping[] {
  return CLAUDE_ROLES.map((role) => ({ from: role.alias, to: model }));
}

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
  {
    id: "agentrouter-claude",
    platform: "claude",
    label: "AgentRouter (ps.air-outer.com)",
    defaultName: "AgentRouter Claude",
    baseUrl: "https://ps.air-outer.com",
    interfaceFormat: "anthropic",
    modelMappings: claudeRolesMappedTo("claude-opus-5"),
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
