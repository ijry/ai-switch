import { QueryClientProvider } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  archiveRouteCredentials,
  createBatch,
  copyRouteCredential,
  createApiRouteCredential,
  deleteRouteCredential,
  fetchRouteModels,
  getRoutePool,
  getRouteProxyKey,
  getRouteProxyStatus,
  getSettings,
  importOfficialRouteCredentialsFromFiles,
  importOfficialRouteCredentialsFromText,
  listPlatformCapabilities,
  listRouteCredentials,
  listRouteCredentialPage,
  reorderRouteCredentials,
  refreshRouteCredentialsQuota,
  restoreRouteCredentials,
  routePoolTestModel,
  saveSettings,
  setRouteCredentialRecovery,
  setRouteCredentialStatuses,
  setRoutePoolMembers,
  startRouteProxy,
  stopRouteProxy,
  subscribeRouteProxyLiveLog,
  unsubscribeRouteProxyLiveLog,
  updateRouteCredential,
  writeRouteProxyConfigs,
} from "../src/lib/api/client";
import { recognizeApiKeysFromImageBlob } from "../src/lib/ocr/apiKeyOcr";
import { CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY } from "../src/lib/codexModelTestEndpoint";
import { MODEL_TEST_MODELS_STORAGE_KEY } from "../src/lib/modelTestModels";
import { createQueryClient } from "../src/lib/query/queryClient";
import { settingsFixture } from "../src/test/fixtures";
import { AccountsScreen } from "../src/screens/AccountsScreen";
import { fetchRouteProxyModels } from "../src/lib/routeProxyModels";
import type {
  CapabilityAvailability,
  CapabilityRule,
  PlatformCapability,
  PlatformId,
  RouteCredential,
  RouteCredentialActivityEvent,
  RoutePoolModelTestOutcome,
  RoutePoolStats,
  RoutePoolUsageLog,
} from "../src/lib/api/types";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("../src/lib/api/client", () => ({
  archiveRouteCredentials: vi.fn(),
  createBatch: vi.fn(),
  copyRouteCredential: vi.fn(),
  createApiRouteCredential: vi.fn(),
  deleteRouteCredential: vi.fn(),
  fetchRouteModels: vi.fn(),
  getRoutePool: vi.fn(),
  getRouteProxyKey: vi.fn(),
  getRouteProxyStatus: vi.fn(),
  importOfficialRouteCredentialsFromFiles: vi.fn(),
  importOfficialRouteCredentialsFromText: vi.fn(),
  listPlatformCapabilities: vi.fn(),
  listRouteCredentials: vi.fn(),
  listRouteCredentialPage: vi.fn(),
  reorderRouteCredentials: vi.fn(),
  refreshRouteCredentialsQuota: vi.fn(),
  restoreRouteCredentials: vi.fn(),
  routePoolTestModel: vi.fn(),
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
  setRouteCredentialRecovery: vi.fn(),
  setRouteCredentialStatuses: vi.fn(),
  setRoutePoolMembers: vi.fn(),
  startRouteProxy: vi.fn(),
  stopRouteProxy: vi.fn(),
  subscribeRouteProxyLiveLog: vi.fn(),
  unsubscribeRouteProxyLiveLog: vi.fn(),
  updateRouteCredential: vi.fn(),
  writeRouteProxyConfigs: vi.fn(),
}));

const transportTestState = vi.hoisted(() => ({
  activityHandler: null as ((payload: unknown) => void) | null,
  statusHandler: null as ((payload: unknown) => void) | null,
  liveLogHandler: null as ((payload: unknown) => void) | null,
  subscribe: vi.fn(),
}));

vi.mock("../src/lib/transport", () => ({
  getTransport: () => transportTestState,
  isTauriRuntime: () => false,
}));

