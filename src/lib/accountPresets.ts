import { CLAUDE_ROLES } from "./claude-roles";
import type { InterfaceFormat, ModelMapping, PlatformId } from "./api/types";

export type AccountPreset = {
  id: string;
  platform: PlatformId;
  label: string;
  /**
   * Provider name shown in the "preset applied" hint. Kept separate from
   * `label` (which carries the host) and from `defaultName` (which can vary per
   * line, e.g. "AgentRouter 备用") so two lines of one provider read the same.
   */
  provider: string;
  defaultName: string;
  baseUrl: string;
  interfaceFormat: InterfaceFormat;
  modelMappings: ModelMapping[];
};

/**
 * Models both AgentRouter codex lines expose, passed through unchanged.
 *
 * The request model doubles as the upstream model here, so the list is also
 * what the proxy will accept for an account created from these presets.
 */
const AGENTROUTER_CODEX_MODELS = [
  "gpt-5.6-sol",
  "glm-5.3",
  "deepseek-v4-flash",
] as const;

function passthroughMappings(models: readonly string[]): ModelMapping[] {
  return models.map((model) => ({ from: model, to: model }));
}

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
    provider: "AgentRouter",
    defaultName: "AgentRouter",
    baseUrl: "https://agentrouter.org/v1",
    interfaceFormat: "openai",
    modelMappings: passthroughMappings(AGENTROUTER_CODEX_MODELS),
  },
  {
    id: "agentrouter-backup",
    platform: "codex",
    label: "AgentRouter (ps.air-outer.com)",
    provider: "AgentRouter",
    defaultName: "AgentRouter 备用",
    baseUrl: "https://ps.air-outer.com/v1",
    interfaceFormat: "openai",
    modelMappings: passthroughMappings(AGENTROUTER_CODEX_MODELS),
  },
  {
    id: "kktoken",
    platform: "codex",
    label: "KKToken (kktoken.cc)",
    provider: "KKToken",
    defaultName: "KKToken",
    baseUrl: "https://kktoken.cc/v1",
    interfaceFormat: "openai",
    // Serves one model under its own id, so the request model doubles as the
    // upstream model like the other codex lines.
    modelMappings: passthroughMappings(["claude-opus-5"]),
  },
  {
    id: "agentrouter-claude",
    platform: "claude",
    label: "AgentRouter (ps.air-outer.com)",
    provider: "AgentRouter",
    defaultName: "AgentRouter Claude",
    baseUrl: "https://ps.air-outer.com",
    interfaceFormat: "anthropic",
    modelMappings: claudeRolesMappedTo("claude-opus-5"),
  },
  {
    id: "gorouter-claude",
    platform: "claude",
    label: "GoRouter (gorouter.app)",
    provider: "GoRouter",
    defaultName: "GoRouter",
    // No /v1: the proxy appends the Anthropic path itself, same as the line
    // above.
    baseUrl: "https://gorouter.app",
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
