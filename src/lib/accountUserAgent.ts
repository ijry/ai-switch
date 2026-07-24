export const GROK_WORKSPACE_USER_AGENT = "xai-grok-workspace/0.2.93";
export const GROK_CLI_USER_AGENT = "grok-cli";
export const BROWSER_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

export type UserAgentPresetId =
  | "default"
  | "grok-workspace"
  | "grok-cli"
  | "browser"
  | "custom";

export const USER_AGENT_PRESETS: Array<{
  id: UserAgentPresetId;
  label: string;
  value: string;
}> = [
  { id: "default", label: "默认（空）", value: "" },
  { id: "grok-workspace", label: "Grok Workspace", value: GROK_WORKSPACE_USER_AGENT },
  { id: "grok-cli", label: "Grok CLI (legacy)", value: GROK_CLI_USER_AGENT },
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
