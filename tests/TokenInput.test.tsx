import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { TokenInput } from "../src/components/auth/TokenInput";
import { I18nProvider } from "../src/lib/i18n";
import { copySensitiveText } from "../src/lib/routeCredentialTransfer";

vi.mock("../src/lib/routeCredentialTransfer", () => ({
  copySensitiveText: vi.fn(),
}));

function renderTokenInput() {
  return render(
    <I18nProvider initialLanguage="zh-CN">
      <TokenInput
        value="secret-token-123456"
        onChange={() => {}}
        label="访问令牌"
        copy
      />
    </I18nProvider>,
  );
}

describe("TokenInput", () => {
  it("toggles token visibility", async () => {
    renderTokenInput();

    const input = screen.getByLabelText("访问令牌");
    expect(input).toHaveAttribute("type", "password");

    await userEvent.click(screen.getByRole("button", { name: "显示访问令牌" }));

    expect(input).toHaveAttribute("type", "text");
    expect(screen.getByRole("button", { name: "隐藏访问令牌" })).toBeInTheDocument();
  });

  it("copies the token on demand", async () => {
    vi.mocked(copySensitiveText).mockResolvedValue(undefined);
    renderTokenInput();

    await userEvent.click(screen.getByRole("button", { name: "复制访问令牌" }));

    expect(copySensitiveText).toHaveBeenCalledWith("secret-token-123456");
  });
});
