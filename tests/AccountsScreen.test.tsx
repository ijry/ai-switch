import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createApiRouteCredential,
  deleteRouteCredential,
  getRoutePool,
  getRouteProxyStatus,
  importOfficialRouteCredentialsFromFiles,
  importOfficialRouteCredentialsFromText,
  listRouteCredentials,
  routePoolRouteOnce,
  setRoutePoolMembers,
  startRouteProxy,
  stopRouteProxy,
  updateRouteCredential,
  writeRouteProxyConfigs,
} from "../src/lib/api/client";
import { createQueryClient } from "../src/lib/query/queryClient";
import { AccountsScreen } from "../src/screens/AccountsScreen";
import type { RouteCredential } from "../src/lib/api/types";

vi.mock("../src/lib/api/client", () => ({
  createApiRouteCredential: vi.fn(),
  deleteRouteCredential: vi.fn(),
  getRoutePool: vi.fn(),
  getRouteProxyStatus: vi.fn(),
  importOfficialRouteCredentialsFromFiles: vi.fn(),
  importOfficialRouteCredentialsFromText: vi.fn(),
  listRouteCredentials: vi.fn(),
  routePoolRouteOnce: vi.fn(),
  setRoutePoolMembers: vi.fn(),
  startRouteProxy: vi.fn(),
  stopRouteProxy: vi.fn(),
  updateRouteCredential: vi.fn(),
  writeRouteProxyConfigs: vi.fn(),
}));

