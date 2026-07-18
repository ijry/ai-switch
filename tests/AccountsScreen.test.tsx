import { QueryClientProvider } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createBatch,
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
import { recognizeApiKeysFromImageBlob } from "../src/lib/ocr/apiKeyOcr";
import { createQueryClient } from "../src/lib/query/queryClient";
import { AccountsScreen } from "../src/screens/AccountsScreen";
import type { RouteCredential, RoutePoolStats } from "../src/lib/api/types";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("../src/lib/api/client", () => ({
  createBatch: vi.fn(),
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

vi.mock("../src/lib/ocr/apiKeyOcr", async () => {
  const actual = await vi.importActual<typeof import("../src/lib/ocr/apiKeyOcr")>("../src/lib/ocr/apiKeyOcr");
  return {
    ...actual,
    recognizeApiKeysFromImageBlob: vi.fn(),
  };
});

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
    config_json: "{\"base_url\":\"https://api.example.com/v1\",\"interface_format\":\"openai\",\"model_mappings\":[{\"from\":\"gpt-5\",\"to\":\"old-upstream\"}]}",
    preview_json: "{\"config_toml\":\"\"}",
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

function statsFixture(overrides: Partial<RoutePoolStats> = {}): RoutePoolStats {
  return {
    member_count: 0,
    request_count: 0,
    token_count: 0,
    cost_micros: 0,
    recent_logs: [],
    requests: [],
    request_row_count: 0,
    request_page: 1,
    request_page_size: 20,
    ...overrides,
  };
}

function renderScreen(platform: "codex" | "claude" = "codex") {
  return render(
    <QueryClientProvider client={createQueryClient()}>
      <AccountsScreen platform={platform} />
    </QueryClientProvider>,
  );
}

describe("AccountsScreen", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  beforeEach(() => {
    vi.mocked(open).mockReset();
    vi.mocked(createBatch).mockReset();
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
    vi.mocked(recognizeApiKeysFromImageBlob).mockReset();

    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: undefined,
    });

    vi.mocked(open).mockResolvedValue(null);
    vi.mocked(createBatch).mockResolvedValue({
      id: "batch-api-1",
      name: "Upstream API 批量",
      source: "api_route_credentials",
      notes: null,
      sort_order: 0,
      created_at: "2026-07-13T00:00:00Z",
      updated_at: "2026-07-13T00:00:00Z",
    });
    vi.mocked(listRouteCredentials).mockResolvedValue(credentialsFixture);
    vi.mocked(getRoutePool).mockResolvedValue({
      platform: "codex",
      account_ids: [],
      stats: statsFixture(),
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
      stats: statsFixture({
        member_count: input.account_ids.length,
        request_count: 1,
        token_count: 4096,
        cost_micros: 2500,
      }),
    }));
    vi.mocked(routePoolRouteOnce).mockResolvedValue({
      platform: "codex",
      selected_account_id: "cred-official-1",
      selected_account_name: "Team Account",
      stats: statsFixture({
        member_count: 1,
        request_count: 2,
        token_count: 5120,
        cost_micros: 3700,
      }),
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
        route_proxy_key: "sk-ai-switch-test",
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
    expect(screen.getByText("已加入 1 个账号")).toBeInTheDocument();
  });

  it("imports a single official CPA credential from the add dialog", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "官方导入" }));
    fireEvent.change(screen.getByLabelText("账号 JSON"), {
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
    await userEvent.click(screen.getByRole("button", { name: "官方导入" }));
    vi.mocked(open).mockResolvedValue(["C:\\one.json", "C:\\two.json"]);
    await userEvent.click(screen.getByRole("button", { name: "导入 JSON 文件" }));
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
    fireEvent.change(screen.getByLabelText("上游模型 1"), {
      target: { value: "up-gpt" },
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

  it("creates multiple API keys as one batch", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Upstream API");
    fireEvent.change(screen.getByLabelText("API Key"), {
      target: { value: "sk-one\nsk-two" },
    });
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://api.upstream.test/v1");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createBatch).toHaveBeenCalledWith({
        name: "Upstream API 批量",
        source: "api_route_credentials",
        notes: null,
      }),
    );
    expect(createApiRouteCredential).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({
        display_name: "Upstream API 1",
        api_key: "sk-one",
        batch_id: "batch-api-1",
      }),
    );
    expect(createApiRouteCredential).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({
        display_name: "Upstream API 2",
        api_key: "sk-two",
        batch_id: "batch-api-1",
      }),
    );
  });

  it("recognizes an API key from a clipboard image and replaces the current input", async () => {
    const imageBlob = new Blob(["fake"], { type: "image/png" });
    const clipboardItem = {
      getType: vi.fn().mockResolvedValue(imageBlob),
      types: ["image/png"],
    };
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        read: vi.fn().mockResolvedValue([clipboardItem]),
      },
    });
    vi.mocked(recognizeApiKeysFromImageBlob).mockResolvedValue("sk-from-clipboard-123456");
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.type(screen.getByLabelText("API Key"), "sk-old");
    await userEvent.click(screen.getByRole("button", { name: "OCR识别 API Key" }));

    await waitFor(() => expect(recognizeApiKeysFromImageBlob).toHaveBeenCalledWith(imageBlob));
    expect(screen.getByLabelText("API Key")).toHaveValue("sk-from-clipboard-123456");
  });

  it("falls back to a selected image file when the clipboard has no image", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        read: vi.fn().mockResolvedValue([]),
      },
    });
    vi.mocked(recognizeApiKeysFromImageBlob).mockResolvedValue("sk-from-file-123456");
    const imageFile = new File(["fake"], "apikey.png", { type: "image/png" });
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "OCR识别 API Key" }));
    await userEvent.upload(screen.getByLabelText("选择图片识别 API Key"), imageFile);

    await waitFor(() => expect(recognizeApiKeysFromImageBlob).toHaveBeenCalledWith(imageFile));
    expect(screen.getByLabelText("API Key")).toHaveValue("sk-from-file-123456");
  });

  it("uses fixed source model options for Claude model mappings", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Claude API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-claude");
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://api.anthropic.test");
    await userEvent.selectOptions(screen.getByLabelText("接口格式"), "anthropic");
    await userEvent.selectOptions(screen.getByLabelText("请求模型 1"), "claude-sonnet");
    await userEvent.type(screen.getByLabelText("上游模型 1"), "provider-sonnet");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith({
        platform: "claude",
        display_name: "Claude API",
        api_key: "sk-claude",
        base_url: "https://api.anthropic.test",
        interface_format: "anthropic",
        model_mappings_json: "[{\"from\":\"claude-sonnet\",\"to\":\"provider-sonnet\"}]",
        preview_json: null,
        batch_id: null,
      }),
    );
  });

  it("edits API credential model mappings through the visual editor", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    fireEvent.change(screen.getByLabelText("上游模型 1"), {
      target: { value: "new-upstream" },
    });
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const updateInput = vi.mocked(updateRouteCredential).mock.calls[0][1];
    expect(JSON.parse(updateInput.config_json).model_mappings).toEqual([
      { from: "gpt-5", to: "new-upstream" },
    ]);
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

  it("renders filtered route request statistics and paginates request rows", async () => {
    const expectedMonthStart = new Date();
    expectedMonthStart.setHours(0, 0, 0, 0);
    expectedMonthStart.setDate(1);

    vi.mocked(getRoutePool).mockImplementation(
      async (platform, since, requestPage = 1, requestPageSize = 20) => ({
        platform,
        account_ids: ["cred-official-1"],
        stats: statsFixture({
          member_count: 1,
          request_count: 99,
          token_count: 2048,
          cost_micros: 1500,
          request_row_count: 42,
          request_page: requestPage ?? 1,
          request_page_size: requestPageSize ?? 20,
          requests: [
            {
              id: `request-${requestPage ?? 1}-${since ?? "all"}`,
              account_id: "cred-official-1",
              account_name: "Team Account",
              source_label: "route_proxy",
              metric_type: "request",
              amount: 1,
              unit: "count",
              metadata_json: "{\"path\":\"/v1/responses\",\"status\":201}",
              created_at: "2026-07-17T08:00:00Z",
            },
            {
              id: "request-invalid-metadata",
              account_id: "cred-api-1",
              account_name: "Broken Metadata Account",
              source_label: "route_proxy",
              metric_type: "request",
              amount: 1,
              unit: "count",
              metadata_json: "{bad json",
              created_at: "2026-07-17T08:01:00Z",
            },
          ],
        }),
      }),
    );

    renderScreen();

    await userEvent.click(await screen.findByLabelText("查看算力池统计"));

    expect(await screen.findByText("请求统计")).toBeInTheDocument();
    expect(screen.getByText("统计当前 Codex 的历史路由请求")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "当日" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "本周" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "本月" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "累计" })).toBeInTheDocument();
    expect(screen.getByText("共 42 条 · 每页 20 条")).toBeInTheDocument();
    expect(screen.getByText("第 1 / 3 页")).toBeInTheDocument();
    expect(screen.getByText("/v1/responses")).toBeInTheDocument();
    expect(screen.getByText("201")).toBeInTheDocument();
    expect(screen.getAllByText("route_proxy")).toHaveLength(2);
    const invalidMetadataRow = screen.getByText("Broken Metadata Account").closest("div");
    expect(invalidMetadataRow).not.toBeNull();
    expect(within(invalidMetadataRow as HTMLElement).getAllByText("-")).toHaveLength(2);

    await userEvent.click(screen.getByLabelText("下一页请求"));

    await waitFor(() =>
      expect(getRoutePool).toHaveBeenLastCalledWith(
        "codex",
        expect.any(String),
        2,
        20,
      ),
    );

    await userEvent.click(screen.getByRole("button", { name: "本月" }));

    await waitFor(() =>
      expect(getRoutePool).toHaveBeenLastCalledWith(
        "codex",
        expectedMonthStart.toISOString(),
        1,
        20,
      ),
    );

    await userEvent.click(screen.getByRole("button", { name: "累计" }));

    await waitFor(() => expect(getRoutePool).toHaveBeenLastCalledWith("codex", null, 1, 20));
  });

  it("auto refreshes route statistics only while the panel is open", async () => {
    vi.mocked(getRoutePool).mockResolvedValue({
      platform: "codex",
      account_ids: [],
      stats: statsFixture({
        request_row_count: 0,
        request_page: 1,
        request_page_size: 20,
      }),
    });

    renderScreen();

    await screen.findByText("Codex 账号");
    expect(getRoutePool).toHaveBeenCalledTimes(1);

    vi.useFakeTimers();

    act(() => {
      fireEvent.click(screen.getByLabelText("查看算力池统计"));
    });

    await act(async () => {
      await Promise.resolve();
    });

    expect(getRoutePool).toHaveBeenCalledTimes(2);

    await act(async () => {
      vi.advanceTimersByTime(5000);
      await Promise.resolve();
    });

    expect(getRoutePool).toHaveBeenCalledTimes(3);

    act(() => {
      fireEvent.click(screen.getByLabelText("查看算力池统计"));
    });

    await act(async () => {
      vi.advanceTimersByTime(5000);
      await Promise.resolve();
    });

    expect(getRoutePool).toHaveBeenCalledTimes(3);
  });

  it("starts proxy, writes configs, and tests the credential pool route", async () => {
    renderScreen();

    expect(await screen.findByText("本地代理：未启动")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    await waitFor(() => expect(startRouteProxy).toHaveBeenCalled());
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("写入路由配置文件"));
    await waitFor(() =>
      expect(writeRouteProxyConfigs).toHaveBeenCalledWith("http://127.0.0.1:43111", "codex"),
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

  it("clears route config write results after a short delay", async () => {
    renderScreen();

    await screen.findByText("本地代理：未启动");
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    vi.useFakeTimers();
    fireEvent.click(screen.getByLabelText("写入路由配置文件"));
    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.getByText("配置写入结果")).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(2999);
    });
    expect(screen.getByText("配置写入结果")).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.queryByText("配置写入结果")).not.toBeInTheDocument();
  });
});
