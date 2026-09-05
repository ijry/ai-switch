export const GROK_WORKSPACE_USER_AGENT = "xai-grok-workspace/0.2.93";
// 与 src-tauri/src/services/client_identity.rs 的内置伪装 UA 保持同形：
// 网关按 `codex_cli_rs/` 前缀和 `claude-cli/<版本> (external, cli)` 指纹识别官方 CLI。
// 版本号也要一起跟：中转站会从 `codex_cli_rs/<版本>` 里解出引擎版本，落在它要求的
// 区间外就整个账号被拒（沿用旧版本的账号会看到「Codex version … below the minimum」）。
export const CODEX_CLI_USER_AGENT = "codex_cli_rs/0.153.4 (MacOS 15.7.2; arm64) Terminal";
export const CLAUDE_CLI_USER_AGENT = "claude-cli/2.1.2 (external, cli)";
export const BROWSER_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

export type UserAgentPresetId =
  | "default"
  | "grok-workspace"
  | "codex-cli"
  | "claude-cli"
  | "browser"
  | "custom";

export const USER_AGENT_PRESETS: Array<{
  id: UserAgentPresetId;
  label: string;
  value: string;
}> = [
  { id: "default", label: "默认（空）", value: "" },
  { id: "grok-workspace", label: "Grok Workspace", value: GROK_WORKSPACE_USER_AGENT },
  { id: "codex-cli", label: "Codex CLI", value: CODEX_CLI_USER_AGENT },
  { id: "claude-cli", label: "Claude CLI", value: CLAUDE_CLI_USER_AGENT },
  { id: "browser", label: "Browser", value: BROWSER_USER_AGENT },
  { id: "custom", label: "自定义", value: "" },
];

function headersFromConfig(config: Record<string, unknown>): Record<string, unknown> {
  const headers = config.headers;
  if (!headers || typeof headers !== "object" || Array.isArray(headers)) {
    return {};
  }
  return { ...(headers as Record<string, unknown>) };
}

export function readUserAgentFromConfig(config: Record<string, unknown>): string {
  const headers = headersFromConfig(config);
  for (const [name, value] of Object.entries(headers)) {
    if (name.toLowerCase() === "user-agent" && typeof value === "string") {
      return value.trim();
    }
  }
  return "";
}

export function writeUserAgentToConfig(
  config: Record<string, unknown>,
  userAgent: string,
): Record<string, unknown> {
  const next = { ...config };
  const headers = headersFromConfig(config);
  for (const key of Object.keys(headers)) {
    if (key.toLowerCase() === "user-agent") {
      delete headers[key];
    }
  }
  const trimmed = userAgent.trim();
  if (trimmed) {
    headers["User-Agent"] = trimmed;
  }
  if (Object.keys(headers).length > 0) {
    next.headers = headers;
  } else {
    delete next.headers;
  }
  return next;
}

export function matchUserAgentPreset(value: string): UserAgentPresetId {
  const trimmed = value.trim();
  if (!trimmed) {
    return "default";
  }
  const preset = USER_AGENT_PRESETS.find(
    (item) => item.id !== "custom" && item.id !== "default" && item.value === trimmed,
  );
  return preset?.id ?? "custom";
}
