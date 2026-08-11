import type { McpAppType } from "../../lib/api/types";

export const MCP_APPS: Array<{ id: McpAppType; label: string }> = [
  { id: "codex", label: "Codex" },
  { id: "claude_code", label: "Claude Code" },
  { id: "gemini", label: "Gemini" },
  { id: "grok", label: "Grok" },
  { id: "open_code", label: "OpenCode" },
  { id: "open_claw", label: "OpenClaw" },
  { id: "hermes", label: "Hermes" },
  { id: "cline", label: "Cline" },
  { id: "cursor", label: "Cursor" },
  { id: "kimi_code", label: "Kimi Code" },
  { id: "code_buddy", label: "CodeBuddy" },
];

export function appLabel(app: McpAppType) {
  return MCP_APPS.find((item) => item.id === app)?.label ?? app;
}
