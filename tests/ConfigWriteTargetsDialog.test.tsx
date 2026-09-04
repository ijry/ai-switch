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

/** The endpoint parameters have their own tab, so they are never on screen first. */
async function openManualTab(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("tab", { name: "其他 Agent" }));
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

  it("disables every row and the submit button when the platform cannot write config", async () => {
    const user = userEvent.setup();
    setup({ capabilityDisabledReason: "该平台的原生配置写入尚未实现。" });

    // The reason belongs to the dialog, not to a panel: the tab it explains is
    // the one a platform that cannot be written gets sent away from.
    expect(screen.getByText("该平台的原生配置写入尚未实现。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "写入" })).toBeDisabled();

    await user.click(screen.getByRole("tab", { name: "内置支持" }));
    expect(screen.getByRole("checkbox", { name: /Codex CLI/ })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: /ZCode/ })).toBeDisabled();
  });

  it("opens on the built-in clients and keeps the endpoint parameters one tab away", async () => {
    const user = userEvent.setup();
    setup();

    expect(screen.getByRole("tab", { name: "内置支持" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    // They used to sit below the client list, where nobody who did not scroll
    // knew they were there.
    expect(screen.queryByLabelText("Base URL")).not.toBeInTheDocument();

    await openManualTab(user);
    expect(screen.getByLabelText("Base URL")).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: /Codex CLI/ })).not.toBeInTheDocument();
  });

  it("opens on the endpoint parameters when the platform cannot be written at all", () => {
    setup({ capabilityDisabledReason: "该平台的原生配置写入尚未实现。" });

    // Every checkbox on the other tab is dead, so the copyable parameters are
    // the only thing the dialog can still do for this platform.
    expect(screen.getByRole("tab", { name: "其他 Agent" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByLabelText("Base URL")).toBeInTheDocument();
  });

  it("keeps a write failure visible from either tab", async () => {
    const user = userEvent.setup();
    setup({ error: "写入 ZCode 失败：配置文件无法解析。" });

    expect(screen.getByRole("alert")).toHaveTextContent("写入 ZCode 失败");
    await openManualTab(user);
    expect(screen.getByRole("alert")).toHaveTextContent("写入 ZCode 失败");
  });

  it("moves between tabs with the arrow keys", async () => {
    const user = userEvent.setup();
    setup();

    await user.click(screen.getByRole("tab", { name: "内置支持" }));
    await user.keyboard("{ArrowRight}");

    expect(screen.getByRole("tab", { name: "其他 Agent" })).toHaveFocus();
    expect(screen.getByLabelText("Base URL")).toBeInTheDocument();
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
    await openManualTab(user);

    // Codex reads the base URL as-is and appends /responses, so the endpoint the
    // user copies has to carry /v1 exactly like the written config does.
    expect(screen.getByLabelText("Base URL")).toHaveValue("http://127.0.0.1:43111/v1");
    await user.click(screen.getByLabelText("复制 Base URL"));
    expect(writeText).toHaveBeenLastCalledWith("http://127.0.0.1:43111/v1");

    await user.click(screen.getByLabelText("复制 API Key"));
    expect(writeText).toHaveBeenLastCalledWith("sk-ai-switch-codex-key");
  });

  it("keeps the key masked until the user reveals it, and names the tab it belongs to", async () => {
    const user = userEvent.setup();
    setup();
    await openManualTab(user);

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

  it("cannot copy an endpoint value that has not been read yet", async () => {
    const user = userEvent.setup();
    setup({ poolApiKey: null });
    await openManualTab(user);

    expect(screen.getByLabelText("复制 API Key")).toBeDisabled();
    expect(screen.getByLabelText("复制 Base URL")).toBeEnabled();
  });

  it("does not submit the write when Enter is pressed in an endpoint field", async () => {
    const user = userEvent.setup();
    const { onSubmit } = setup();
    await openManualTab(user);

    await user.click(screen.getByLabelText("Base URL"));
    await user.keyboard("{Enter}");
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("lists the HTTPS endpoint beside the HTTP one, both carrying /v1 for codex", async () => {
    const user = userEvent.setup();
    const writeText = stubClipboard();
    setup({
      poolBaseUrl: "http://127.0.0.1:19527",
      poolHttpsBaseUrl: "https://127.0.0.1:19528",
    });
    await openManualTab(user);

    expect(screen.getByLabelText("Base URL")).toHaveValue("http://127.0.0.1:19527/v1");
    expect(screen.getByLabelText("HTTPS Base URL")).toHaveValue("https://127.0.0.1:19528/v1");
    await user.click(screen.getByLabelText("复制 HTTPS Base URL"));
    expect(writeText).toHaveBeenLastCalledWith("https://127.0.0.1:19528/v1");

    expect(screen.getByText(/正常情况用上面的 Base URL（HTTP）即可/)).toBeInTheDocument();
    expect(screen.getByText(/必须信任本地根证书/)).toBeInTheDocument();
  });

  it("omits the HTTPS row when HTTPS is off", async () => {
    const user = userEvent.setup();
    setup({ poolBaseUrl: "http://127.0.0.1:19527" });
    await openManualTab(user);

    expect(screen.queryByLabelText("HTTPS Base URL")).not.toBeInTheDocument();
    expect(screen.getByText(/可在设置里开启 HTTPS/)).toBeInTheDocument();
  });

  it("says HTTPS failed rather than telling the user to turn it on", async () => {
    const user = userEvent.setup();
    setup({
      poolBaseUrl: "http://127.0.0.1:19527",
      httpsError: "Could not load local route proxy HTTPS certificate (missing-cert.pem)",
    });
    await openManualTab(user);

    expect(screen.getByText(/HTTPS 端点本次未能启动/)).toBeInTheDocument();
    expect(screen.queryByText(/可在设置里开启 HTTPS/)).not.toBeInTheDocument();
  });

  it("leaves a claude endpoint bare", async () => {
    const user = userEvent.setup();
    setup({
      clients: [
        { ...clients[0], client_key: "claude", display_name: "Claude Code", platform: "claude" },
      ],
      platform: "claude",
      poolBaseUrl: "http://127.0.0.1:19527",
    });
    await openManualTab(user);

    expect(screen.getByLabelText("Base URL")).toHaveValue("http://127.0.0.1:19527");
  });
});
