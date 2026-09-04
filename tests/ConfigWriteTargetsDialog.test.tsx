import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConfigWriteTargetsDialog } from "../src/components/accounts/ConfigWriteTargetsDialog";
import type { ConfigWriteClientStatus } from "../src/lib/api/types";

/** Call after `userEvent.setup()`: it installs a clipboard stub of its own. */
function stubClipboard() {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  return writeText;
}

const clients: ConfigWriteClientStatus[] = [
  {
    client_key: "codex",
    display_name: "Codex CLI",
    native: true,
    restart_required: false,
    target_key: "codex",
    platform: "codex",
    config_path: "/home/u/.codex/config.toml",
    file_status: "managed",
    error_code: null,
  },
  {
    client_key: "zcode",
    display_name: "ZCode",
    native: false,
    restart_required: true,
    target_key: "zcode_codex",
    platform: "codex",
    config_path: "/home/u/.zcode/v2/config.json",
    file_status: "unmanaged",
    error_code: null,
  },
];

function setup(overrides: Partial<React.ComponentProps<typeof ConfigWriteTargetsDialog>> = {}) {
  const onSubmit = vi.fn();
  const onClose = vi.fn();
  render(
    <ConfigWriteTargetsDialog
      clients={clients}
      error={null}
      initialSelection={null}
      loading={false}
      onClose={onClose}
      onSubmit={onSubmit}
      platform="codex"
      platformLabel="Codex"
      poolApiKey="sk-ai-switch-codex-key"
      poolBaseUrl="http://127.0.0.1:43111"
      {...overrides}
    />,
  );
  return { onSubmit, onClose };
}

describe("ConfigWriteTargetsDialog", () => {
  it("lists every client with its file status", () => {
    setup();

    expect(screen.getByRole("checkbox", { name: /Codex CLI/ })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /ZCode/ })).toBeInTheDocument();
    expect(screen.getByText("已接管")).toBeInTheDocument();
    expect(screen.getByText("未接管")).toBeInTheDocument();
    expect(screen.getByText("/home/u/.zcode/v2/config.json")).toBeInTheDocument();
  });

  it("checks only the native client when the user has never chosen", () => {
    setup();

    // Preserves today's behavior for users who never open the dialog.
    expect(screen.getByRole("checkbox", { name: /Codex CLI/ })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: /ZCode/ })).not.toBeChecked();
  });

  it("restores a stored selection", () => {
    setup({ initialSelection: ["zcode"] });

    expect(screen.getByRole("checkbox", { name: /Codex CLI/ })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /ZCode/ })).toBeChecked();
  });

  it("shows the restart notice only while a restart-required client is checked", async () => {
    const user = userEvent.setup();
    setup();

    expect(screen.queryByText(/需重启 ZCode/)).not.toBeInTheDocument();

    await user.click(screen.getByRole("checkbox", { name: /ZCode/ }));
    expect(screen.getByText(/需重启 ZCode/)).toBeInTheDocument();

    await user.click(screen.getByRole("checkbox", { name: /ZCode/ }));
    expect(screen.queryByText(/需重启 ZCode/)).not.toBeInTheDocument();
  });

  it("submits the checked client keys", async () => {
    const user = userEvent.setup();
    const { onSubmit } = setup();

    await user.click(screen.getByRole("checkbox", { name: /ZCode/ }));
    await user.click(screen.getByRole("button", { name: "写入" }));

    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith(["codex", "zcode"]));
  });

  it("refuses to submit with nothing checked", async () => {
    const user = userEvent.setup();
    const { onSubmit } = setup();

    await user.click(screen.getByRole("checkbox", { name: /Codex CLI/ }));
    // An empty write would report success and do nothing.
    expect(screen.getByRole("button", { name: "写入" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "写入" }));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("disables every row and the submit button when the platform cannot write config", () => {
    setup({ capabilityDisabledReason: "该平台的原生配置写入尚未实现。" });

    expect(screen.getByRole("checkbox", { name: /Codex CLI/ })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: /ZCode/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "写入" })).toBeDisabled();
    expect(screen.getByText("该平台的原生配置写入尚未实现。")).toBeInTheDocument();
  });

  it("closes on Escape when not writing", async () => {
    const user = userEvent.setup();
    const { onClose } = setup();

    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
  });

  it("stays open on Escape while a write is in flight", async () => {
    const user = userEvent.setup();
    const { onClose } = setup({ loading: true });

    await user.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();
  });

  it("copies the pool endpoint for clients it cannot write", async () => {
    const user = userEvent.setup();
    const writeText = stubClipboard();
    setup();

    expect(screen.getByLabelText("Base URL")).toHaveValue("http://127.0.0.1:43111");
    await user.click(screen.getByLabelText("复制 Base URL"));
    expect(writeText).toHaveBeenLastCalledWith("http://127.0.0.1:43111");

    await user.click(screen.getByLabelText("复制 API Key"));
    expect(writeText).toHaveBeenLastCalledWith("sk-ai-switch-codex-key");
  });

  it("keeps the key masked until the user reveals it, and names the tab it belongs to", async () => {
    const user = userEvent.setup();
    setup();

    // Each agent tab has its own key, so the prose has to say which one this is.
    expect(screen.getByText(/每个智能体标签页的算力池端点 API Key 都不一样/)).toBeInTheDocument();
    expect(screen.getByText(/Codex 标签页的 Key/)).toBeInTheDocument();

    const key = screen.getByLabelText("API Key");
    expect(key).toHaveAttribute("type", "password");
    await user.click(screen.getByLabelText("显示 API Key"));
    expect(key).toHaveAttribute("type", "text");
    await user.click(screen.getByLabelText("隐藏 API Key"));
    expect(key).toHaveAttribute("type", "password");
  });

  it("cannot copy an endpoint value that has not been read yet", () => {
    setup({ poolApiKey: null });

    expect(screen.getByLabelText("复制 API Key")).toBeDisabled();
    expect(screen.getByLabelText("复制 Base URL")).toBeEnabled();
  });

  it("does not submit the write when Enter is pressed in an endpoint field", async () => {
    const user = userEvent.setup();
    const { onSubmit } = setup();

    await user.click(screen.getByLabelText("Base URL"));
    await user.keyboard("{Enter}");
    expect(onSubmit).not.toHaveBeenCalled();
  });
});
