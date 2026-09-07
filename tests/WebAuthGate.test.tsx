import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { WebAuthGate } from "../src/components/auth/WebAuthGate";
import { I18nProvider } from "../src/lib/i18n";

describe("WebAuthGate", () => {
  it("toggles the token input visibility", async () => {
    render(
      <I18nProvider initialLanguage="zh-CN">
        <WebAuthGate onAuthenticated={() => {}} />
      </I18nProvider>,
    );

    const input = screen.getByLabelText("访问令牌");
    expect(input).toHaveAttribute("type", "password");

    await userEvent.click(screen.getByRole("button", { name: "显示访问令牌" }));

    expect(input).toHaveAttribute("type", "text");
  });
});
