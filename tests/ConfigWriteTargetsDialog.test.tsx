import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConfigWriteTargetsDialog } from "../src/components/accounts/ConfigWriteTargetsDialog";
import type { ConfigWriteClientStatus } from "../src/lib/api/types";

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
});
