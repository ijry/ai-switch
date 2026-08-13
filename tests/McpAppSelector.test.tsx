import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "../src/lib/i18n";
import { McpAppSelector } from "../src/components/mcp/McpAppSelector";

describe("McpAppSelector", () => {
  it("localizes the legend and client labels", () => {
    render(
      <I18nProvider initialLanguage="zh-CN">
        <McpAppSelector legend="目标客户端" onChange={() => undefined} selectedApps={["codex"]} />
      </I18nProvider>,
    );

    expect(screen.getByText("目标客户端")).toBeInTheDocument();
    expect(screen.getByText("Codex CLI")).toBeInTheDocument();
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
  });
});