const credentialsFixture: RouteCredential[] = [
  {
    id: "cred-official-1",
    platform: "codex",
    kind: "official",
    display_name: "Team Account",
    email: "team@example.com",
    status: "ok",
    sort_order: 0,
    batch_id: "batch-1",
    secret_payload_json: "{\"access_token\":\"at\",\"refresh_token\":\"rt\"}",
    config_json: "{\"type\":\"codex\"}",
    preview_json: "{\"auth_json\":{}}",
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
  {
    id: "cred-api-1",
    platform: "codex",
    kind: "api",
    display_name: "API Account",
    email: null,
    status: "ok",
    sort_order: 1,
    batch_id: null,
    secret_payload_json: "{\"api_key\":\"sk-test\"}",
    config_json: "{\"base_url\":\"https://api.example.com/v1\",\"interface_format\":\"openai\"}",
    preview_json: "{\"config_toml\":\"\"}",
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

function renderScreen() {
  return render(
    <QueryClientProvider client={createQueryClient()}>
      <AccountsScreen platform="codex" />
    </QueryClientProvider>,
  );
}

describe("AccountsScreen", () => {
  beforeEach(() => {
    vi.mocked(createApiRouteCredential).mockReset();
    vi.mocked(deleteRouteCredential).mockReset();
    vi.mocked(getRoutePool).mockReset();
    vi.mocked(getRouteProxyStatus).mockReset();
    vi.mocked(importOfficialRouteCredentialsFromFiles).mockReset();
    vi.mocked(importOfficialRouteCredentialsFromText).mockReset();
    vi.mocked(listRouteCredentials).mockReset();
    vi.mocked(routePoolRouteOnce).mockReset();
    vi.mocked(setRoutePoolMembers).mockReset();
    vi.mocked(startRouteProxy).mockReset();
    vi.mocked(stopRouteProxy).mockReset();
    vi.mocked(updateRouteCredential).mockReset();
    vi.mocked(writeRouteProxyConfigs).mockReset();

    vi.mocked(listRouteCredentials).mockResolvedValue(credentialsFixture);
    vi.mocked(getRoutePool).mockResolvedValue({
      platform: "codex",
      account_ids: [],
      stats: {
        member_count: 0,
        request_count: 0,
        token_count: 0,
        cost_micros: 0,
        recent_logs: [],
      },
    });
    vi.mocked(getRouteProxyStatus).mockResolvedValue({
      running: false,
      bind_host: "127.0.0.1",
      port: null,
      base_url: null,
    });
    vi.mocked(setRoutePoolMembers).mockImplementation(async (input) => ({
      platform: input.platform,
      account_ids: input.account_ids,
      stats: {
        member_count: input.account_ids.length,
        request_count: 1,
        token_count: 4096,
        cost_micros: 2500,
        recent_logs: [],
      },
    }));
    vi.mocked(routePoolRouteOnce).mockResolvedValue({
      platform: "codex",
      selected_account_id: "cred-official-1",
      selected_account_name: "Team Account",
      stats: {
        member_count: 1,
        request_count: 2,
        token_count: 5120,
        cost_micros: 3700,
        recent_logs: [],
      },
    });
    vi.mocked(startRouteProxy).mockResolvedValue({
      running: true,
      bind_host: "127.0.0.1",
      port: 43111,
      base_url: "http://127.0.0.1:43111",
    });
    vi.mocked(stopRouteProxy).mockResolvedValue({
      running: false,
      bind_host: "127.0.0.1",
      port: null,
      base_url: null,
    });
    vi.mocked(writeRouteProxyConfigs).mockResolvedValue([
      {
        target_key: "codex",
        path: "C:\\Users\\test\\.codex\\config.toml",
        status: "written",
      },
    ]);
    vi.mocked(importOfficialRouteCredentialsFromText).mockResolvedValue({
      imported: [credentialsFixture[0]],
      failed: [],
    });
    vi.mocked(importOfficialRouteCredentialsFromFiles).mockResolvedValue({
      imported: [credentialsFixture[0]],
      failed: [],
    });
    vi.mocked(createApiRouteCredential).mockResolvedValue(credentialsFixture[1]);
    vi.mocked(updateRouteCredential).mockResolvedValue({
      ...credentialsFixture[0],
      display_name: "Updated Team Account",
    });
    vi.mocked(deleteRouteCredential).mockResolvedValue(undefined);
  });

  it("renders route credentials under the selected first-level agent tab and toggles pool membership", async () => {
    renderScreen();

    expect(await screen.findByText("Codex 账号")).toBeInTheDocument();
    expect(await screen.findByText("Team Account")).toBeInTheDocument();
    expect(screen.getByText("API Account")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("将 Team Account 加入算力池"));

    await waitFor(() =>
      expect(setRoutePoolMembers).toHaveBeenCalledWith({
        platform: "codex",
        account_ids: ["cred-official-1"],
      }),
    );
    expect(screen.getByText("已加入 1 个账号用于本地路由。")).toBeInTheDocument();
  });

  it("imports a single official CPA credential from the add dialog", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    fireEvent.change(screen.getByLabelText("CPA JSON"), {
      target: {
        value: "{\"type\":\"codex\",\"email\":\"new@example.com\",\"access_token\":\"at\"}",
      },
    });
    await userEvent.type(screen.getByLabelText("导入批量名称"), "Codex Batch");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(importOfficialRouteCredentialsFromText).toHaveBeenCalledWith({
        platform: "codex",
        text: "{\"type\":\"codex\",\"email\":\"new@example.com\",\"access_token\":\"at\"}",
        batch_name: "Codex Batch",
      }),
    );
  });

  it("imports official credentials from multiple file paths", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "官方批量" }));
    fireEvent.change(screen.getByLabelText("批量文件路径"), {
      target: { value: "C:\\one.json\nC:\\two.json" },
    });
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(importOfficialRouteCredentialsFromFiles).toHaveBeenCalledWith({
        platform: "codex",
        file_paths: ["C:\\one.json", "C:\\two.json"],
        batch_name: null,
      }),
    );
  });

  it("creates an API route credential with interface format and model mappings", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Upstream API");
    await userEvent.type(screen.getByLabelText("API Key"), "c2stMQ==");
    await userEvent.click(screen.getByLabelText("Base64 解码 API Key"));
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://api.upstream.test/v1");
    await userEvent.selectOptions(screen.getByLabelText("接口格式"), "openai-responses");
    fireEvent.change(screen.getByLabelText("模型映射 JSON"), {
      target: { value: "[{\"from\":\"gpt-5\",\"to\":\"up-gpt\"}]" },
    });
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith({
        platform: "codex",
        display_name: "Upstream API",
        api_key: "sk-1",
        base_url: "https://api.upstream.test/v1",
        interface_format: "openai-responses",
        model_mappings_json: "[{\"from\":\"gpt-5\",\"to\":\"up-gpt\"}]",
        preview_json: null,
        batch_id: null,
      }),
    );
  });

  it("edits route credential details from the right-side drawer", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 Team Account" }));
    await userEvent.clear(screen.getByLabelText("编辑账号名称"));
    await userEvent.type(screen.getByLabelText("编辑账号名称"), "Updated Team Account");
    await userEvent.selectOptions(screen.getByLabelText("编辑状态"), "warning");
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() =>
      expect(updateRouteCredential).toHaveBeenCalledWith("cred-official-1", {
        display_name: "Updated Team Account",
        email: "team@example.com",
        status: "warning",
        secret_payload_json: "{\n  \"access_token\": \"at\",\n  \"refresh_token\": \"rt\"\n}",
        config_json: "{\n  \"type\": \"codex\"\n}",
        preview_json: "{\n  \"auth_json\": {}\n}",
      }),
    );
  });

  it("starts proxy, writes configs, and tests the credential pool route", async () => {
    renderScreen();

    expect(await screen.findByText("本地代理：未启动")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    await waitFor(() => expect(startRouteProxy).toHaveBeenCalled());
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("写入路由配置文件"));
    await waitFor(() =>
      expect(writeRouteProxyConfigs).toHaveBeenCalledWith("http://127.0.0.1:43111"),
    );
    expect(screen.getByText("配置写入结果")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("将 Team Account 加入算力池"));
    await userEvent.click(screen.getByLabelText("测试算力池路由"));
    await waitFor(() =>
      expect(routePoolRouteOnce).toHaveBeenCalledWith({
        platform: "codex",
        token_count: 1024,
        cost_micros: 1200,
        metadata_json: JSON.stringify({ source: "ui_test_route" }),
      }),
    );
    expect(screen.getByText("最近路由到：Team Account")).toBeInTheDocument();
  });
});
