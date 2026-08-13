import type { McpAppType } from "../../lib/api/types";
import type { TranslationKey } from "../../lib/i18n";

export const MCP_APPS: Array<{ id: McpAppType; labelKey: TranslationKey }> = [
  { id: "codex", labelKey: "mcp.client.codex" },
  { id: "claude_code", labelKey: "mcp.client.claude_code" },
  { id: "gemini", labelKey: "mcp.client.gemini" },
  { id: "grok", labelKey: "mcp.client.grok" },
  { id: "open_code", labelKey: "mcp.client.open_code" },
  { id: "open_claw", labelKey: "mcp.client.open_claw" },
  { id: "hermes", labelKey: "mcp.client.hermes" },
  { id: "cline", labelKey: "mcp.client.cline" },
  { id: "cursor", labelKey: "mcp.client.cursor" },
  { id: "kimi_code", labelKey: "mcp.client.kimi_code" },
  { id: "code_buddy", labelKey: "mcp.client.code_buddy" },
];

export function appLabelKey(app: McpAppType): TranslationKey {
  return MCP_APPS.find((item) => item.id === app)?.labelKey ?? "mcp.client.codex";
}