vi.mock("../src/lib/routeProxyModels", () => ({
  fetchRouteProxyModels: vi.fn(),
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
    route_priority: 3,
    max_concurrency: 1,
    batch_id: "batch-1",
    batch_name: "Codex Batch",
    secret_payload_json: "{\"access_token\":\"at\",\"refresh_token\":\"rt\"}",
    config_json: "{\"type\":\"codex\"}",
    preview_json: "{\"auth_json\":{}}",
    request_count: 3,
    success_count: 2,
    failure_count: 1,
    success_rate: 66.6667,
    active_request_count: 0,
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
    route_priority: 3,
    max_concurrency: 1,
    batch_id: null,
    batch_name: null,
    secret_payload_json: "{\"api_key\":\"sk-test\"}",
    config_json: "{\"base_url\":\"https://api.example.com/v1\",\"interface_format\":\"openai\",\"model_mappings\":[{\"from\":\"gpt-5\",\"to\":\"old-upstream\"}]}",
    preview_json: "{\"config_toml\":\"\"}",
    request_count: 0,
    success_count: 0,
    failure_count: 0,
    success_rate: null,
    active_request_count: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

let poolStateByPlatform = new Map<string, string[]>();

function statsFixture(overrides: Partial<RoutePoolStats> = {}): RoutePoolStats {
  return {
    member_count: 0,
    request_count: 0,
    token_count: 0,
    input_token_count: 0,
    output_token_count: 0,
    cache_token_count: 0,
    cost_micros: 0,
    recent_logs: [],
    requests: [],
    request_row_count: 0,
    request_page: 1,
    request_page_size: 20,
    ...overrides,
  };
}

function modelTestOutcomeFixture(
  overrides: Partial<RoutePoolModelTestOutcome> = {},
): RoutePoolModelTestOutcome {
  return {
    platform: "codex",
    selected_account_id: "cred-official-1",
    selected_account_name: "Team Account",
    interface_format: "openai",
    request_path: "/chat/completions",
    base_url: "https://api.example.com/v1",
    target_url: "https://api.example.com/v1/chat/completions",
    request_body_json: JSON.stringify(
      {
        model: "gpt-5",
        messages: [{ role: "user", content: "Reply with exactly: ai-switch-ok" }],
        temperature: 0,
        max_tokens: 16,
      },
      null,
      2,
    ),
    response_status: 200,
    response_body: "{\"choices\":[{\"message\":{\"content\":\"ai-switch-ok\"}}]}",
    response_text: "ai-switch-ok",
    error_message: null,
    success: true,
    duration_ms: 321,
    stats: statsFixture({
      member_count: 1,
      request_count: 1,
      token_count: 8,
      cost_micros: 42,
    }),
    ...overrides,
  };
}

function renderScreen(
  platform: PlatformId = "codex",
  initialView: "in_pool" | "out_of_pool" | "archived" = "out_of_pool",
  sidebarCollapsed = false,
) {
  const result = render(
    <QueryClientProvider client={createQueryClient()}>
      <AccountsScreen platform={platform} sidebarCollapsed={sidebarCollapsed} />
    </QueryClientProvider>,
  );
  if (initialView !== "in_pool") {
    act(() => {
      fireEvent.click(
        screen.getByRole("button", { name: initialView === "archived" ? "已归档" : "未入池" }),
      );
    });
  }
  return result;
}

async function selectAccountView(name: "算力池" | "未入池" | "已归档" | "统计") {
  const button = screen.getByRole("button", { name });
  if (button.getAttribute("aria-pressed") !== "true") {
    await userEvent.click(button);
  }
  await waitFor(() =>
    expect(screen.getByRole("button", { name })).toHaveAttribute("aria-pressed", "true"),
  );
}

describe("AccountsScreen", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  beforeEach(() => {
    window.localStorage.clear();
    vi.mocked(open).mockReset();
    vi.mocked(archiveRouteCredentials).mockReset();
    vi.mocked(createBatch).mockReset();
    vi.mocked(copyRouteCredential).mockReset();
    vi.mocked(createApiRouteCredential).mockReset();
    vi.mocked(deleteRouteCredential).mockReset();
    vi.mocked(fetchRouteModels).mockReset();
    vi.mocked(getRoutePool).mockReset();
    vi.mocked(getRouteProxyKey).mockReset();
    vi.mocked(getRouteProxyStatus).mockReset();
    vi.mocked(importOfficialRouteCredentialsFromFiles).mockReset();
    vi.mocked(importOfficialRouteCredentialsFromText).mockReset();
    vi.mocked(listPlatformCapabilities).mockReset();
    vi.mocked(listRouteCredentials).mockReset();
    vi.mocked(listRouteCredentialPage).mockReset();
    vi.mocked(reorderRouteCredentials).mockReset();
    vi.mocked(refreshRouteCredentialsQuota).mockReset();
    vi.mocked(restoreRouteCredentials).mockReset();
    vi.mocked(routePoolTestModel).mockReset();
    vi.mocked(setRouteCredentialRecovery).mockReset();
    transportTestState.activityHandler = null;
    transportTestState.statusHandler = null;
    transportTestState.liveLogHandler = null;
    transportTestState.subscribe.mockReset();
    transportTestState.subscribe.mockImplementation(
      async (event: string, handler: (payload: unknown) => void) => {
        if (event === "route-credential-activity") {
          transportTestState.activityHandler = handler;
        }
        if (event === "route-credential-status") {
          transportTestState.statusHandler = handler;
        }
        if (event === "route-proxy-live-log") {
          transportTestState.liveLogHandler = handler;
        }
        return () => undefined;
      },
    );
    vi.mocked(setRoutePoolMembers).mockReset();
    vi.mocked(startRouteProxy).mockReset();
    vi.mocked(stopRouteProxy).mockReset();
    vi.mocked(subscribeRouteProxyLiveLog).mockReset();
    vi.mocked(subscribeRouteProxyLiveLog).mockResolvedValue([]);
    vi.mocked(unsubscribeRouteProxyLiveLog).mockReset();
    vi.mocked(unsubscribeRouteProxyLiveLog).mockResolvedValue(undefined);
    vi.mocked(updateRouteCredential).mockReset();
    vi.mocked(writeRouteProxyConfigs).mockReset();
    vi.mocked(fetchRouteProxyModels).mockReset();
    vi.mocked(recognizeApiKeysFromImageBlob).mockReset();

    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: vi.fn().mockResolvedValue(undefined),
        read: vi.fn().mockResolvedValue([]),
      },
    });

    vi.mocked(open).mockResolvedValue(null);
    const rule = (
      availability: CapabilityAvailability,
      reason_code: string | null = null,
      credential_kinds: string[] = [],
    ): CapabilityRule => ({
      availability,
      reason_code,
      credential_kinds,
      requires_base_url: availability === "partial",
      requires_api_dialect: availability === "partial",
    });
    const supportedCapability = (platform: PlatformId, display_name: string): PlatformCapability => ({
      platform,
      display_name,
      support_level: "supported",
      operations: {
        route_credentials: rule("supported"),
        generic_api_routing: rule("supported"),
        config_write: rule("supported"),
        official_import: rule("supported"),
        official_account_routing: rule("supported"),
        deeplink_import: rule("supported"),
        official_quota:
          platform === "gemini"
            ? rule("unavailable", "capability.quota_unavailable")
            : rule("supported"),
        model_test: rule("supported"),
        terminal_launch: rule("supported"),
        session_resume: rule("supported"),
      },
    });
    const partialCapability = (platform: PlatformId, display_name: string): PlatformCapability => ({
      platform,
      display_name,
      support_level: "partial",
      operations: {
        route_credentials: rule("supported"),
        generic_api_routing: rule("partial", "capability.api_credentials_only", ["api"]),
        config_write: rule("unavailable", "capability.native_config_unavailable"),
        official_import: rule("unavailable", "capability.official_account_unavailable"),
        official_account_routing: rule("unavailable", "capability.official_account_unavailable"),
        deeplink_import: rule("unavailable", "capability.deeplink_unavailable"),
        official_quota: rule("unavailable", "capability.quota_unavailable"),
        model_test: rule("partial", "capability.api_credentials_only", ["api"]),
        terminal_launch: rule("supported"),
        session_resume: rule("supported"),
      },
    });
    vi.mocked(listPlatformCapabilities).mockResolvedValue([
      supportedCapability("codex", "Codex"),
      supportedCapability("claude", "Claude"),
      supportedCapability("gemini", "Gemini"),
      supportedCapability("grok", "Grok"),
      partialCapability("opencode", "OpenCode"),
      partialCapability("openclaw", "OpenClaw"),
      partialCapability("hermes", "Hermes"),
    ]);
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
    vi.mocked(getSettings).mockResolvedValue(settingsFixture);
    vi.mocked(saveSettings).mockImplementation(async (settings) => settings);
    vi.mocked(archiveRouteCredentials).mockResolvedValue(undefined);
    vi.mocked(refreshRouteCredentialsQuota).mockResolvedValue([]);
    vi.mocked(restoreRouteCredentials).mockResolvedValue(undefined);
    poolStateByPlatform = new Map<string, string[]>([["codex", []]]);
    vi.mocked(getRoutePool).mockImplementation(async (platform) => ({
      platform,
      account_ids: [...(poolStateByPlatform.get(platform) ?? [])],
      stats: statsFixture({
        member_count: (poolStateByPlatform.get(platform) ?? []).length,
      }),
    }));
    vi.mocked(listRouteCredentialPage).mockImplementation(async (input) => {
      const poolIds = poolStateByPlatform.get(input.platform) ?? [];
      const source = await vi.mocked(listRouteCredentials)(input.platform);
      const scoped = source.filter((credential) => {
        if (input.pool_scope === "archived") {
          return Boolean(credential.archived_at);
        }
        return input.pool_scope === "in_pool"
          ? !credential.archived_at && poolIds.includes(credential.id)
          : !credential.archived_at && !poolIds.includes(credential.id);
      });
      const filtered = input.filters.length
        ? scoped.filter((credential) => input.filters.includes(credential.batch_id ?? "__single__"))
        : scoped;
      return {
        items: filtered,
        total: filtered.length,
        page: 1,
        page_count: 1,
        page_size: input.page_size,
        previous_page_account_id: null,
        next_page_account_id: null,
        filter_options: [
          ...Array.from(
            new Map(
              source.map((credential) => [
                credential.batch_id ?? "__single__",
                credential.batch_name ?? "单账号",
              ]),
            ),
          ).map(([key, label]) => ({ key, label })),
        ],
        official_account_count: filtered.filter((credential) => credential.kind === "official").length,
      };
    });
    vi.mocked(reorderRouteCredentials).mockResolvedValue({
      items: credentialsFixture,
      total: credentialsFixture.length,
      page: 1,
      page_count: 1,
      page_size: 20,
      previous_page_account_id: null,
      next_page_account_id: null,
      filter_options: [],
      official_account_count: 1,
    });
    vi.mocked(getRouteProxyStatus).mockResolvedValue({
      running: false,
      bind_host: "127.0.0.1",
      port: null,
      base_url: null,
    });
    vi.mocked(getRouteProxyKey).mockResolvedValue("sk-ai-switch-codex-key");
    vi.mocked(setRoutePoolMembers).mockImplementation(async (input) => {
      poolStateByPlatform.set(input.platform, [...input.account_ids]);
      return {
        platform: input.platform,
        account_ids: [...input.account_ids],
        stats: statsFixture({
          member_count: input.account_ids.length,
          request_count: 1,
          token_count: 4096,
          cost_micros: 2500,
        }),
      };
    });
    vi.mocked(routePoolTestModel).mockResolvedValue(modelTestOutcomeFixture());
    vi.mocked(setRouteCredentialStatuses).mockResolvedValue(undefined);
    vi.mocked(setRouteCredentialRecovery).mockResolvedValue(credentialsFixture[0]);
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
        operation_id: "operation-1",
        snapshot_id: "snapshot-1",
        target_app_id: "target-codex",
        target_key: "codex",
        platform: "codex",
        path: "C:\\Users\\test\\.codex\\config.toml",
        status: "succeeded",
        before_hash: "before-hash",
        after_hash: "after-hash",
        error_code: null,
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
    vi.mocked(copyRouteCredential).mockImplementation(async (id: string) => {
      const source = credentialsFixture.find((item) => item.id === id) ?? credentialsFixture[0];
      return {
        ...source,
        id: `${source.id}-copy`,
        display_name: `${source.display_name} 2026-07-25`,
      };
    });
    vi.mocked(updateRouteCredential).mockResolvedValue({
      ...credentialsFixture[0],
      display_name: "Updated Team Account",
    });
    vi.mocked(deleteRouteCredential).mockResolvedValue(undefined);
    vi.mocked(fetchRouteModels).mockResolvedValue([
      { id: "gpt-4o", owned_by: "openai" },
      { id: "gpt-5", owned_by: "openai" },
    ]);
  });

  it("renders route credentials under the selected first-level agent tab and toggles pool membership", async () => {
    renderScreen();

    expect(await screen.findByText("筛选：")).toHaveClass("max-[599px]:hidden");
    const workspace = screen.getByTestId("account-workspace");
    const scrollRegion = screen.getByTestId("account-workspace-scroll-region");
    expect(workspace.children).toHaveLength(3);
    expect(scrollRegion.parentElement).toBe(workspace);
    expect(workspace).toHaveClass("bg-transparent");
    expect(scrollRegion).toHaveClass("bg-transparent");
    expect(screen.getByTestId("account-workspace-toolbar")).toContainElement(
      screen.getByRole("button", { name: "会话管理" }),
    );
    expect(screen.getByTestId("pool-status-strip")).toHaveClass(
      "max-[599px]:mx-0",
      "max-[599px]:max-w-none",
      "max-[599px]:flex-1",
    );
    expect(screen.getByRole("button", { name: "会话管理" }).parentElement).toHaveClass("max-[599px]:hidden");
    expect(screen.getByLabelText("刷新账号列表")).toBeInTheDocument();
    expect(screen.queryByRole("menu", { name: "刷新操作" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("打开刷新菜单"));
    expect(screen.getByRole("menu", { name: "刷新操作" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "刷新账号列表" })).toBeInTheDocument();
    expect(screen.getByText("刷新账号额度")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("打开刷新菜单"));
    expect(screen.queryByLabelText("Codex 已支持")).not.toBeInTheDocument();
    expect(await screen.findByText("Team Account")).toBeInTheDocument();
    expect(screen.getByText("API Account")).toBeInTheDocument();
    // The API credential derives a clickable domain link from its base_url.
    const openLinkButton = screen.getByLabelText("打开 api.example.com");
    expect(openLinkButton).toHaveAttribute("title", "https://api.example.com");
    const openSpy = vi.spyOn(window, "open").mockReturnValue(null);
    await userEvent.click(openLinkButton);
    expect(openSpy).toHaveBeenCalledWith(
      "https://api.example.com",
      "_blank",
      "noopener,noreferrer",
    );
    openSpy.mockRestore();
    expect(screen.getByText("算力中心")).toBeInTheDocument();
    expect(screen.getByText("请求 3")).toBeInTheDocument();
    expect(screen.getByText(/成功 2/)).toBeInTheDocument();
    expect(screen.getByText(/失败 1/)).toBeInTheDocument();
    expect(screen.getByText(/成功率 66\.7%/)).toBeInTheDocument();
    expect(screen.getByText("暂无请求")).toBeInTheDocument();
    expect(screen.queryByText("team@example.com ·", { exact: false })).not.toBeInTheDocument();
    expect(screen.getByText(/批量 Codex Batch/)).toBeInTheDocument();
    expect(screen.getAllByText("P3-")).toHaveLength(2);
    expect(screen.queryByText("并发 1")).not.toBeInTheDocument();

    expect(screen.queryByLabelText("导出选中账号")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("批量加入算力池")).not.toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("选择 Team Account"));
    expect(screen.getByText("已选 1 个账号")).toBeInTheDocument();
    expect(screen.getByLabelText("导出选中账号")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("批量加入算力池"));

    await waitFor(() =>
      expect(setRoutePoolMembers).toHaveBeenCalledWith({
        platform: "codex",
        account_ids: ["cred-official-1"],
      }),
    );
    expect(screen.getByText("已加入 1 个账号")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "算力池" })).toBeInTheDocument();
    expect(screen.queryByLabelText("批量加入算力池")).not.toBeInTheDocument();
  });

  it("uses the red status treatment for paused accounts", async () => {
    vi.mocked(listRouteCredentials).mockResolvedValue([
      {
        ...credentialsFixture[0],
        status: "paused",
      },
    ]);

    renderScreen();

    const pausedStatus = await screen.findByText("暂停");
    expect(pausedStatus).toHaveClass("bg-red-50", "text-red-800", "ring-red-200");
  });

  it("switches to the imported account pool scope and consumes the focus nonce", async () => {
    const onPoolScopeFocusConsumed = vi.fn();
    render(
      <QueryClientProvider client={createQueryClient()}>
        <AccountsScreen
          onPoolScopeFocusConsumed={onPoolScopeFocusConsumed}
          platform="codex"
          poolScopeFocus={{ platform: "codex", scope: "in_pool", nonce: 42 }}
        />
      </QueryClientProvider>,
    );

    await waitFor(() => {
      expect(onPoolScopeFocusConsumed).toHaveBeenCalledWith(42);
      expect(screen.getByRole("button", { name: "算力池" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
    });
  });

  it("batch updates selected account statuses and clears the selection", async () => {
    renderScreen();

    await screen.findByText("Team Account");
    await userEvent.click(screen.getByLabelText("选择 Team Account"));
    await userEvent.click(screen.getByLabelText("选择 API Account"));
    await userEvent.selectOptions(screen.getByLabelText("批量设置状态"), "paused");
    await userEvent.click(screen.getByLabelText("应用批量状态"));

    await waitFor(() =>
      expect(setRouteCredentialStatuses).toHaveBeenCalledWith(
        ["cred-official-1", "cred-api-1"],
        "paused",
      ),
    );
    expect(screen.queryByText("已选 2 个账号")).not.toBeInTheDocument();
  });

  it("keeps the account selection and shows an error when batch status update fails", async () => {
    vi.mocked(setRouteCredentialStatuses).mockRejectedValueOnce(
      new Error("批量设置状态失败"),
    );
    renderScreen();

    await screen.findByText("Team Account");
    await userEvent.click(screen.getByLabelText("选择 Team Account"));
    await userEvent.selectOptions(screen.getByLabelText("批量设置状态"), "paused");
    await userEvent.click(screen.getByLabelText("应用批量状态"));

    expect(await screen.findByText("批量设置状态失败")).toBeInTheDocument();
    expect(screen.getByText("已选 1 个账号")).toBeInTheDocument();
  });

  it("updates the account activity indicator from transport events", async () => {
    vi.mocked(listRouteCredentials).mockResolvedValue([
      {
        ...credentialsFixture[0],
        max_concurrency: 2,
      },
      credentialsFixture[1],
    ]);
    renderScreen();

    expect(await screen.findByText("Team Account")).toBeInTheDocument();
    expect(screen.getAllByText("P3-")).toHaveLength(2);
    expect(screen.queryByText("并发 2")).not.toBeInTheDocument();
    expect(screen.queryByTestId("credential-activity-cred-official-1")).not.toBeInTheDocument();

    const event: RouteCredentialActivityEvent = {
      platform: "codex",
      credential_id: "cred-official-1",
      active_request_count: 1,
      max_concurrency: 2,
    };
    act(() => {
      transportTestState.activityHandler?.(event);
    });
    expect(await screen.findByLabelText("正在处理请求，当前 1/2")).toBeInTheDocument();

    act(() => {
      transportTestState.activityHandler?.({
        ...event,
        active_request_count: 0,
      });
    });
    await waitFor(() =>
      expect(screen.queryByTestId("credential-activity-cred-official-1")).not.toBeInTheDocument(),
    );
  });

  it("refreshes account and pool caches after a route credential status event", async () => {
    renderScreen();

    await screen.findByText("Team Account");
    await waitFor(() => expect(transportTestState.statusHandler).not.toBeNull());
    const pageCallCount = vi.mocked(listRouteCredentialPage).mock.calls.length;
    const allCredentialsCallCount = vi.mocked(listRouteCredentials).mock.calls.length;
    const poolCallCount = vi.mocked(getRoutePool).mock.calls.length;

    act(() => {
      transportTestState.statusHandler?.({
        platform: "codex",
        credential_id: "cred-official-1",
      });
    });

    await waitFor(() => {
      expect(vi.mocked(listRouteCredentialPage).mock.calls.length).toBeGreaterThan(pageCallCount);
      expect(vi.mocked(listRouteCredentials).mock.calls.length).toBeGreaterThan(allCredentialsCallCount);
      expect(vi.mocked(getRoutePool).mock.calls.length).toBeGreaterThan(poolCallCount);
    });
  });

  it("shows account model mapping tags and the complete mapping popover", async () => {
    const account = {
      ...credentialsFixture[1],
      config_json: JSON.stringify({
        base_url: "https://api.example.com/v1",
        interface_format: "openai",
        model_mappings: [
          { from: "gpt-5.6-sol", to: "sol-upstream" },
          { from: "gpt-5.6-terra", to: "terra-upstream" },
          { from: "gpt-5.6-luna", to: "luna-upstream" },
          { from: "gpt-5.5", to: "gpt-5.5-upstream" },
        ],
      }),
    };
    vi.mocked(listRouteCredentials).mockResolvedValue([account]);

    renderScreen();

    expect(await screen.findByText("gpt-5.6-sol")).toBeInTheDocument();
    expect(screen.getByText("gpt-5.6-terra")).toBeInTheDocument();
    expect(screen.getByText("gpt-5.6-luna")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "+1" }));
    expect(screen.getByText("gpt-5.5 → gpt-5.5-upstream")).toBeInTheDocument();
  });

  it("views effective pool models from the test menu", async () => {
    poolStateByPlatform.set("codex", ["cred-official-1"]);
    vi.mocked(listRouteCredentials).mockResolvedValue([
      {
        ...credentialsFixture[0],
        config_json: JSON.stringify({
          model_mappings: [{ from: "gpt-5.6-sol", to: "sol-upstream" }],
        }),
      },
    ]);
    vi.mocked(getRouteProxyStatus).mockResolvedValue({
      running: true,
      bind_host: "127.0.0.1",
      port: 43111,
      base_url: "http://127.0.0.1:43111",
    });
    vi.mocked(fetchRouteProxyModels).mockResolvedValue([
      {
        id: "gpt-5.6-sol",
        owned_by: "ai-switch",
        supported_reasoning_levels: [
          { effort: "low", description: "Fast" },
          { effort: "medium", description: "Balanced" },
          { effort: "high", description: "Deep" },
        ],
        default_reasoning_level: "low",
      },
      {
        id: "gpt-5.6-terra",
        owned_by: "ai-switch",
        supported_reasoning_levels: [
          { effort: "low", description: "Fast" },
          { effort: "medium", description: "Balanced" },
        ],
        default_reasoning_level: "medium",
      },
    ]);

    renderScreen("codex", "in_pool");

    await userEvent.click(screen.getByLabelText("打开算力池测试菜单"));
    await userEvent.click(screen.getByRole("menuitem", { name: "查看模型列表" }));

    const dialog = await screen.findByRole("dialog", { name: "算力池模型列表" });
    expect(dialog).toBeInTheDocument();
    expect(within(dialog).getByText("gpt-5.6-sol")).toBeInTheDocument();
    expect(within(dialog).getByText("gpt-5.6-terra")).toBeInTheDocument();
    expect(within(dialog).getByText("映射的上游模型：sol-upstream")).toBeInTheDocument();
    expect(within(dialog).getByText("推理等级：low、medium、high · 默认 low")).toBeInTheDocument();
    expect(within(dialog).getByText("推理等级：low、medium · 默认 medium")).toBeInTheDocument();
    expect(fetchRouteProxyModels).toHaveBeenCalledWith(
      "http://127.0.0.1:43111",
      "sk-ai-switch-codex-key",
      "codex",
    );
  });

  it("streams the four request stages in the live log dialog", async () => {
    const historyEntry = {
      id: "entry-1",
      trace_id: null,
      platform: "codex",
      credential_id: "cred-a",
      credential_name: "Codex A",
      attempt: 0,
      path: "/v1/responses",
      target_url: "https://upstream.example/v1/chat/completions",
      requested_model: "gpt-5",
      upstream_model: "deepseek-chat",
      status: 200,
      success: true,
      error_message: null,
      duration_ms: 42,
      bridge: "CodexResponsesToChat",
      client_request: '{"input":"hello"}',
      upstream_request: '{"messages":[{"role":"user","content":"hello"}],"model":"deepseek-chat"}',
      upstream_response: '{"choices":[{"message":{"content":"hi"}}]}',
      final_response: '{"object":"response","output_text":"hi"}',
      truncated: false,
      created_at: new Date().toISOString(),
    };
    vi.mocked(subscribeRouteProxyLiveLog).mockResolvedValue([historyEntry]);

    renderScreen("codex", "in_pool");

    await userEvent.click(screen.getByLabelText("打开算力池测试菜单"));
    await userEvent.click(screen.getByRole("menuitem", { name: "实时日志" }));

    const dialog = await screen.findByRole("dialog", { name: "实时日志弹窗" });
    await waitFor(() => expect(subscribeRouteProxyLiveLog).toHaveBeenCalledWith("codex"));

    await userEvent.click(await within(dialog).findByText("gpt-5"));
    expect(within(dialog).getByText("原始请求")).toBeInTheDocument();
    expect(within(dialog).getByText("发往上游")).toBeInTheDocument();
    expect(within(dialog).getByText("上游原始返回")).toBeInTheDocument();
    expect(within(dialog).getByText("最终返回")).toBeInTheDocument();
    expect(within(dialog).getByText("协议转换")).toBeInTheDocument();

    await waitFor(() => expect(transportTestState.liveLogHandler).not.toBeNull());
    act(() => {
      transportTestState.liveLogHandler?.({
        ...historyEntry,
        id: "entry-2",
        requested_model: "claude-x",
      });
    });
    expect(await within(dialog).findByText("claude-x")).toBeInTheDocument();

    act(() => {
      transportTestState.liveLogHandler?.({
        ...historyEntry,
        id: "entry-3",
        platform: "claude",
        requested_model: "should-not-show",
      });
    });
    expect(within(dialog).queryByText("should-not-show")).not.toBeInTheDocument();
  });

  it("explains that the route proxy must run before viewing pool models", async () => {
    renderScreen("codex", "in_pool");

    await userEvent.click(screen.getByLabelText("打开算力池测试菜单"));
    await userEvent.click(screen.getByRole("menuitem", { name: "查看模型列表" }));

    expect(
      await screen.findByText("请先启动本地路由代理，再查看算力池模型列表。"),
    ).toBeInTheDocument();
    expect(fetchRouteProxyModels).not.toHaveBeenCalled();
  });

  it("hides the workspace toolbar label when the sidebar is collapsed", async () => {
    renderScreen("codex", "out_of_pool", true);

    await screen.findByTestId("pool-status-strip");
    expect(screen.getByTestId("workspace-toolbar-leading")).toHaveClass("hidden", "max-[599px]:hidden");
  });

  it("reorders accounts when dragging a handle onto another account row", async () => {
    renderScreen();

    const dragHandle = await screen.findByLabelText("拖动 Team Account");
    const dropTarget = screen.getByLabelText("放置在 API Account 前");
    const dataTransfer = {
      dropEffect: "none",
      effectAllowed: "all",
      setData: vi.fn(),
    };

    fireEvent.dragStart(dragHandle, { dataTransfer });
    await waitFor(() => expect(dragHandle).toHaveAttribute("aria-grabbed", "true"));
    expect(dataTransfer.setData).toHaveBeenCalledWith("text/plain", "cred-official-1");

    fireEvent.dragOver(dropTarget, { dataTransfer });
    expect(dropTarget).toHaveClass("border-blue-400", "bg-blue-50/70");
    fireEvent.drop(dropTarget, { dataTransfer });

    await waitFor(() =>
      expect(reorderRouteCredentials).toHaveBeenCalledWith({
        platform: "codex",
        moved_account_id: "cred-official-1",
        previous_account_id: "cred-api-1",
        next_account_id: null,
        filters: [],
        pool_scope: "out_of_pool",
        page_size: 20,
      }),
    );
  });

  it("keeps side toolbar content visible when the window is not pinned to the top", () => {
    vi.useFakeTimers();
    renderScreen();

    const workspace = screen.getByTestId("account-workspace");
    const toolbar = screen.getByTestId("account-workspace-toolbar");
    expect(workspace).toHaveStyle("grid-template-rows: 60px minmax(0, 1fr) 32px");

    act(() => {
      vi.advanceTimersByTime(2600);
    });
    expect(workspace).toHaveStyle("grid-template-rows: 60px minmax(0, 1fr) 32px");
    expect(toolbar).toContainElement(screen.getByTestId("pool-status-strip"));
    expect(screen.getByTestId("workspace-toolbar-leading")).toHaveClass("opacity-100");
    expect(screen.getByRole("button", { name: "会话管理" }).parentElement).toHaveClass("opacity-100");
  });

  it("fills the account list area and centers the empty state when no accounts exist", async () => {
    vi.mocked(listRouteCredentials).mockResolvedValue([]);

    renderScreen();

    const emptyState = await screen.findByTestId("account-empty-state");
    expect(emptyState).toHaveTextContent("空空如也");
    expect(emptyState).toHaveClass("flex-1", "items-center", "justify-center");
  });

  it("filters accounts by batch name and single-account option", async () => {
    renderScreen();

    expect(await screen.findByText("Team Account")).toBeInTheDocument();
    expect(screen.getByText("API Account")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("打开账号筛选"));
    await userEvent.click(screen.getByLabelText("筛选 Codex Batch"));
    expect(screen.getByText("Team Account")).toBeInTheDocument();
    expect(screen.queryByText("API Account")).not.toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("筛选 单账号"));
    expect(screen.getByText("Team Account")).toBeInTheDocument();
    expect(screen.getByText("API Account")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("移除筛选 Codex Batch"));
    expect(screen.queryByText("Team Account")).not.toBeInTheDocument();
    expect(screen.getByText("API Account")).toBeInTheDocument();
  });

  it("duplicates an account from the account list with a dated name", async () => {
    renderScreen();

    await userEvent.click(await screen.findByLabelText("复制 Team Account"));
    await waitFor(() => expect(copyRouteCredential).toHaveBeenCalledWith("cred-official-1"));
    expect(screen.getByLabelText("复制 Team Account")).toHaveTextContent("已复制");
  });

  it("keeps a copied account visible in the compute pool", async () => {
    let copiedCredential: RouteCredential | null = null;
    vi.mocked(listRouteCredentials).mockImplementation(async () => [
      ...credentialsFixture,
      ...(copiedCredential ? [copiedCredential] : []),
    ]);
    vi.mocked(copyRouteCredential).mockImplementationOnce(async (id: string) => {
      const source = credentialsFixture.find((item) => item.id === id) ?? credentialsFixture[0];
      copiedCredential = {
        ...source,
        id: `${source.id}-copy`,
        display_name: `${source.display_name} 2026-07-25`,
      };
      poolStateByPlatform.set("codex", ["cred-official-1", copiedCredential.id]);
      return copiedCredential;
    });
    poolStateByPlatform.set("codex", ["cred-official-1"]);
    renderScreen("codex", "in_pool");

    await userEvent.click(await screen.findByLabelText("复制 Team Account"));

    expect(await screen.findByText("Team Account 2026-07-25")).toBeInTheDocument();
    expect(setRoutePoolMembers).not.toHaveBeenCalled();
  });

  it("supports batch remove from pool and batch delete for selected accounts", async () => {
    renderScreen();

    await userEvent.click(await screen.findByLabelText("选择 Team Account"));
    await userEvent.click(screen.getByLabelText("选择 API Account"));
    expect(screen.getByText("已选 2 个账号")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("批量加入算力池"));

    await waitFor(() => {
      expect(setRoutePoolMembers).toHaveBeenCalled();
      const lastCall = vi.mocked(setRoutePoolMembers).mock.calls.at(-1)?.[0];
      expect(lastCall?.platform).toBe("codex");
      expect(new Set(lastCall?.account_ids ?? [])).toEqual(new Set(["cred-official-1", "cred-api-1"]));
    });
    await selectAccountView("算力池");
    expect(await screen.findByText("Team Account")).toBeInTheDocument();
    expect(screen.getByText("API Account")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("选择 Team Account"));
    await userEvent.click(screen.getByLabelText("批量移出算力池"));

    await waitFor(() => {
      const lastCall = vi.mocked(setRoutePoolMembers).mock.calls.at(-1)?.[0];
      expect(lastCall?.account_ids).toEqual(["cred-api-1"]);
    });
    await waitFor(() => expect(screen.queryByText("Team Account")).not.toBeInTheDocument());
    expect(screen.getByText("API Account")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("选择 API Account"));
    expect(screen.getByText("已选 1 个账号")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("批量删除账号"));

    await waitFor(() => {
      expect(deleteRouteCredential).toHaveBeenCalledTimes(1);
      expect(deleteRouteCredential).toHaveBeenCalledWith("cred-api-1");
    });
    await waitFor(() => {
      const lastCall = vi.mocked(setRoutePoolMembers).mock.calls.at(-1)?.[0];
      expect(lastCall?.account_ids).toEqual([]);
    });
  });

  it("archives selected active accounts in one batch", async () => {
    renderScreen();

    await userEvent.click(await screen.findByLabelText("选择 Team Account"));
    expect(screen.getByLabelText("批量归档账号")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("批量归档账号"));

    await waitFor(() =>
      expect(archiveRouteCredentials).toHaveBeenCalledWith(["cred-official-1"]),
    );
    expect(screen.queryByText("已选 1 个账号")).not.toBeInTheDocument();
  });

  it("restores selected archived accounts without pool actions", async () => {
    vi.mocked(listRouteCredentials).mockResolvedValue([
      { ...credentialsFixture[0], archived_at: "2026-08-05T00:00:00Z" },
      credentialsFixture[1],
    ]);
    poolStateByPlatform.set("codex", ["cred-official-1"]);
    renderScreen("codex", "archived");

    expect(await screen.findByText("Team Account")).toBeInTheDocument();
    await waitFor(() =>
      expect(listRouteCredentialPage).toHaveBeenLastCalledWith(
        expect.objectContaining({ pool_scope: "archived" }),
      ),
    );
    await userEvent.click(screen.getByLabelText("选择 Team Account"));
    expect(screen.getByLabelText("批量恢复账号")).toBeInTheDocument();
    expect(screen.queryByLabelText("批量加入算力池")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("批量移出算力池")).not.toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("批量恢复账号"));
    await waitFor(() =>
      expect(restoreRouteCredentials).toHaveBeenCalledWith(["cred-official-1"]),
    );
    expect(screen.queryByText("已选 1 个账号")).not.toBeInTheDocument();
  });

  it("shows a centered empty state for archived accounts", async () => {
    vi.mocked(listRouteCredentials).mockResolvedValue([]);
    renderScreen("codex", "archived");

    const emptyState = await screen.findByTestId("account-empty-state");
    expect(emptyState).toHaveTextContent("空空如也");
    expect(emptyState).toHaveClass("flex-1", "items-center", "justify-center");
  });

  it("switches between pooled, unpooled, and statistics segments with scoped actions", async () => {
    renderScreen("codex", "in_pool");

    expect(screen.getByRole("button", { name: "算力池" })).toHaveAttribute("aria-pressed", "true");
    await waitFor(() =>
      expect(listRouteCredentialPage).toHaveBeenLastCalledWith(
        expect.objectContaining({ platform: "codex", pool_scope: "in_pool" }),
      ),
    );
    expect(screen.queryByLabelText("批量加入算力池")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "未入池" }));
    expect(screen.getByRole("button", { name: "未入池" })).toHaveAttribute("aria-pressed", "true");
    expect(await screen.findByText("Team Account")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("选择 Team Account"));
    expect(screen.getByLabelText("批量加入算力池")).toBeInTheDocument();
    expect(screen.queryByLabelText("批量移出算力池")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "已归档" }));
    expect(screen.getByRole("button", { name: "已归档" })).toHaveAttribute("aria-pressed", "true");
    await waitFor(() =>
      expect(listRouteCredentialPage).toHaveBeenLastCalledWith(
        expect.objectContaining({ pool_scope: "archived" }),
      ),
    );

    await userEvent.click(screen.getByRole("button", { name: "统计" }));
    expect(await screen.findByText("请求统计")).toBeInTheDocument();
    expect(screen.queryByText("筛选：")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("批量加入算力池")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("批量移出算力池")).not.toBeInTheDocument();
  });

  it("shows Hermes as partial and disables unsupported account actions", async () => {
    const hermesCredentials: RouteCredential[] = credentialsFixture.map((credential) => ({
      ...credential,
      id: credential.kind === "official" ? "hermes-official" : "hermes-api",
      platform: "hermes",
      display_name: credential.kind === "official" ? "Hermes Official" : "Hermes API",
    }));
    vi.mocked(listRouteCredentials).mockResolvedValue(hermesCredentials);
    vi.mocked(getRoutePool).mockResolvedValue({
      platform: "hermes",
      account_ids: hermesCredentials.map((credential) => credential.id),
      stats: statsFixture({ member_count: 2 }),
    });
    vi.mocked(getRouteProxyStatus).mockResolvedValue({
      running: true,
      bind_host: "127.0.0.1",
      port: 43111,
      base_url: "http://127.0.0.1:43111",
    });

    renderScreen("hermes");

    expect(await screen.findByLabelText("Hermes 部分支持")).toBeInTheDocument();
    const writeConfig = screen.getByLabelText("写入路由配置文件");
    expect(writeConfig).toBeDisabled();
    expect(writeConfig).toHaveAttribute("title", expect.stringContaining("原生配置"));
    await userEvent.click(screen.getByLabelText("打开刷新菜单"));
    expect(screen.getByLabelText("刷新官方账号额度")).toBeDisabled();
    await waitFor(() => expect(refreshRouteCredentialsQuota).not.toHaveBeenCalled());

    await userEvent.click(screen.getByRole("button", { name: "新增账号" }));
    expect(screen.getByRole("button", { name: "API 账号" })).toBeEnabled();
    const officialImport = screen.getByRole("button", { name: "批量导入" });
    expect(officialImport).toBeDisabled();
    expect(officialImport).toHaveAttribute("title", expect.stringContaining("官方账号"));

    expect(screen.getByLabelText("测试 Hermes Official")).toBeDisabled();
    expect(screen.getByLabelText("测试 Hermes API")).toBeEnabled();
  });

  it("allows an error-status account to be added to and removed from the pool", async () => {
    vi.mocked(listRouteCredentials).mockResolvedValue([
      { ...credentialsFixture[0], status: "error" },
    ]);
    renderScreen();

    expect(await screen.findByText("异常")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("选择 Team Account"));
    await userEvent.click(screen.getByLabelText("批量加入算力池"));
    await waitFor(() =>
      expect(setRoutePoolMembers).toHaveBeenLastCalledWith({
        platform: "codex",
        account_ids: ["cred-official-1"],
      }),
    );

    await selectAccountView("算力池");
    expect(await screen.findByText("Team Account")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("选择 Team Account"));
    await userEvent.click(screen.getByLabelText("批量移出算力池"));
    await waitFor(() =>
      expect(setRoutePoolMembers).toHaveBeenLastCalledWith({
        platform: "codex",
        account_ids: [],
      }),
    );
    await selectAccountView("未入池");
    expect(await screen.findByText("Team Account")).toBeInTheDocument();
  });

  it("shows the stored failure response for error and cooldown accounts", async () => {
    const retryAt = new Date(Date.now() + 60_000).toISOString();
    vi.mocked(listRouteCredentials).mockResolvedValue([
      {
        ...credentialsFixture[0],
        status: "error",
        last_failure_kind: "semantic_response_failed",
        last_failure_message: "upstream rejected the request",
        last_failure_response_json: '{"error":{"message":"bad key"}}',
      },
      {
        ...credentialsFixture[1],
        cooldown_until: retryAt,
        next_retry_at: retryAt,
        last_failure_kind: "upstream_status",
        last_failure_message: "upstream returned 429",
        last_failure_response_json: '{"error":{"message":"rate limited"}}',
      },
      {
        ...credentialsFixture[0],
        id: "cred-no-response",
        display_name: "No Response Account",
      },
    ]);

    renderScreen();

    const firstRow = await screen.findByLabelText("放置在 Team Account 前");
    const secondRow = screen.getByLabelText("放置在 API Account 前");
    const thirdRow = screen.getByLabelText("放置在 No Response Account 前");
    expect(within(firstRow).getByText(/bad key/)).toBeInTheDocument();
    expect(within(secondRow).getAllByText(/rate limited/)).toHaveLength(2);
    expect(within(thirdRow).queryByText(/失败类型：/)).not.toBeInTheDocument();
  });

  it("reports route pool membership errors, rolls back, and refreshes account state", async () => {
    vi.mocked(listRouteCredentials)
      .mockResolvedValueOnce(credentialsFixture)
      .mockResolvedValue([
        { ...credentialsFixture[0], status: "error" },
        credentialsFixture[1],
      ]);
    vi.mocked(setRoutePoolMembers).mockRejectedValueOnce({
      code: "database.route_pool_commit",
      message: "Could not save route pool members",
      details: "disk I/O error",
      recoverable: true,
    });

    renderScreen();

    await userEvent.click(await screen.findByLabelText("选择 Team Account"));
    await userEvent.click(screen.getByLabelText("批量加入算力池"));

    expect(
      await screen.findByText("算力池更新失败：Could not save route pool members (disk I/O error)"),
    ).toBeInTheDocument();
    expect(
      within(screen.getByLabelText("放置在 Team Account 前")).queryByText("已入池"),
    ).not.toBeInTheDocument();
    expect(screen.getByText("已加入 0 个账号")).toBeInTheDocument();
    expect(await screen.findByText("异常")).toBeInTheDocument();
  });

  it("imports a single official CPA credential from the add dialog", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "批量导入" }));
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
    await userEvent.click(screen.getByRole("button", { name: "批量导入" }));
    vi.mocked(open).mockResolvedValue(["C:\\one.json", "C:\\two.json"]);
    await userEvent.click(screen.getByRole("button", { name: "导入 JSON 文件" }));
    await userEvent.type(screen.getByLabelText("导入批量名称"), "File Batch");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(importOfficialRouteCredentialsFromFiles).toHaveBeenCalledWith({
        platform: "codex",
        file_paths: ["C:\\one.json", "C:\\two.json"],
        batch_name: "File Batch",
      }),
    );
  });

  it("shows readable interface format labels for OpenAI and Claude options", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));

    const formatSelect = screen.getByLabelText("接口格式");
    expect(screen.queryByText("Claude Messages（兼容）")).not.toBeInTheDocument();
    expect(within(formatSelect).getAllByRole("option").map((option) => option.getAttribute("value"))).toEqual([
      "openai",
      "openai-responses",
      "anthropic",
      "gemini",
    ]);
    expect(within(formatSelect).getByRole("option", { name: "OpenAI Chat Completions" })).toHaveValue(
      "openai",
    );
    expect(within(formatSelect).getByRole("option", { name: "OpenAI Responses" })).toHaveValue(
      "openai-responses",
    );
    expect(within(formatSelect).getByRole("option", { name: "Claude Messages" })).toHaveValue("anthropic");
    expect(within(formatSelect).getByRole("option", { name: "Gemini" })).toHaveValue("gemini");
  });

  it("shows four upstream interface formats for Claude API accounts", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));

    const formatSelect = screen.getByLabelText("接口格式");
    expect(within(formatSelect).getAllByRole("option").map((option) => option.getAttribute("value"))).toEqual([
      "openai",
      "openai-responses",
      "anthropic",
      "gemini",
    ]);
    expect(screen.queryByLabelText("兼容 custom 工具（Responses 中转）")).not.toBeInTheDocument();
  });

  it("keeps Gemini API accounts Gemini-only without protocol controls", async () => {
    renderScreen("gemini");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));

    expect(screen.queryByLabelText("接口格式")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("兼容 custom 工具（Responses 中转）")).not.toBeInTheDocument();
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
    await userEvent.click(screen.getByRole("button", { name: "新增映射" }));
    await userEvent.type(screen.getByLabelText("请求模型 1"), "gpt-5");
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
        fetched_models_json: "[]",
        preview_json: null,
        batch_id: null,
        responses_custom_tool_compat: false,
        user_agent: null,
      }),
    );
  });

  it("adds a newly created API account to the selected pool by default", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    expect(screen.getByLabelText("创建后加入算力池")).toBeChecked();
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Pooled API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-pooled");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(setRoutePoolMembers).toHaveBeenCalledWith({
        platform: "codex",
        account_ids: ["cred-api-1"],
      }),
    );
    expect(screen.getByRole("button", { name: "算力池" })).toHaveAttribute("aria-pressed", "true");
  });

  it("keeps a newly created API account out of the pool when unchecked", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByLabelText("创建后加入算力池"));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Unpooled API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-unpooled");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(setRoutePoolMembers).toHaveBeenCalledWith({
        platform: "codex",
        account_ids: [],
      }),
    );
    expect(screen.getByRole("button", { name: "未入池" })).toHaveAttribute("aria-pressed", "true");
  });

  it("creates an API route credential with custom User-Agent", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "UA API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-ua");
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://api.upstream.test/v1");
    await userEvent.selectOptions(screen.getByLabelText("创建 User-Agent 预设"), "grok-workspace");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith(
        expect.objectContaining({
          display_name: "UA API",
          user_agent: "xai-grok-workspace/0.2.93",
        }),
      ),
    );
  });

  it("creates an API route credential without placeholder model mappings by default", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    expect(screen.getByText(/上游只支持有限模型.*建议.*配置模型映射/)).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Plain API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-plain");
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://api.upstream.test/v1");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith(
        expect.objectContaining({
          display_name: "Plain API",
          model_mappings_json: "[]",
          fetched_models_json: "[]",
        }),
      ),
    );
  });

  it("fetches upstream models and one-click sets a model mapping", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Fetched API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-fetch");
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://api.fetch.test/v1");
    await userEvent.click(screen.getByRole("button", { name: "获取模型列表" }));

    await waitFor(() =>
      expect(fetchRouteModels).toHaveBeenCalledWith({
        base_url: "https://api.fetch.test/v1",
        api_key: "sk-fetch",
        interface_format: "openai",
      }),
    );
    expect(await screen.findByText(/已获取 2 个模型/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "一键设置" }));
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith(
        expect.objectContaining({
          display_name: "Fetched API",
          model_mappings_json: "[{\"from\":\"gpt-5.5\",\"to\":\"gpt-5\"}]",
          fetched_models_json: JSON.stringify([
            { id: "gpt-4o", owned_by: "openai" },
            { id: "gpt-5", owned_by: "openai" },
          ]),
        }),
      ),
    );
  });

  it("applies the AgentRouter preset to the create form", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    // Move the interface format off "openai" first, so the assertion below
    // proves the preset set it rather than passively matching the codex default.
    await userEvent.selectOptions(screen.getByLabelText("接口格式"), "anthropic");
    await userEvent.selectOptions(
      screen.getByLabelText("创建 账号预设"),
      "agentrouter-primary",
    );

    expect(screen.getByLabelText("Base URL")).toHaveValue("https://agentrouter.org/v1");
    expect(screen.getByLabelText("接口格式")).toHaveValue("openai");
    expect(screen.getByLabelText("API 账号名称")).toHaveValue("AgentRouter");
    expect(screen.getByLabelText("请求模型 1")).toHaveValue("gpt-5.6-sol");
    expect(screen.getByLabelText("上游模型 1")).toHaveValue("gpt-5.6-sol");
    expect(
      screen.getByText("已套用 AgentRouter 预设，通常只需填写 API Key。"),
    ).toBeInTheDocument();
  });

  it("keeps a name the user already typed when applying a preset", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "我的账号");
    await userEvent.selectOptions(
      screen.getByLabelText("创建 账号预设"),
      "agentrouter-primary",
    );

    expect(screen.getByLabelText("API 账号名称")).toHaveValue("我的账号");
    expect(screen.getByLabelText("Base URL")).toHaveValue("https://agentrouter.org/v1");
  });

  it("replaces existing model mappings when applying a preset", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    // The create form starts with zero mapping rows, so add one to overwrite.
    await userEvent.click(screen.getByRole("button", { name: "新增映射" }));
    await userEvent.type(screen.getByLabelText("请求模型 1"), "foo");
    await userEvent.type(screen.getByLabelText("上游模型 1"), "bar");
    await userEvent.selectOptions(
      screen.getByLabelText("创建 账号预设"),
      "agentrouter-primary",
    );

    expect(screen.getByLabelText("请求模型 1")).toHaveValue("gpt-5.6-sol");
    expect(screen.getByLabelText("上游模型 1")).toHaveValue("gpt-5.6-sol");
    expect(screen.queryByLabelText("请求模型 2")).not.toBeInTheDocument();
  });

  it("falls back to the custom option after the base url changes", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.selectOptions(
      screen.getByLabelText("创建 账号预设"),
      "agentrouter-primary",
    );
    expect(screen.getByLabelText("创建 账号预设")).toHaveValue("agentrouter-primary");

    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://other.example.com/v1");

    expect(screen.getByLabelText("创建 账号预设")).toHaveValue("");
    expect(
      screen.queryByText("已套用 AgentRouter 预设，通常只需填写 API Key。"),
    ).not.toBeInTheDocument();
  });

  it("creates an AgentRouter account from a preset and an api key alone", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.selectOptions(
      screen.getByLabelText("创建 账号预设"),
      "agentrouter-primary",
    );
    await userEvent.type(screen.getByLabelText("API Key"), "sk-agentrouter");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith(
        expect.objectContaining({
          platform: "codex",
          display_name: "AgentRouter",
          api_key: "sk-agentrouter",
          base_url: "https://agentrouter.org/v1",
          interface_format: "openai",
          model_mappings_json: "[{\"from\":\"gpt-5.6-sol\",\"to\":\"gpt-5.6-sol\"}]",
        }),
      ),
    );
  });

  it("hides the preset select on platforms without presets", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));

    expect(screen.queryByLabelText("创建 账号预设")).not.toBeInTheDocument();
  });

  it("rejects the placeholder upstream model mapping before saving", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Bad API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-bad");
    await userEvent.click(screen.getByRole("button", { name: "新增映射" }));
    await userEvent.type(screen.getByLabelText("请求模型 1"), "gpt-5");
    await userEvent.type(screen.getByLabelText("上游模型 1"), "upstream-model");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    expect((await screen.findAllByText(/upstream-model 只是示例占位/)).length).toBeGreaterThan(0);
    expect(createApiRouteCredential).not.toHaveBeenCalled();
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

  it("shows Claude role templates without saving empty mappings", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Claude API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-claude");
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://api.anthropic.test");
    await userEvent.selectOptions(screen.getByLabelText("接口格式"), "anthropic");
    expect(screen.getByLabelText("显示名称 1")).toHaveValue("Sonnet");
    expect(screen.getByLabelText("显示名称 2")).toHaveValue("Opus");
    expect(screen.getByLabelText("显示名称 3")).toHaveValue("Fable");
    expect(screen.getByLabelText("显示名称 4")).toHaveValue("Haiku");
    await userEvent.type(screen.getByLabelText("上游模型 1"), "provider-sonnet");
    expect(screen.getByLabelText("声明支持 1M 1")).not.toBeChecked();
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith({
        platform: "claude",
        display_name: "Claude API",
        api_key: "sk-claude",
        base_url: "https://api.anthropic.test",
        interface_format: "anthropic",
        api_key_field: "ANTHROPIC_AUTH_TOKEN",
        model_mappings_json: "[{\"from\":\"claude-sonnet-5\",\"to\":\"provider-sonnet\",\"label\":\"Sonnet\"}]",
        fetched_models_json: "[]",
        preview_json: null,
        batch_id: null,
        responses_custom_tool_compat: false,
        user_agent: null,
      }),
    );
  });

  it("shows the Claude subagent and fallback rows after the menu roles", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));

    // The four /model-menu roles keep positions 1-4.
    expect(screen.getByLabelText("显示名称 1")).toHaveValue("Sonnet");
    expect(screen.getByLabelText("显示名称 4")).toHaveValue("Haiku");

    // The two new roles have no editable display name.
    expect(screen.getByLabelText("显示名称 5")).toBeDisabled();
    expect(screen.getByLabelText("显示名称 6")).toBeDisabled();
    expect(screen.getByLabelText("显示名称 5")).toHaveAttribute(
      "placeholder",
      "不显示在 /model 菜单",
    );
    expect(screen.getByLabelText("请求模型 5")).toHaveValue("claude-subagent");
    expect(screen.getByLabelText("请求模型 6")).toHaveValue("*");

    // Neither declares 1M support, but the menu roles still do.
    expect(screen.getByLabelText("声明支持 1M 4")).toBeInTheDocument();
    expect(screen.queryByLabelText("声明支持 1M 5")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("声明支持 1M 6")).not.toBeInTheDocument();
  });

  it("saves the Claude subagent and fallback mappings", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Claude API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-claude");
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://api.anthropic.test");
    await userEvent.selectOptions(screen.getByLabelText("接口格式"), "anthropic");
    await userEvent.type(screen.getByLabelText("上游模型 5"), "provider-haiku");
    await userEvent.type(screen.getByLabelText("上游模型 6"), "provider-sonnet");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith(
        expect.objectContaining({
          // Appended after Haiku, and with no label key on either row.
          model_mappings_json:
            "[{\"from\":\"claude-subagent\",\"to\":\"provider-haiku\"},{\"from\":\"*\",\"to\":\"provider-sonnet\"}]",
        }),
      ),
    );
  });

  it("counts only configured mappings in the editor hint", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.type(screen.getByLabelText("上游模型 1"), "provider-sonnet");

    // Touching one row pushes every synthesized row into state; the hint must
    // still report the one row that will actually be persisted.
    expect(await screen.findByText(/共 1 条/)).toBeInTheDocument();
  });

  it("saves the selected Claude API key field and uses it when fetching models", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Claude x-api-key API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-claude");
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://api.anthropic.test");

    expect(screen.getByLabelText("Claude 鉴权字段")).toHaveValue("ANTHROPIC_AUTH_TOKEN");
    await userEvent.selectOptions(screen.getByLabelText("Claude 鉴权字段"), "ANTHROPIC_API_KEY");
    await userEvent.click(screen.getByRole("button", { name: "获取模型列表" }));

    await waitFor(() =>
      expect(fetchRouteModels).toHaveBeenCalledWith({
        base_url: "https://api.anthropic.test",
        api_key: "sk-claude",
        interface_format: "anthropic",
        api_key_field: "ANTHROPIC_API_KEY",
      }),
    );

    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith(
        expect.objectContaining({
          display_name: "Claude x-api-key API",
          api_key_field: "ANTHROPIC_API_KEY",
        }),
      ),
    );
  });

  it("saves Claude 1M support only when the role mapping is checked", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Claude 1M API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-claude");
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://api.anthropic.test");
    await userEvent.selectOptions(screen.getByLabelText("接口格式"), "anthropic");
    await userEvent.type(screen.getByLabelText("上游模型 1"), "provider-sonnet-1m");
    await userEvent.click(screen.getByLabelText("声明支持 1M 1"));
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith(
        expect.objectContaining({
          display_name: "Claude 1M API",
          model_mappings_json:
            "[{\"from\":\"claude-sonnet-5\",\"to\":\"provider-sonnet-1m\",\"label\":\"Sonnet\",\"supports_1m\":true}]",
        }),
      ),
    );
  });

  it("does not persist Claude role templates when the upstream models are empty", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Claude Empty API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-claude");
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://api.anthropic.test");
    await userEvent.selectOptions(screen.getByLabelText("接口格式"), "anthropic");
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith(
        expect.objectContaining({
          display_name: "Claude Empty API",
          api_key_field: "ANTHROPIC_AUTH_TOKEN",
          model_mappings_json: "[]",
        }),
      ),
    );
  });

  it("creates API account with responses custom tool compat enabled when checked", async () => {
    renderScreen();
    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Compat API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-compat");
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://api.upstream.test/v1");
    await userEvent.selectOptions(screen.getByLabelText("接口格式"), "openai-responses");
    await userEvent.click(screen.getByLabelText("兼容 custom 工具（Responses 中转）"));
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith(
        expect.objectContaining({
          display_name: "Compat API",
          responses_custom_tool_compat: true,
        }),
      ),
    );
  });

  it("loads and saves responses custom tool compat from API account config", async () => {
    const api = {
      ...credentialsFixture[1],
      config_json: JSON.stringify({
        base_url: "https://api.example.com/v1",
        interface_format: "openai-responses",
        model_mappings: [],
        responses_custom_tool_compat: true,
      }),
    };
    vi.mocked(listRouteCredentials).mockResolvedValue([api]);
    vi.mocked(updateRouteCredential).mockResolvedValue({
      ...api,
      display_name: "API Account Updated",
    });

    renderScreen();
    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    const checkbox = await screen.findByLabelText("兼容 custom 工具（Responses 中转）");
    expect(checkbox).toBeChecked();
    await userEvent.click(checkbox);
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const payload = vi.mocked(updateRouteCredential).mock.calls[0][1];
    const config = JSON.parse(payload.config_json);
    expect(config.responses_custom_tool_compat).toBe(false);
  });

  it("hydrates and saves User-Agent when editing an API account", async () => {
    vi.mocked(listRouteCredentials).mockResolvedValue([
      {
        ...credentialsFixture[1],
        config_json: JSON.stringify({
          base_url: "https://api.example.com/v1",
          interface_format: "openai",
          model_mappings: [],
          headers: { "User-Agent": "OldBot/1.0" },
        }),
      },
    ]);
    vi.mocked(updateRouteCredential).mockResolvedValue(credentialsFixture[1]);

    renderScreen();
    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));

    expect(screen.getByLabelText("编辑 User-Agent")).toHaveValue("OldBot/1.0");
    await userEvent.clear(screen.getByLabelText("编辑 User-Agent"));
    await userEvent.type(screen.getByLabelText("编辑 User-Agent"), "NewBot/2.0");
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const payload = vi.mocked(updateRouteCredential).mock.calls[0][1];
    const config = JSON.parse(payload.config_json);
    expect(config.headers["User-Agent"]).toBe("NewBot/2.0");
  });

  it("hydrates and saves the fetched model list without refetching", async () => {
    const api = {
      ...credentialsFixture[1],
      config_json: JSON.stringify({
        base_url: "https://api.example.com/v1",
        interface_format: "openai",
        model_mappings: [],
        fetched_models: [
          { id: "gpt-4o", owned_by: "openai" },
          { id: "gpt-5", owned_by: "openai" },
        ],
      }),
    };
    vi.mocked(listRouteCredentials).mockResolvedValue([api]);
    vi.mocked(updateRouteCredential).mockResolvedValue(api);

    renderScreen();
    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));

    expect(screen.getByText(/已获取 2 个模型/)).toBeInTheDocument();
    expect(fetchRouteModels).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const config = JSON.parse(vi.mocked(updateRouteCredential).mock.calls[0][1].config_json);
    expect(config.fetched_models).toEqual([
      { id: "gpt-4o", owned_by: "openai" },
      { id: "gpt-5", owned_by: "openai" },
    ]);
  });

  it("keeps cached models when a manual refresh fails", async () => {
    const api = {
      ...credentialsFixture[1],
      config_json: JSON.stringify({
        base_url: "https://api.example.com/v1",
        interface_format: "openai",
        model_mappings: [],
        fetched_models: [{ id: "gpt-5", owned_by: "openai" }],
      }),
    };
    vi.mocked(listRouteCredentials).mockResolvedValue([api]);
    vi.mocked(fetchRouteModels).mockRejectedValueOnce(new Error("获取模型列表失败。"));

    renderScreen();
    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    await userEvent.click(screen.getByRole("button", { name: "获取模型列表" }));

    expect(await screen.findByText("获取模型列表失败。")).toBeInTheDocument();
    expect(screen.getByText(/已获取 1 个模型/)).toBeInTheDocument();
  });

  it("clears cached models when the upstream connection changes", async () => {
    const api = {
      ...credentialsFixture[1],
      config_json: JSON.stringify({
        base_url: "https://api.example.com/v1",
        interface_format: "openai",
        model_mappings: [],
        fetched_models: [{ id: "gpt-5", owned_by: "openai" }],
      }),
    };
    vi.mocked(listRouteCredentials).mockResolvedValue([api]);
    vi.mocked(updateRouteCredential).mockResolvedValue(api);

    renderScreen();
    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    await userEvent.clear(screen.getByLabelText("编辑 Base URL"));
    await userEvent.type(screen.getByLabelText("编辑 Base URL"), "https://new.example.com/v1");

    expect(screen.queryByText(/已获取 1 个模型/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));
    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const config = JSON.parse(vi.mocked(updateRouteCredential).mock.calls[0][1].config_json);
    expect(config.fetched_models).toEqual([]);
  });

  it("edits API credential model mappings through the visual editor", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    expect(
      screen.getByText(/当前账号仅按已配置的本地模型别名参与匹配/),
    ).toBeInTheDocument();
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

  it("edits API credential base URL through structured fields without showing email", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));

    expect(screen.queryByLabelText("编辑邮箱")).not.toBeInTheDocument();
    expect(screen.getByLabelText("编辑 API Key")).toHaveValue("sk-test");
    expect(screen.getByLabelText("编辑 Base URL")).toHaveValue("https://api.example.com/v1");

    await userEvent.clear(screen.getByLabelText("编辑 Base URL"));
    await userEvent.type(screen.getByLabelText("编辑 Base URL"), "https://api.changed.test/v1");
    await userEvent.selectOptions(screen.getByLabelText("编辑接口格式"), "openai-responses");
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const updateInput = vi.mocked(updateRouteCredential).mock.calls[0][1];
    expect(updateInput.email).toBeNull();
    expect(JSON.parse(updateInput.secret_payload_json).api_key).toBe("sk-test");
    expect(JSON.parse(updateInput.config_json)).toMatchObject({
      base_url: "https://api.changed.test/v1",
      interface_format: "openai-responses",
      model_mappings: [{ from: "gpt-5", to: "old-upstream" }],
    });
    expect(JSON.parse(updateInput.preview_json).config_toml).toContain("https://api.changed.test/v1");
  });

  it("edits the Claude API key field through structured API fields", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    await userEvent.selectOptions(screen.getByLabelText("编辑接口格式"), "anthropic");
    await userEvent.selectOptions(screen.getByLabelText("编辑 Claude 鉴权字段"), "ANTHROPIC_AUTH_TOKEN");
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const updateInput = vi.mocked(updateRouteCredential).mock.calls[0][1];
    expect(JSON.parse(updateInput.config_json)).toMatchObject({
      interface_format: "anthropic",
      api_key_field: "ANTHROPIC_AUTH_TOKEN",
    });
  });

  it("syncs the API edit JSON preview when decoding a Base64 API key", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    await userEvent.clear(screen.getByLabelText("编辑 API Key"));
    await userEvent.type(screen.getByLabelText("编辑 API Key"), "c2stZWRpdA==");
    await userEvent.click(screen.getByLabelText("编辑 Base64 解码 API Key"));

    expect(screen.getByLabelText("编辑 API Key")).toHaveValue("sk-edit");
    expect((screen.getByLabelText("编辑 Preview JSON") as HTMLTextAreaElement).value).toContain("sk-edit");

    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const updateInput = vi.mocked(updateRouteCredential).mock.calls[0][1];
    expect(JSON.parse(updateInput.secret_payload_json).api_key).toBe("sk-edit");
    expect(JSON.parse(updateInput.preview_json).auth_json.api_key).toBe("sk-edit");
  });

  it("shows default failure policy values and explains handled failures", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 Team Account" }));

    expect(screen.getByLabelText("额外重试次数")).toHaveValue(2);
    expect(screen.getByLabelText("重试间隔（毫秒）")).toHaveValue(200);
    expect(screen.getByLabelText("异常触发次数")).toHaveValue(10);
    expect(screen.getByText(/网络连接失败、请求超时、响应读取失败/)).toBeInTheDocument();
    expect(screen.getByText(/相同且连续达到设定次数后/)).toBeInTheDocument();
    expect(screen.getByText(/HTTP 401 \/ 403/)).toBeInTheDocument();
  });

  it("hydrates custom failure policy and preserves other account config when saving", async () => {
    const api = {
      ...credentialsFixture[1],
      config_json: JSON.stringify({
        base_url: "https://api.example.com/v1",
        interface_format: "openai",
        model_mappings: [{ from: "gpt-5", to: "existing-upstream" }],
        headers: { "User-Agent": "ExistingBot/1.0" },
        custom_option: "keep-me",
        failure_policy: {
          retry_count: 4,
          retry_interval_ms: 750,
          semantic_error_threshold: 18,
        },
      }),
    };
    vi.mocked(listRouteCredentials).mockResolvedValue([api]);
    vi.mocked(updateRouteCredential).mockResolvedValue(api);

    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    expect(screen.getByLabelText("额外重试次数")).toHaveValue(4);
    expect(screen.getByLabelText("重试间隔（毫秒）")).toHaveValue(750);
    expect(screen.getByLabelText("异常触发次数")).toHaveValue(18);

    fireEvent.change(screen.getByLabelText("额外重试次数"), { target: { value: "6" } });
    fireEvent.change(screen.getByLabelText("重试间隔（毫秒）"), { target: { value: "900" } });
    fireEvent.change(screen.getByLabelText("异常触发次数"), { target: { value: "25" } });
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const payload = vi.mocked(updateRouteCredential).mock.calls[0][1];
    expect(JSON.parse(payload.config_json)).toMatchObject({
      base_url: "https://api.example.com/v1",
      interface_format: "openai",
      model_mappings: [{ from: "gpt-5", to: "existing-upstream" }],
      headers: { "User-Agent": "ExistingBot/1.0" },
      custom_option: "keep-me",
      failure_policy: {
        retry_count: 6,
        retry_interval_ms: 900,
        semantic_error_threshold: 25,
      },
    });
  });

  it("rejects an invalid retry count before saving the account", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 Team Account" }));
    fireEvent.change(screen.getByLabelText("额外重试次数"), { target: { value: "11" } });
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    expect(
      await screen.findAllByText("额外重试次数必须是 0-10 的整数"),
    ).toHaveLength(2);
    expect(updateRouteCredential).not.toHaveBeenCalled();
  });

  it("recognizes an API key from a clipboard image while editing an API credential", async () => {
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
    vi.mocked(recognizeApiKeysFromImageBlob).mockResolvedValue("sk-edit-ocr-123456");
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    await userEvent.click(screen.getByRole("button", { name: "编辑 OCR识别 API Key" }));

    await waitFor(() => expect(recognizeApiKeysFromImageBlob).toHaveBeenCalledWith(imageBlob));
    expect(screen.getByLabelText("编辑 API Key")).toHaveValue("sk-edit-ocr-123456");
    expect((screen.getByLabelText("编辑 Preview JSON") as HTMLTextAreaElement).value).toContain("sk-edit-ocr-123456");
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
        route_priority: 3,
        max_concurrency: 1,
        secret_payload_json: "{\n  \"access_token\": \"at\",\n  \"refresh_token\": \"rt\"\n}",
        config_json: expect.stringContaining('"failure_policy"'),
        preview_json: "{\n  \"auth_json\": {}\n}",
      }),
    );
  });

  it("saves per-account scheduled recovery settings", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 Team Account" }));
    await userEvent.selectOptions(screen.getByLabelText("自动恢复模式"), "scheduled");
    fireEvent.change(screen.getByLabelText("恢复时间 1"), { target: { value: "03:30" } });
    await userEvent.click(screen.getByLabelText("添加恢复时间"));
    fireEvent.change(screen.getByLabelText("恢复时间 2"), { target: { value: "15:45" } });
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() =>
      expect(setRouteCredentialRecovery).toHaveBeenCalledWith("cred-official-1", {
        mode: "scheduled",
        times: ["03:30", "15:45"],
        probe_interval_minutes: null,
      }),
    );
  });

  it("saves official account User-Agent into config headers", async () => {
    vi.mocked(updateRouteCredential).mockResolvedValue(credentialsFixture[0]);
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 Team Account" }));
    await userEvent.selectOptions(screen.getByLabelText("编辑 User-Agent 预设"), "browser");
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const payload = vi.mocked(updateRouteCredential).mock.calls[0][1];
    const config = JSON.parse(payload.config_json);
    expect(config.headers["User-Agent"]).toContain("Mozilla/5.0");
  });

  it("renders filtered route request statistics, expands request details, and paginates request rows", async () => {
    vi.setSystemTime(new Date("2026-07-17T08:00:00Z"));
    const expectedMonthStart = new Date();
    expectedMonthStart.setHours(0, 0, 0, 0);
    expectedMonthStart.setDate(1);

    vi.mocked(getRoutePool).mockImplementation(async (platform, since, requestPage = 1, requestPageSize = 20) => {
      const requests: RoutePoolUsageLog[] =
        requestPage === 2
          ? [
              {
                id: "request-page-2",
                account_id: "cred-api-1",
                account_name: "API Account",
                source_label: "route_proxy",
                metric_type: "request",
                amount: 1,
                unit: "count",
                input_tokens: 50,
                output_tokens: 10,
                cache_tokens: 5,
                price_usd_micros: null,
                price_cny_micros: null,
                price_currency: null,
                metadata_json: JSON.stringify({
                  requested_model: "gpt-5",
                  upstream_model: "gpt-5",
                  path: "/chat/completions",
                  status: 401,
                  success: false,
                  error_message: "upstream returned 401",
                  response_body: "{\"error\":{\"message\":\"expired\"}}",
                }),
                created_at: "2026-07-17T08:02:00Z",
              },
            ]
          : [
              {
                id: "request-success",
                account_id: "cred-official-1",
                account_name: "Team Account",
                source_label: "route_proxy",
                metric_type: "request",
                amount: 1,
                unit: "count",
                input_tokens: 120,
                output_tokens: 30,
                cache_tokens: 80,
                price_usd_micros: null,
                price_cny_micros: 7_100_000,
                price_currency: "cny",
                metadata_json: JSON.stringify({
                  source: "ui_model_connectivity_test",
                  request_kind: "model_connectivity",
                  platform: "codex",
                  route_credential_id: "cred-official-1",
                  route_credential_name: "Team Account",
                  interface_format: "openai",
                  requested_model: "gpt-5.6-sol",
                  upstream_model: "sol-upstream",
                  path: "/chat/completions",
                  status: 200,
                  success: true,
                  duration_ms: 321,
                  request_body_json:
                    "{\"model\":\"gpt-5\",\"messages\":[{\"role\":\"user\",\"content\":\"Reply with exactly: ai-switch-ok\"}]}",
                  response_body: "{\"choices\":[{\"message\":{\"content\":\"ai-switch-ok\"}}]}",
                  response_text: "ai-switch-ok",
                  error_message: null,
                }),
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
            ];

      return {
        platform,
        account_ids: ["cred-official-1"],
        stats: statsFixture({
          member_count: 1,
          request_count: 99,
          token_count: 150,
          input_token_count: 120,
          output_token_count: 30,
          cache_token_count: 80,
          cost_micros: 1_000_000,
          request_row_count: 42,
          request_page: requestPage ?? 1,
          request_page_size: requestPageSize ?? 20,
          requests,
        }),
      };
    });

    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "统计" }));

    expect(await screen.findByText("请求统计")).toBeInTheDocument();
    expect(screen.getByText("统计当前 Codex 的历史路由请求")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "当日" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "本周" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "本月" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "累计" })).toBeInTheDocument();
    expect(await screen.findByText("42 条请求")).toBeInTheDocument();
    expect(await screen.findByText("请求 1/3")).toBeInTheDocument();
    const statusBar = screen.getByTestId("account-workspace-status-bar");
    expect(statusBar).toContainElement(screen.getByText("请求 1/3"));
    expect(statusBar).toContainElement(screen.getByRole("button", { name: "下一页请求" }));
    expect(screen.getByText(/\/chat\/completions/)).toBeInTheDocument();
    expect(screen.getByText("200")).toBeInTheDocument();
    expect(screen.getAllByText("route_proxy")).toHaveLength(2);
    expect(screen.getByLabelText("查看请求 request-success 详情")).toBeInTheDocument();

    const invalidMetadataRow = screen.getByText("Broken Metadata Account").closest("[data-route-request-row]");
    expect(invalidMetadataRow).not.toBeNull();
    expect(within(invalidMetadataRow as HTMLElement).getAllByText("-")).toHaveLength(5);

    expect(screen.getByText("输入 Token")).toBeInTheDocument();
    expect(screen.getByText("输出 Token")).toBeInTheDocument();
    expect(screen.getByText("缓存 Token")).toBeInTheDocument();
    expect(screen.getByText("总费用（USD）")).toBeInTheDocument();
    expect(screen.getByText("¥7.100000")).toBeInTheDocument();
    expect(screen.getByText("$1.00")).toBeInTheDocument();
    expect(screen.getAllByText("120").length).toBeGreaterThan(0);
    expect(screen.getAllByText("30").length).toBeGreaterThan(0);
    expect(screen.getAllByText("80").length).toBeGreaterThan(0);
    expect(screen.getByText("gpt-5.6-sol->sol-upstream")).toBeInTheDocument();
    expect(
      screen.getByTitle("输入 Token：120；输出 Token：30；缓存 Token：80"),
    ).toBeInTheDocument();
    const successRow = screen.getByText("Team Account").closest("[data-route-request-row]");
    expect(successRow).not.toBeNull();
    expect((successRow as HTMLElement).firstElementChild).toHaveClass(
      "grid-cols-2",
      "sm:grid-cols-4",
    );

    await userEvent.click(screen.getByLabelText("查看请求 request-success 详情"));

    const successDetail = await screen.findByLabelText("请求 request-success 详情");
    expect(within(successDetail).getByText("请求详情")).toBeInTheDocument();
    expect(within(successDetail).getByText("request-success")).toBeInTheDocument();
    expect(within(successDetail).getByText("cred-official-1")).toBeInTheDocument();
    expect(within(successDetail).getByText("Team Account")).toBeInTheDocument();
    expect(within(successDetail).getByText("1 count")).toBeInTheDocument();
    expect(within(successDetail).getByText("120")).toBeInTheDocument();
    expect(within(successDetail).getByText("30")).toBeInTheDocument();
    expect(within(successDetail).getByText("80")).toBeInTheDocument();
    expect(within(successDetail).getByText("¥7.100000")).toBeInTheDocument();
    expect(within(successDetail).getByText(/"path": "\/chat\/completions"/)).toBeInTheDocument();
    expect(within(successDetail).getByText(/"status": 200/)).toBeInTheDocument();
    expect(within(successDetail).getByText(/model_connectivity/)).toBeInTheDocument();
    expect(within(successDetail).getByText(/request_body_json/)).toBeInTheDocument();
    expect(within(successDetail).getByText(/response_body/)).toBeInTheDocument();
    expect(within(successDetail).getAllByText(/ai-switch-ok/).length).toBeGreaterThan(0);

    await userEvent.click(screen.getByLabelText("查看请求 request-invalid-metadata 详情"));

    const invalidDetail = await screen.findByLabelText("请求 request-invalid-metadata 详情");
    expect(within(invalidDetail).getByText("metadata_json 无法解析，显示原始内容。")).toBeInTheDocument();
    expect(within(invalidDetail).getByText("{bad json")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("下一页请求"));

    await waitFor(() =>
      expect(getRoutePool).toHaveBeenLastCalledWith(
        "codex",
        expect.any(String),
        2,
        20,
      ),
    );
    expect(await screen.findByText("请求 2/3")).toBeInTheDocument();
    const pageTwoDetailButton = await screen.findByLabelText("查看请求 request-page-2 详情");
    expect(pageTwoDetailButton).toBeInTheDocument();
    await userEvent.click(pageTwoDetailButton);
    expect(await screen.findByText("request-page-2")).toBeInTheDocument();
    const failedDetail = await screen.findByLabelText("请求 request-page-2 详情");
    expect(within(failedDetail).getByText("上游原始响应")).toBeInTheDocument();
    expect(within(failedDetail).getByText(/"message": "expired"/)).toBeInTheDocument();
    expect(screen.queryByText("request-success")).not.toBeInTheDocument();

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

    await screen.findByText("筛选：");
    expect(getRoutePool).toHaveBeenCalledTimes(1);

    vi.useFakeTimers();

    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "统计" }));
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
      fireEvent.click(screen.getByRole("button", { name: "未入池" }));
    });

    await act(async () => {
      vi.advanceTimersByTime(5000);
      await Promise.resolve();
    });

    expect(getRoutePool).toHaveBeenCalledTimes(3);
  });

  it("tests the credential pool route through the internal model connectivity check", async () => {
    vi.mocked(routePoolTestModel).mockResolvedValue(
      modelTestOutcomeFixture({
        via_route_proxy: true,
        route_proxy_entry_url: "http://127.0.0.1:43111/v1/chat/completions",
        route_proxy_entry_path: "/v1/chat/completions",
        route_proxy_trace_id: "trace-1234",
      }),
    );
    poolStateByPlatform.set("codex", ["cred-official-1"]);
    renderScreen("codex", "in_pool");

    expect(await screen.findByText("本地代理：未启动")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByLabelText("真实生成测试算力池路由")).toBeEnabled());
    await userEvent.click(screen.getByLabelText("真实生成测试算力池路由"));
    expect(await screen.findByLabelText("真实生成测试弹窗")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("开始真实生成测试"));

    await waitFor(() =>
      expect(routePoolTestModel).toHaveBeenCalledWith({
        platform: "codex",
        model: null,
        interface_format: "openai-responses",
      }),
    );
    expect(startRouteProxy).not.toHaveBeenCalled();
    expect(writeRouteProxyConfigs).not.toHaveBeenCalled();
    expect(await screen.findByText("真实生成测试：通过")).toBeInTheDocument();
    expect(screen.getByText("模型输出")).toBeInTheDocument();
    expect(screen.getByText("ai-switch-ok")).toBeInTheDocument();
    expect(screen.getByText("HTTP 200 · 321 ms")).toBeInTheDocument();
    expect(screen.getAllByText(/https:\/\/api\.example\.com\/v1\/chat\/completions/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/Reply with exactly: ai-switch-ok/)).toBeInTheDocument();
    expect(screen.getByText(/choices/)).toBeInTheDocument();
    expect(screen.getByText("最近路由到：Team Account")).toBeInTheDocument();
    expect(screen.getByLabelText("算力池请求链路")).toBeInTheDocument();
    expect(screen.getByText("算力池入口")).toBeInTheDocument();
    expect(screen.getByText("http://127.0.0.1:43111/v1/chat/completions")).toBeInTheDocument();
    expect(screen.getByText("命中账号")).toBeInTheDocument();
    expect(screen.getByText(/Team Account · cred-official-1/)).toBeInTheDocument();
    expect(screen.getByText("上游接口")).toBeInTheDocument();
    expect(screen.queryByText("请求统计")).not.toBeInTheDocument();

    const proxyStatus = screen.getByText("本地代理：未启动");
    const recentRouteStatus = screen.getByText("最近路由到：Team Account");
    expect(proxyStatus.className).not.toContain("bg-white");
    expect(recentRouteStatus.className).not.toContain("bg-white");
  });

  it("keeps pool testing available outside the pool list and copies a curl command", async () => {
    poolStateByPlatform.set("codex", ["cred-official-1"]);
    vi.mocked(getRouteProxyStatus).mockResolvedValue({
      running: true,
      bind_host: "127.0.0.1",
      port: 43111,
      base_url: "https://127.0.0.1:43111",
    });
    renderScreen("codex", "out_of_pool");

    const sendTest = await screen.findByLabelText("真实生成测试算力池路由");
    await waitFor(() => expect(sendTest).toBeEnabled());

    await userEvent.click(screen.getByLabelText("打开算力池测试菜单"));
    expect(screen.getByRole("menu", { name: "算力池测试菜单" })).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("复制 curl 执行语句"));

    await waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
        expect.stringContaining("curl.exe 'https://127.0.0.1:43111/responses'"),
      ),
    );
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
      expect.stringContaining("--ssl-no-revoke"),
    );
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
      expect.stringContaining("--data-raw '{\"model\""),
    );
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
      expect.stringContaining('x-ai-switch-platform: codex'),
    );
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
      expect.stringContaining('Authorization: Bearer sk-ai-switch-codex-key'),
    );

    await userEvent.click(screen.getByLabelText("打开算力池测试菜单"));
    await userEvent.click(screen.getByLabelText("复制 CMD curl 执行语句"));
    await waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith(
        expect.stringContaining('--data-raw "{""model""'),
      ),
    );
  });

  it("copies the running route proxy Base URL and platform sk", async () => {
    vi.mocked(getRouteProxyStatus).mockResolvedValue({
      running: true,
      bind_host: "127.0.0.1",
      port: 43111,
      base_url: "https://127.0.0.1:43111",
    });
    poolStateByPlatform.set("codex", ["cred-official-1"]);
    renderScreen("codex", "out_of_pool");

    await waitFor(() => expect(screen.getByLabelText("真实生成测试算力池路由")).toBeEnabled());
    await userEvent.click(screen.getByLabelText("打开算力池测试菜单"));
    await userEvent.click(screen.getByLabelText("复制 Base URL"));
    expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith("https://127.0.0.1:43111");

    await userEvent.click(screen.getByLabelText("打开算力池测试菜单"));
    await userEvent.click(screen.getByLabelText("复制 sk"));
    expect(getRouteProxyKey).toHaveBeenCalledWith("codex");
    expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith("sk-ai-switch-codex-key");
  });

  it("tests the credential pool route with a user-specified model", async () => {
    poolStateByPlatform.set("codex", ["cred-official-1"]);
    renderScreen("codex", "in_pool");

    await userEvent.click(await screen.findByLabelText("真实生成测试算力池路由"));
    await userEvent.type(await screen.findByLabelText("弹窗测试模型"), "gpt-4o");
    await userEvent.click(screen.getByLabelText("开始真实生成测试"));

    await waitFor(() =>
      expect(routePoolTestModel).toHaveBeenCalledWith({
        platform: "codex",
        model: "gpt-4o",
        interface_format: "openai-responses",
      }),
    );
  });

  it("tests a single credential from the account row action", async () => {
    renderScreen();

    expect(await screen.findByLabelText("测试 API Account")).toBeEnabled();
    await userEvent.click(screen.getByLabelText("测试 API Account"));
    expect(await screen.findByLabelText("真实生成测试弹窗")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("开始真实生成测试"));

    await waitFor(() =>
      expect(routePoolTestModel).toHaveBeenCalledWith({
        platform: "codex",
        account_id: "cred-api-1",
        model: null,
        interface_format: "openai-responses",
      }),
    );
    expect(await screen.findByText("真实生成测试：通过")).toBeInTheDocument();
  });

  it("defaults Codex model tests to responses", async () => {
    renderScreen();

    await userEvent.click(await screen.findByLabelText("测试 API Account"));

    expect(screen.getByLabelText("测试接口 /responses")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByLabelText("测试接口 /chat/completions")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("persists Chat Completions globally for pool and account tests", async () => {
    const first = renderScreen();

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    await userEvent.click(screen.getByLabelText("测试接口 /chat/completions"));
    expect(window.localStorage.getItem(CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY)).toBe(
      "/chat/completions",
    );
    await userEvent.click(screen.getByLabelText("开始真实生成测试"));
    await waitFor(() =>
      expect(routePoolTestModel).toHaveBeenLastCalledWith({
        platform: "codex",
        account_id: "cred-api-1",
        model: null,
        interface_format: "openai",
      }),
    );
    first.unmount();

    poolStateByPlatform.set("codex", ["cred-official-1"]);
    renderScreen("codex", "in_pool");
    await waitFor(() => expect(screen.getByLabelText("真实生成测试算力池路由")).toBeEnabled());
    await userEvent.click(screen.getByLabelText("真实生成测试算力池路由"));
    expect(screen.getByLabelText("测试接口 /chat/completions")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await userEvent.click(screen.getByLabelText("开始真实生成测试"));
    await waitFor(() =>
      expect(routePoolTestModel).toHaveBeenLastCalledWith({
        platform: "codex",
        model: null,
        interface_format: "openai",
      }),
    );
  });

  it("does not show the endpoint selector for Claude", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));

    expect(screen.queryByLabelText("测试接口 /responses")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("测试接口 /chat/completions")).not.toBeInTheDocument();
  });

  it("hydrates the test model from localStorage for that account", async () => {
    window.localStorage.setItem(
      MODEL_TEST_MODELS_STORAGE_KEY,
      JSON.stringify({
        "cred-api-1": { model: "gpt-5.6-sol", platform: "codex" },
      }),
    );

    renderScreen("codex", "out_of_pool");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    expect(await screen.findByLabelText("弹窗测试模型")).toHaveValue("gpt-5.6-sol");
  });

  it("persists the test model only after the test starts", async () => {
    renderScreen("codex", "out_of_pool");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    await userEvent.type(await screen.findByLabelText("弹窗测试模型"), "  gpt-4o  ");
    // Typing must not reach storage; only submitting does.
    expect(window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY)).toBeNull();

    await userEvent.click(screen.getByLabelText("开始真实生成测试"));

    await waitFor(() =>
      expect(
        JSON.parse(window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY) ?? "null"),
      ).toEqual({
        "cred-api-1": { model: "gpt-4o", platform: "codex" },
      }),
    );
    // What got stored must equal what was actually sent upstream.
    expect(routePoolTestModel).toHaveBeenCalledWith(
      expect.objectContaining({ account_id: "cred-api-1", model: "gpt-4o" }),
    );
  });

  it("keeps the optional test model separately for each account", async () => {
    renderScreen("codex", "out_of_pool");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    await userEvent.type(await screen.findByLabelText("弹窗测试模型"), "gpt-4o");
    await userEvent.click(screen.getByLabelText("关闭真实生成测试弹窗"));

    // A different account on the same platform starts empty.
    const officialTest = screen.getByLabelText("测试 Team Account");
    await waitFor(() => expect(officialTest).toBeEnabled());
    await userEvent.click(officialTest);
    const officialInput = await screen.findByLabelText("弹窗测试模型");
    expect(officialInput).toHaveValue("");
    await userEvent.type(officialInput, "gpt-5.5");
    await userEvent.click(screen.getByLabelText("关闭真实生成测试弹窗"));

    // Each account still remembers its own value.
    await userEvent.click(screen.getByLabelText("测试 API Account"));
    expect(await screen.findByLabelText("弹窗测试模型")).toHaveValue("gpt-4o");
    await userEvent.click(screen.getByLabelText("关闭真实生成测试弹窗"));

    await userEvent.click(screen.getByLabelText("测试 Team Account"));
    expect(await screen.findByLabelText("弹窗测试模型")).toHaveValue("gpt-5.5");
  });

  it("keeps the pool test model separate from any account cache", async () => {
    window.localStorage.setItem(
      MODEL_TEST_MODELS_STORAGE_KEY,
      JSON.stringify({
        "cred-api-1": { model: "gpt-4o", platform: "codex" },
        "pool:codex": { model: "gpt-5", platform: "codex" },
      }),
    );
    // The pool test button stays disabled while the pool has no eligible member.
    poolStateByPlatform.set("codex", ["cred-official-1"]);
    renderScreen("codex", "out_of_pool");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    expect(await screen.findByLabelText("弹窗测试模型")).toHaveValue("gpt-4o");
    await userEvent.click(screen.getByLabelText("关闭真实生成测试弹窗"));

    await waitFor(() =>
      expect(screen.getByLabelText("真实生成测试算力池路由")).toBeEnabled(),
    );
    await userEvent.click(screen.getByLabelText("真实生成测试算力池路由"));
    expect(await screen.findByLabelText("弹窗测试模型")).toHaveValue("gpt-5");
  });

  it("drops the cached model when the field is cleared and the test starts", async () => {
    window.localStorage.setItem(
      MODEL_TEST_MODELS_STORAGE_KEY,
      JSON.stringify({
        "cred-api-1": { model: "gpt-4o", platform: "codex" },
        "pool:codex": { model: "gpt-5", platform: "codex" },
      }),
    );

    renderScreen("codex", "out_of_pool");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    await userEvent.clear(await screen.findByLabelText("弹窗测试模型"));
    await userEvent.click(screen.getByLabelText("开始真实生成测试"));

    await waitFor(() =>
      expect(
        JSON.parse(window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY) ?? "null"),
      ).toEqual({
        "pool:codex": { model: "gpt-5", platform: "codex" },
      }),
    );
  });

  it("saves the pool-wide client config and rejects non-objects", async () => {
    renderScreen("claude", "in_pool");

    await userEvent.click(await screen.findByLabelText("编辑全局客户端配置"));
    const input = await screen.findByLabelText("全局客户端配置 JSON");

    // A JSON array parses but is not a settings object — reject before saving,
    // because the writer silently ignores a malformed value.
    fireEvent.change(input, { target: { value: "[1,2]" } });
    await userEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(await screen.findByText(/需要一个 JSON 对象/)).toBeInTheDocument();
    expect(vi.mocked(saveSettings)).not.toHaveBeenCalled();

    fireEvent.change(input, { target: { value: '{"includeCoAuthoredBy": false}' } });
    await userEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() =>
      expect(vi.mocked(saveSettings)).toHaveBeenCalledWith(
        expect.objectContaining({
          claude_client_config_json: '{"includeCoAuthoredBy": false}',
        }),
      ),
    );
  });

  it("clears the pool-wide client config when emptied", async () => {
    vi.mocked(getSettings).mockResolvedValue({
      ...settingsFixture,
      claude_client_config_json: '{"includeCoAuthoredBy":false}',
    });
    renderScreen("claude", "in_pool");

    await userEvent.click(await screen.findByLabelText("编辑全局客户端配置"));
    const input = await screen.findByLabelText("全局客户端配置 JSON");
    expect(input).toHaveValue('{"includeCoAuthoredBy":false}');

    await userEvent.clear(input);
    await userEvent.click(screen.getByRole("button", { name: "保存" }));

    // null, not "" — the writer treats absent and empty the same, but null is
    // the honest representation of "manage nothing".
    await waitFor(() =>
      expect(vi.mocked(saveSettings)).toHaveBeenCalledWith(
        expect.objectContaining({ claude_client_config_json: null }),
      ),
    );
  });

  it("offers the global client config for Claude only", async () => {
    renderScreen("codex", "in_pool");

    await screen.findByLabelText("写入路由配置文件");
    expect(screen.queryByLabelText("编辑全局客户端配置")).not.toBeInTheDocument();
  });

  it("prunes cached models for accounts that no longer exist", async () => {
    window.localStorage.setItem(
      MODEL_TEST_MODELS_STORAGE_KEY,
      JSON.stringify({
        "cred-api-1": { model: "gpt-4o", platform: "codex" },
        "cred-deleted": { model: "gpt-4.1", platform: "codex" },
        "cred-claude": { model: "claude-opus-4-8", platform: "claude" },
        "pool:codex": { model: "gpt-5", platform: "codex" },
      }),
    );

    renderScreen("codex", "out_of_pool");
    await screen.findByLabelText("测试 API Account");

    // Only this platform's orphan is dropped; other platforms and pool keys stay.
    await waitFor(() =>
      expect(
        JSON.parse(window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY) ?? "null"),
      ).toEqual({
        "cred-api-1": { model: "gpt-4o", platform: "codex" },
        "cred-claude": { model: "claude-opus-4-8", platform: "claude" },
        "pool:codex": { model: "gpt-5", platform: "codex" },
      }),
    );
  });

  it("keeps a half-typed test model when the account list refetches", async () => {
    window.localStorage.setItem(
      MODEL_TEST_MODELS_STORAGE_KEY,
      JSON.stringify({
        "cred-api-1": { model: "gpt-4o", platform: "codex" },
        "cred-official-1": { model: "gpt-5", platform: "codex" },
      }),
    );

    renderScreen("codex", "out_of_pool");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    const input = await screen.findByLabelText("弹窗测试模型");
    await userEvent.clear(input);
    await userEvent.type(input, "gpt-5.6-sol");
    await waitFor(() => expect(transportTestState.statusHandler).not.toBeNull());

    // Drop the other account so the refetch both changes the query reference and
    // gives the prune a real orphan to remove. The resulting storage write is the
    // only reliable proof that the effect re-ran against the new list.
    vi.mocked(listRouteCredentials).mockResolvedValue([credentialsFixture[1]]);
    act(() => {
      transportTestState.statusHandler?.({
        platform: "codex",
        credential_id: "cred-official-1",
      });
    });
    await waitFor(() =>
      expect(
        JSON.parse(window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY) ?? "null"),
      ).toEqual({ "cred-api-1": { model: "gpt-4o", platform: "codex" } }),
    );

    // Pruning must not reload the whole map from storage: doing so would revert
    // the box to the last-submitted "gpt-4o" and eat what the user is typing.
    expect(screen.getByLabelText("弹窗测试模型")).toHaveValue("gpt-5.6-sol");
  });

  it("offers this account's mapped aliases plus baseline models in the single-account test dropdown", async () => {
    renderScreen("codex", "out_of_pool");

    await userEvent.click(await screen.findByLabelText("测试 API Account"));
    await screen.findByLabelText("弹窗测试模型");

    const datalist = document.getElementById("model-test-model-options");
    expect(datalist).not.toBeNull();
    const options = Array.from(datalist!.querySelectorAll("option")).map((option) => option.value);
    // 本账号映射别名 gpt-5 + codex 基线模型，不含其它平台。
    expect(options).toEqual([
      "gpt-5",
      "gpt-5.6-sol",
      "gpt-5.6-terra",
      "gpt-5.6-luna",
      "gpt-5.5",
    ]);
  });

  it("collapses row actions into an overflow menu when the account list is narrow", async () => {
    const globalRef = globalThis as unknown as { ResizeObserver?: unknown };
    const original = globalRef.ResizeObserver;
    // jsdom 下没有 ResizeObserver，注入桩即可让回调把 clientWidth(0) 判定为窄宽度并折叠。
    class StubResizeObserver {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
    globalRef.ResizeObserver = StubResizeObserver;
    try {
      renderScreen("codex", "out_of_pool");

      const overflow = await screen.findByLabelText("更多操作 API Account");
      expect(screen.queryByLabelText("测试 API Account")).not.toBeInTheDocument();
      expect(screen.queryByLabelText("编辑 API Account")).not.toBeInTheDocument();

      await userEvent.click(overflow);

      expect(await screen.findByRole("menu", { name: "API Account 操作菜单" })).toBeInTheDocument();
      expect(screen.getByLabelText("测试 API Account")).toBeInTheDocument();
      expect(screen.getByLabelText("编辑 API Account")).toBeInTheDocument();
    } finally {
      if (original === undefined) {
        delete globalRef.ResizeObserver;
      } else {
        globalRef.ResizeObserver = original;
      }
    }
  });

  it("closes the model connectivity result panel", async () => {
    poolStateByPlatform.set("codex", ["cred-official-1"]);
    renderScreen("codex", "in_pool");

    await userEvent.click(await screen.findByLabelText("真实生成测试算力池路由"));
    await userEvent.click(await screen.findByLabelText("开始真实生成测试"));
    expect(await screen.findByLabelText("真实生成测试结果")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("关闭真实生成测试结果"));

    await waitFor(() =>
      expect(screen.queryByLabelText("真实生成测试结果")).not.toBeInTheDocument(),
    );
  });

  it("shows model connectivity failure details from the route test", async () => {
    vi.mocked(routePoolTestModel).mockResolvedValue(
      modelTestOutcomeFixture({
        response_status: 401,
        response_body: "{\"error\":{\"message\":\"bad key\"}}",
        response_text: null,
        success: false,
        duration_ms: 88,
      }),
    );

    poolStateByPlatform.set("codex", ["cred-official-1"]);
    renderScreen("codex", "in_pool");

    await userEvent.click(await screen.findByLabelText("真实生成测试算力池路由"));
    await userEvent.click(await screen.findByLabelText("开始真实生成测试"));

    expect(await screen.findByText("真实生成测试：失败")).toBeInTheDocument();
    expect(screen.getByText("HTTP 401 · 88 ms")).toBeInTheDocument();
    expect(screen.getByText(/bad key/)).toBeInTheDocument();
    expect(screen.getByText("Team Account")).toBeInTheDocument();
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
    expect(screen.getByText(/operation operation-1/)).toBeInTheDocument();
    expect(screen.getByText(/snapshot snapshot-1/)).toBeInTheDocument();
    expect(screen.getByText(/before before-hash/)).toBeInTheDocument();
    expect(screen.queryByText(/sk-ai-switch-test/)).not.toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(2999);
    });
    expect(screen.getByText("配置写入结果")).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.queryByText("配置写入结果")).not.toBeInTheDocument();
  });

  it("uses distinct service start, stop, and send-test controls", async () => {
    poolStateByPlatform.set("codex", ["cred-official-1"]);
    renderScreen("codex", "in_pool");

    const startButton = await screen.findByLabelText("启动本地路由代理");
    expect(startButton).toHaveClass("bg-emerald-600");
    expect(screen.getByLabelText("真实生成测试算力池路由")).toHaveClass("bg-transparent");

    await userEvent.click(startButton);
    const stopButton = await screen.findByLabelText("停止本地路由代理");
    expect(stopButton).toHaveClass("bg-red-600");
    expect(screen.queryByLabelText("启动本地路由代理")).not.toBeInTheDocument();

    await userEvent.click(stopButton);
    await waitFor(() => expect(stopRouteProxy).toHaveBeenCalledTimes(1));
  });
});
