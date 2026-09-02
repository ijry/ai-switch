import { QueryClientProvider } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  archiveRouteCredentials,
  clearRouteCredentialModelState,
  createBatch,
  copyRouteCredential,
  createApiRouteCredential,
  deleteRouteCredential,
  fetchRouteModels,
  getRoutePool,
  getRouteProxyKey,
  getRouteProxyStatus,
  getUsageOverview,
  getSettings,
  importExternalClientAccounts,
  importOfficialRouteCredentialsFromFiles,
  importOfficialRouteCredentialsFromText,
  listConfigWriteClients,
  listPlatformCapabilities,
  listRouteCredentials,
  listRouteCredentialPage,
  reorderRouteCredentials,
  refreshRouteCredentialRelayBalance,
  refreshRouteCredentialsQuota,
  restoreRouteCredentials,
  routePoolTestModel,
  saveSettings,
  setRouteCredentialRecovery,
  setRouteCredentialModelStatus,
  setRouteCredentialStatuses,
  setRoutePoolMembers,
  startRouteProxy,
  stopRouteProxy,
  subscribeRouteProxyLiveLog,
  unsubscribeRouteProxyLiveLog,
  updateRouteCredential,
  routeConfigWriteIsStale,
  writeRouteProxyConfigs,
  previewExternalClientImport,
} from "../src/lib/api/client";
import { recognizeApiKeysFromImageBlob } from "../src/lib/ocr/apiKeyOcr";
import { CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY } from "../src/lib/codexModelTestEndpoint";
import { MODEL_TEST_MODELS_STORAGE_KEY } from "../src/lib/modelTestModels";
import { openExternal } from "../src/lib/openExternal";
import { createQueryClient } from "../src/lib/query/queryClient";
import { settingsFixture } from "../src/test/fixtures";
import { AccountsScreen } from "../src/screens/AccountsScreen";
import { fetchRouteProxyModels } from "../src/lib/routeProxyModels";
import type {
  CapabilityAvailability,
  CapabilityRule,
  ConfigWriteClientStatus,
  ExternalClientAccountPreviewItem,
  ExternalClientImportPreview,
  PlatformCapability,
  PlatformId,
  RouteCredential,
  RouteCredentialActivityEvent,
  RouteCredentialModelState,
  RoutePoolModelTestOutcome,
  RoutePoolStats,
  RoutePoolUsageLog,
} from "../src/lib/api/types";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("../src/lib/api/client", () => ({
  archiveRouteCredentials: vi.fn(),
  clearRouteCredentialModelState: vi.fn(),
  createBatch: vi.fn(),
  copyRouteCredential: vi.fn(),
  createApiRouteCredential: vi.fn(),
  deleteRouteCredential: vi.fn(),
  fetchRouteModels: vi.fn(),
  getRoutePool: vi.fn(),
  getRouteProxyKey: vi.fn(),
  getRouteProxyStatus: vi.fn(),
  getUsageOverview: vi.fn(),
  importExternalClientAccounts: vi.fn(),
  importOfficialRouteCredentialsFromFiles: vi.fn(),
  importOfficialRouteCredentialsFromText: vi.fn(),
  listConfigWriteClients: vi.fn(),
  listPlatformCapabilities: vi.fn(),
  listRouteCredentials: vi.fn(),
  listRouteCredentialPage: vi.fn(),
  reorderRouteCredentials: vi.fn(),
  refreshRouteCredentialRelayBalance: vi.fn(),
  refreshRouteCredentialsQuota: vi.fn(),
  refreshRouteCredentialsRelayBalance: vi.fn(),
  restoreRouteCredentials: vi.fn(),
  routePoolTestModel: vi.fn(),
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
  setRouteCredentialRecovery: vi.fn(),
  setRouteCredentialModelStatus: vi.fn(),
  setRouteCredentialStatuses: vi.fn(),
  setRoutePoolMembers: vi.fn(),
  startRouteProxy: vi.fn(),
  stopRouteProxy: vi.fn(),
  subscribeRouteProxyLiveLog: vi.fn(),
  unsubscribeRouteProxyLiveLog: vi.fn(),
  updateRouteCredential: vi.fn(),
  routeConfigWriteIsStale: vi.fn(),
  writeRouteProxyConfigs: vi.fn(),
  previewExternalClientImport: vi.fn(),
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

vi.mock("../src/lib/openExternal", () => ({
  openExternal: vi.fn(async () => {}),
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

function externalPreviewItemFixture(
  overrides: Partial<ExternalClientAccountPreviewItem> = {},
): ExternalClientAccountPreviewItem {
  return {
    source_id: "codex:p1",
    display_name: "kktoken",
    platform: "codex",
    interface_format: "openai-responses",
    base_url: "https://kktoken.cc/v1",
    api_key_masked: "sk-1***7890",
    model_mapping_count: 1,
    disposition: "create",
    existing_credential_id: null,
    existing_display_name: null,
    issue_codes: [],
    ...overrides,
  };
}

function externalPreviewFixture(
  items: ExternalClientAccountPreviewItem[],
  overrides: Partial<ExternalClientImportPreview["counts"]> = {},
): ExternalClientImportPreview {
  const importable = items.filter(
    (item) => item.disposition === "create" || item.disposition === "overwrite",
  );
  return {
    client: "cc-switch",
    source_path: "C:/Users/example/.cc-switch/cc-switch.db",
    counts: {
      total: items.length,
      importable: importable.length,
      create: items.filter((item) => item.disposition === "create").length,
      overwrite: items.filter((item) => item.disposition === "overwrite").length,
      errors: items.filter((item) => item.disposition === "error").length,
      other_platform: 0,
      other_platform_counts: {},
      ...overrides,
    },
    items,
  };
}

/** What `list_config_write_clients` returns for Codex once ZCode is seeded. */
const configWriteClientsFixture: ConfigWriteClientStatus[] = [
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

async function openFormTab(name: "基础" | "高级" | "故障处理" | "其他") {
  await userEvent.click(await screen.findByRole("tab", { name }));
}

// jsdom has no layout, and the pointer drag reads row geometry to decide where the
// placeholder goes, so the rows under test get their rects handed to them.
function stubRect(element: HTMLElement, top: number, height: number) {
  element.getBoundingClientRect = () =>
    ({
      top,
      bottom: top + height,
      height,
      left: 0,
      right: 320,
      width: 320,
      x: 0,
      y: top,
      toJSON: () => ({}),
    }) as DOMRect;
}

const ROW_HEIGHT = 40;
const ROW_PITCH = 42;

function stubAccountRowGeometry(rows: HTMLElement[]) {
  stubRect(screen.getByTestId("account-workspace-scroll-region"), 0, 600);
  rows.forEach((row, index) => stubRect(row, index * ROW_PITCH, ROW_HEIGHT));
}

function dispatchPointerEvent(
  target: EventTarget,
  type: "pointerdown" | "pointermove" | "pointerup",
  { button = 0, clientX = 0, clientY = 0, pointerId = 1 } = {},
) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperties(event, {
    button: { configurable: true, value: button },
    clientX: { configurable: true, value: clientX },
    clientY: { configurable: true, value: clientY },
    pointerId: { configurable: true, value: pointerId },
  });
  act(() => {
    target.dispatchEvent(event);
  });
}

describe("AccountsScreen", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  beforeEach(() => {
    window.localStorage.clear();
    vi.mocked(open).mockReset();
    vi.mocked(openExternal).mockReset();
    vi.mocked(openExternal).mockResolvedValue(undefined);
    vi.mocked(archiveRouteCredentials).mockReset();
    vi.mocked(createBatch).mockReset();
    vi.mocked(copyRouteCredential).mockReset();
    vi.mocked(createApiRouteCredential).mockReset();
    vi.mocked(deleteRouteCredential).mockReset();
    vi.mocked(fetchRouteModels).mockReset();
    vi.mocked(getRoutePool).mockReset();
    vi.mocked(getRouteProxyKey).mockReset();
    vi.mocked(getRouteProxyStatus).mockReset();
    vi.mocked(getUsageOverview).mockReset();
    // Default to an empty overview so tests that only switch to the stats view
    // resolve the panel's query instead of leaving it pending.
    vi.mocked(getUsageOverview).mockResolvedValue({
      totals: {
        request_count: 0,
        input_tokens: 0,
        output_tokens: 0,
        cache_write_tokens: 0,
        cache_read_tokens: 0,
        cost_micros: 0,
      },
      rows: [],
      groups: { by_model: [], by_platform: [], by_account: [], by_source: [] },
      row_count: 0,
      page: 1,
      page_size: 20,
      integrity: {
        scanned_file_count: 0,
        truncated: false,
        unpriced_request_count: 0,
        estimated_price_request_count: 0,
        unmatchable_proxy_row_count: 0,
      },
    });
    vi.mocked(importExternalClientAccounts).mockReset();
    vi.mocked(importOfficialRouteCredentialsFromFiles).mockReset();
    vi.mocked(importOfficialRouteCredentialsFromText).mockReset();
    vi.mocked(previewExternalClientImport).mockReset();
    // The panel only reads another app's config when its tab is open, so the
    // default is an empty preview rather than a rejection.
    vi.mocked(previewExternalClientImport).mockResolvedValue(externalPreviewFixture([]));
    vi.mocked(listConfigWriteClients).mockReset();
    vi.mocked(listConfigWriteClients).mockResolvedValue(configWriteClientsFixture);
    vi.mocked(listPlatformCapabilities).mockReset();
    vi.mocked(listRouteCredentials).mockReset();
    vi.mocked(listRouteCredentialPage).mockReset();
    vi.mocked(reorderRouteCredentials).mockReset();
    vi.mocked(refreshRouteCredentialsQuota).mockReset();
    vi.mocked(restoreRouteCredentials).mockReset();
    vi.mocked(routePoolTestModel).mockReset();
    vi.mocked(setRouteCredentialRecovery).mockReset();
    vi.mocked(setRouteCredentialModelStatus).mockReset();
    vi.mocked(clearRouteCredentialModelState).mockReset();
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
    vi.mocked(routeConfigWriteIsStale).mockReset();
    vi.mocked(routeConfigWriteIsStale).mockResolvedValue(false);
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
    // Kept out of the way until the row is hovered, but reachable by keyboard —
    // opacity rather than `hidden` is what preserves that.
    expect(openLinkButton).toHaveClass(
      "opacity-0",
      "group-hover/name:opacity-100",
      "focus-visible:opacity-100",
    );
    // The desktop webview swallows `window.open`, so the link has to go through
    // the opener adapter instead.
    await userEvent.click(openLinkButton);
    expect(openExternal).toHaveBeenCalledWith("https://api.example.com");
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

  it("reorders accounts when a dragged handle is released over the placeholder slot", async () => {
    renderScreen();

    const dragHandle = await screen.findByLabelText("拖动 Team Account");
    const firstRow = screen.getByLabelText("放置在 Team Account 前");
    const secondRow = screen.getByLabelText("放置在 API Account 前");
    stubAccountRowGeometry([firstRow, secondRow]);

    dispatchPointerEvent(dragHandle, "pointerdown", { clientY: 20 });
    dispatchPointerEvent(document, "pointermove", { clientY: 70 });

    expect(dragHandle).toHaveAttribute("aria-grabbed", "true");
    expect(firstRow.style.position).toBe("fixed");
    // The placeholder marks the slot the row lands in, below the row it passed.
    const placeholder = screen.getByTestId("account-drop-placeholder");
    expect(placeholder.previousElementSibling).toBe(secondRow);

    dispatchPointerEvent(document, "pointerup", { clientY: 70 });

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
    expect(screen.queryByTestId("account-drop-placeholder")).not.toBeInTheDocument();
    expect(firstRow.style.position).toBe("");
  });

  it("moves the last account to the front when dragged over the first row", async () => {
    renderScreen();

    const dragHandle = await screen.findByLabelText("拖动 API Account");
    const firstRow = screen.getByLabelText("放置在 Team Account 前");
    const secondRow = screen.getByLabelText("放置在 API Account 前");
    stubAccountRowGeometry([firstRow, secondRow]);

    dispatchPointerEvent(dragHandle, "pointerdown", { clientY: 60 });
    dispatchPointerEvent(document, "pointermove", { clientY: 8 });

    const placeholder = screen.getByTestId("account-drop-placeholder");
    expect(placeholder.nextElementSibling).toBe(firstRow);

    dispatchPointerEvent(document, "pointerup", { clientY: 8 });

    await waitFor(() =>
      expect(reorderRouteCredentials).toHaveBeenCalledWith({
        platform: "codex",
        moved_account_id: "cred-api-1",
        previous_account_id: null,
        next_account_id: "cred-official-1",
        filters: [],
        pool_scope: "out_of_pool",
        page_size: 20,
      }),
    );
  });

  it("leaves the order alone when a drag is released where it started", async () => {
    renderScreen();

    const dragHandle = await screen.findByLabelText("拖动 API Account");
    stubAccountRowGeometry([
      screen.getByLabelText("放置在 Team Account 前"),
      screen.getByLabelText("放置在 API Account 前"),
    ]);

    dispatchPointerEvent(dragHandle, "pointerdown", { clientY: 60 });
    dispatchPointerEvent(document, "pointermove", { clientY: 52 });
    dispatchPointerEvent(document, "pointerup", { clientY: 52 });

    expect(reorderRouteCredentials).not.toHaveBeenCalled();
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

  it("opens a same-platform-only copy dialog for official accounts", async () => {
    renderScreen();

    await userEvent.click(await screen.findByLabelText("复制 Team Account"));
    const dialog = screen.getByRole("dialog", { name: "复制账号" });
    const targetSelect = within(dialog).getByLabelText("复制目标");
    expect(targetSelect).toHaveValue("codex");
    expect(within(targetSelect).getAllByRole("option")).toHaveLength(1);
    expect(within(dialog).getByText("官方账号仅支持复制到当前智能体。")).toBeInTheDocument();
    expect(within(dialog).queryByLabelText("新 API Key（可选）")).not.toBeInTheDocument();
    expect(copyRouteCredential).not.toHaveBeenCalled();

    await userEvent.click(within(dialog).getByRole("button", { name: "确认复制" }));
    await waitFor(() =>
      expect(copyRouteCredential).toHaveBeenCalledWith("cred-official-1", {
        target_platform: "codex",
      }),
    );
    expect(screen.getByLabelText("复制 Team Account")).toHaveTextContent("已复制");
  });

  it("copies an API account to another agent with a compatibility warning and optional key", async () => {
    vi.mocked(copyRouteCredential).mockResolvedValueOnce({
      ...credentialsFixture[1],
      id: "cred-api-claude-copy",
      platform: "claude",
      display_name: "API Account 2026-08-31",
      secret_payload_json: '{"api_key":"sk-override"}',
      config_json: '{"base_url":"https://api.example.com","interface_format":"anthropic"}',
    });
    renderScreen();

    await userEvent.click(await screen.findByLabelText("复制 API Account"));
    const dialog = screen.getByRole("dialog", { name: "复制账号" });
    expect(within(dialog).getByText("API Account")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("复制目标")).toHaveValue("codex");
    expect(within(dialog).getByLabelText("新 API Key（可选）")).toHaveAttribute(
      "placeholder",
      "不填则复制原 API Key",
    );
    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();

    await userEvent.selectOptions(within(dialog).getByLabelText("复制目标"), "claude");
    expect(within(dialog).getByRole("alert")).toHaveTextContent(
      "复制到其他智能体不会保留模型映射、已获取模型等不兼容配置",
    );
    await userEvent.type(within(dialog).getByLabelText("新 API Key（可选）"), "sk-override");
    await userEvent.click(within(dialog).getByRole("button", { name: "确认复制" }));

    await waitFor(() =>
      expect(copyRouteCredential).toHaveBeenCalledWith("cred-api-1", {
        target_platform: "claude",
        api_key: "sk-override",
      }),
    );
    expect(screen.queryByRole("dialog", { name: "复制账号" })).not.toBeInTheDocument();
    expect(screen.queryByText("API Account 2026-08-31")).not.toBeInTheDocument();
  });

  it("keeps the original API key when the optional copy field is blank", async () => {
    renderScreen();

    await userEvent.click(await screen.findByLabelText("复制 API Account"));
    const dialog = screen.getByRole("dialog", { name: "复制账号" });
    await userEvent.click(within(dialog).getByRole("button", { name: "确认复制" }));

    await waitFor(() =>
      expect(copyRouteCredential).toHaveBeenCalledWith("cred-api-1", {
        target_platform: "codex",
      }),
    );
  });

  it("keeps the copy dialog open and reports API errors", async () => {
    vi.mocked(copyRouteCredential).mockRejectedValueOnce(new Error("copy request failed"));
    renderScreen();

    await userEvent.click(await screen.findByLabelText("复制 API Account"));
    const dialog = screen.getByRole("dialog", { name: "复制账号" });
    await userEvent.click(within(dialog).getByRole("button", { name: "确认复制" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("copy request failed");
    expect(screen.getByRole("dialog", { name: "复制账号" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "确认复制" })).toBeEnabled();
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
    await userEvent.click(
      within(screen.getByRole("dialog", { name: "复制账号" })).getByRole("button", {
        name: "确认复制",
      }),
    );

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
    expect(deleteRouteCredential).not.toHaveBeenCalled();
    await userEvent.click(await screen.findByLabelText("确认删除"));

    await waitFor(() => {
      expect(deleteRouteCredential).toHaveBeenCalledTimes(1);
      expect(deleteRouteCredential).toHaveBeenCalledWith("cred-api-1");
    });
    await waitFor(() => {
      const lastCall = vi.mocked(setRoutePoolMembers).mock.calls.at(-1)?.[0];
      expect(lastCall?.account_ids).toEqual([]);
    });
  });

  it("keeps selected accounts when batch delete confirmation is cancelled", async () => {
    renderScreen();

    await userEvent.click(await screen.findByLabelText("选择 Team Account"));
    await userEvent.click(screen.getByLabelText("批量删除账号"));

    expect(await screen.findByLabelText("删除确认弹窗")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "取消" }));

    await waitFor(() =>
      expect(screen.queryByLabelText("删除确认弹窗")).not.toBeInTheDocument(),
    );
    expect(deleteRouteCredential).not.toHaveBeenCalled();
    expect(screen.getByText("已选 1 个账号")).toBeInTheDocument();
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
    expect(await screen.findByText("用量总览")).toBeInTheDocument();
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

  it("adds a sensitive-word hint when the upstream reports sensitive_words_detected", async () => {
    vi.mocked(listRouteCredentials).mockResolvedValue([
      {
        ...credentialsFixture[0],
        status: "error",
        last_failure_kind: "upstream_status",
        last_failure_message: "upstream returned 400",
        last_failure_response_json:
          '{"error":{"code":"sensitive_words_detected","message":"sensitive words detected (request id: 20260829114519886186250zsjglFr21CX8y)","type":"new_api_error"}}',
      },
      {
        ...credentialsFixture[1],
        status: "error",
        last_failure_kind: "upstream_status",
        last_failure_message: "upstream returned 429",
        last_failure_response_json: '{"error":{"message":"rate limited"}}',
      },
    ]);

    renderScreen();

    const firstRow = await screen.findByLabelText("放置在 Team Account 前");
    const secondRow = screen.getByLabelText("放置在 API Account 前");
    expect(
      within(firstRow).getByText(
        /当前中转站似乎对项目存在关键词检测，您的项目可能存在敏感词，也不排除是中转站误判。/,
      ),
    ).toBeInTheDocument();
    expect(within(secondRow).queryByText(/关键词检测/)).not.toBeInTheDocument();
  });

  it("explains that an exhausted budget pool is the relay's, not the user's own quota", async () => {
    vi.mocked(listRouteCredentials).mockResolvedValue([
      {
        ...credentialsFixture[0],
        status: "error",
        last_failure_kind: "semantic_response_failed",
        last_failure_message:
          "Budget pool quota has been exhausted. Please ask an administrator to increase the limit or select another budget pool.",
        last_failure_response_json:
          '{"error":{"message":"Budget pool quota has been exhausted. Please ask an administrator to increase the limit or select another budget pool.","type":"bad_response_status_code","param":"","code":"bad_response_status_code"}}',
      },
      {
        // The user's own balance running out must NOT get the reassurance.
        ...credentialsFixture[1],
        status: "error",
        last_failure_kind: "semantic_response_failed",
        last_failure_message: "用户额度不足, 剩余额度: ＄-0.398052",
        last_failure_response_json:
          '{"error":{"type":"new_api_error","message":"用户额度不足, 剩余额度: ＄-0.398052"}}',
      },
      {
        // Recorded by status code, so the original wording survives only in the
        // body — and a relay may not capitalize it the way the sample did.
        ...credentialsFixture[0],
        id: "cred-pool-by-status",
        display_name: "Status Coded Account",
        status: "error",
        last_failure_kind: "upstream_status",
        last_failure_message: "upstream returned 429",
        last_failure_response_json:
          '{"error":{"message":"budget pool quota has been exhausted, please ask an administrator"}}',
      },
    ]);

    renderScreen();

    const firstRow = await screen.findByLabelText("放置在 Team Account 前");
    const secondRow = screen.getByLabelText("放置在 API Account 前");
    const thirdRow = screen.getByLabelText("放置在 Status Coded Account 前");
    expect(
      within(firstRow).getByText(
        /当前中转站公共池额度耗尽，并非你个人额度耗尽，请等待下一次公共池补充额度。/,
      ),
    ).toBeInTheDocument();
    expect(within(secondRow).queryByText(/公共池/)).not.toBeInTheDocument();
    expect(within(thirdRow).getByText(/公共池额度耗尽/)).toBeInTheDocument();
  });

  it("keeps the failure tooltip hoverable so its text can be selected", async () => {
    vi.mocked(listRouteCredentials).mockResolvedValue([
      {
        ...credentialsFixture[0],
        status: "error",
        last_failure_kind: "semantic_response_failed",
        last_failure_message: "upstream rejected the request",
        last_failure_response_json: '{"error":{"message":"bad key"}}',
      },
      credentialsFixture[1],
    ]);

    renderScreen();

    const firstRow = await screen.findByLabelText("放置在 Team Account 前");
    // The row also renders ModelMappingSummary's baseline tooltip, so reach the
    // failure one through its payload rather than by role.
    const panel = within(firstRow).getByText(/bad key/).closest("pre")
      ?.parentElement as HTMLElement;
    const tooltip = panel.parentElement as HTMLElement;
    expect(tooltip).toHaveAttribute("role", "tooltip");

    // pointer-events-none would make the panel unhittable, so the pointer could
    // never enter it to drag-select the upstream error payload.
    expect(tooltip.className).not.toContain("pointer-events-none");
    expect(panel.className).not.toContain("pointer-events-none");
    expect(panel.className).toContain("select-text");
    // Padding, not margin: a margin gap drops :hover before the pointer arrives.
    expect(tooltip.className).toContain("pt-1");
    expect(tooltip.className).not.toContain("mt-1");
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

  it("imports selected cc-switch accounts and joins only the new ones to the pool", async () => {
    vi.mocked(previewExternalClientImport).mockResolvedValue(
      externalPreviewFixture([
        externalPreviewItemFixture(),
        externalPreviewItemFixture({
          source_id: "codex:p2",
          display_name: "gorouter",
          disposition: "overwrite",
          existing_credential_id: "cred-api-1",
          existing_display_name: "API Account",
        }),
      ]),
    );
    const created: RouteCredential = {
      ...credentialsFixture[1],
      id: "cred-api-new",
      display_name: "kktoken",
    };
    vi.mocked(importExternalClientAccounts).mockResolvedValue({
      created: 1,
      overwritten: 1,
      skipped: 0,
      failed: 0,
      imported: [created, credentialsFixture[1]],
      created_ids: [created.id],
    });
    vi.mocked(setRoutePoolMembers).mockImplementation(async ({ platform, account_ids }) => {
      poolStateByPlatform.set(platform, [...account_ids]);
      return {
        platform,
        account_ids: [...account_ids],
        stats: statsFixture({ member_count: account_ids.length }),
      };
    });
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "导入其他客户端" }));

    await waitFor(() =>
      expect(previewExternalClientImport).toHaveBeenCalledWith({
        client: "cc-switch",
        platform: "codex",
        source_path: null,
      }),
    );
    expect(await screen.findByText("kktoken")).toBeInTheDocument();
    expect(screen.getByText("覆盖已有")).toBeInTheDocument();
    expect(screen.getByText(/将覆盖「API Account」/)).toBeInTheDocument();
    // Everything importable starts checked, so the default action is "take all".
    expect(screen.getByRole("checkbox", { name: "导入 kktoken" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "导入 gorouter" })).toBeChecked();

    await userEvent.click(screen.getByRole("button", { name: /导入所选账号/ }));

    await waitFor(() =>
      expect(importExternalClientAccounts).toHaveBeenCalledWith({
        client: "cc-switch",
        platform: "codex",
        source_path: null,
        source_ids: ["codex:p1", "codex:p2"],
      }),
    );
    // Only the created id joins the pool: an overwrite must not move an account
    // the user had deliberately left out.
    await waitFor(() =>
      expect(setRoutePoolMembers).toHaveBeenCalledWith({
        platform: "codex",
        account_ids: ["cred-api-new"],
      }),
    );
    expect(await screen.findByText(/新增 1 个，覆盖 1 个/)).toBeInTheDocument();
  });

  it("keeps unpickable cc-switch entries out of the import selection", async () => {
    vi.mocked(previewExternalClientImport).mockResolvedValue(
      externalPreviewFixture([
        externalPreviewItemFixture({
          source_id: "codex:broken",
          display_name: "No key",
          base_url: null,
          api_key_masked: null,
          interface_format: null,
          model_mapping_count: 0,
          disposition: "error",
          issue_codes: ["external_import.api_key_missing"],
        }),
      ]),
    );
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "导入其他客户端" }));

    expect(await screen.findByText("No key")).toBeInTheDocument();
    expect(screen.getByText("缺少 API Key")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "导入 No key" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "全选可导入账号" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "导入所选账号" })).toBeDisabled();
  });

  it("re-reads the preview from a hand-picked config file", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "导入其他客户端" }));
    await waitFor(() => expect(previewExternalClientImport).toHaveBeenCalledTimes(1));

    vi.mocked(open).mockResolvedValue("D:\\backup\\cc-switch.db");
    vi.mocked(previewExternalClientImport).mockResolvedValue(
      externalPreviewFixture([externalPreviewItemFixture({ display_name: "from backup" })]),
    );
    await userEvent.click(screen.getByRole("button", { name: "选择客户端配置文件" }));

    await waitFor(() =>
      expect(previewExternalClientImport).toHaveBeenLastCalledWith({
        client: "cc-switch",
        platform: "codex",
        source_path: "D:\\backup\\cc-switch.db",
      }),
    );
    expect(await screen.findByText("from backup")).toBeInTheDocument();
  });

  it("shows the backend error when the client config cannot be read", async () => {
    vi.mocked(previewExternalClientImport).mockRejectedValue({
      code: "external_import.source_not_found",
      message: "Could not find a cc-switch configuration on this machine",
      details: "C:/Users/example/.cc-switch",
      recoverable: true,
    });
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "导入其他客户端" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not find a cc-switch configuration on this machine",
    );
    expect(screen.getByRole("button", { name: "导入所选账号" })).toBeDisabled();
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
        relay_balance_provider: null,
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
    await openFormTab("高级");
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

  it("one-click flags 1M for the roles that have a 1M tier", async () => {
    // Most third-party relays omit `supports_1m` from /v1/models. Reading that
    // silence as a denial meant one-click never flagged 1M on those relays and
    // users had to tick every role by hand.
    vi.mocked(fetchRouteModels).mockResolvedValue([
      { id: "claude-opus-5", owned_by: "gateway" },
    ]);
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Relay API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-relay");
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://relay.test/v1");
    await userEvent.selectOptions(screen.getByLabelText("接口格式"), "anthropic");
    await userEvent.click(screen.getByRole("button", { name: "获取模型列表" }));
    await waitFor(() => expect(fetchRouteModels).toHaveBeenCalled());

    await userEvent.click(screen.getByRole("button", { name: "一键设置" }));

    // Sonnet / Opus / Fable have a 1M tier; Haiku does not.
    expect(screen.getByLabelText("声明支持 1M 1")).toBeChecked();
    expect(screen.getByLabelText("声明支持 1M 2")).toBeChecked();
    expect(screen.getByLabelText("声明支持 1M 3")).toBeChecked();
    expect(screen.queryByLabelText("声明支持 1M 4")).not.toBeInTheDocument();
  });

  it("one-click respects an upstream that explicitly denies 1M", async () => {
    vi.mocked(fetchRouteModels).mockResolvedValue([
      { id: "claude-opus-5", owned_by: "gateway", supports_1m: false },
    ]);
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.type(screen.getByLabelText("API 账号名称"), "Denying API");
    await userEvent.type(screen.getByLabelText("API Key"), "sk-deny");
    await userEvent.clear(screen.getByLabelText("Base URL"));
    await userEvent.type(screen.getByLabelText("Base URL"), "https://deny.test/v1");
    await userEvent.selectOptions(screen.getByLabelText("接口格式"), "anthropic");
    await userEvent.click(screen.getByRole("button", { name: "获取模型列表" }));
    await waitFor(() => expect(fetchRouteModels).toHaveBeenCalled());

    await userEvent.click(screen.getByRole("button", { name: "一键设置" }));

    // An explicit `false` is a denial and must be honoured.
    expect(screen.getByLabelText("声明支持 1M 1")).not.toBeChecked();
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
    expect(screen.getByLabelText("请求模型 2")).toHaveValue("glm-5.3");
    expect(screen.getByLabelText("上游模型 2")).toHaveValue("glm-5.3");
    expect(screen.getByLabelText("请求模型 3")).toHaveValue("deepseek-v4-flash");
    expect(screen.getByLabelText("上游模型 3")).toHaveValue("deepseek-v4-flash");
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

  it("applies the KKToken preset and names the provider in the hint", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.selectOptions(screen.getByLabelText("创建 账号预设"), "kktoken");

    expect(screen.getByLabelText("Base URL")).toHaveValue("https://kktoken.cc/v1");
    expect(screen.getByLabelText("接口格式")).toHaveValue("openai");
    expect(screen.getByLabelText("API 账号名称")).toHaveValue("KKToken");
    expect(screen.getByLabelText("请求模型 1")).toHaveValue("claude-opus-5");
    expect(screen.getByLabelText("上游模型 1")).toHaveValue("claude-opus-5");
    expect(screen.queryByLabelText("请求模型 2")).not.toBeInTheDocument();
    // The hint follows the selected provider rather than hardcoding AgentRouter.
    expect(
      screen.getByText("已套用 KKToken 预设，通常只需填写 API Key。"),
    ).toBeInTheDocument();
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
    // The hand-typed row is gone rather than pushed below the preset rows.
    expect(screen.queryByDisplayValue("foo")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("请求模型 4")).not.toBeInTheDocument();
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
          model_mappings_json:
            "[{\"from\":\"gpt-5.6-sol\",\"to\":\"gpt-5.6-sol\"},{\"from\":\"glm-5.3\",\"to\":\"glm-5.3\"},{\"from\":\"deepseek-v4-flash\",\"to\":\"deepseek-v4-flash\"}]",
        }),
      ),
    );
  });

  it("hides the preset select on platforms without presets", async () => {
    // Claude and Codex both have presets now, so Gemini is what proves the
    // select is conditional rather than always rendered.
    renderScreen("gemini");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));

    expect(screen.queryByLabelText("创建 账号预设")).not.toBeInTheDocument();
  });

  it("applies the Claude preset, filling every role with one upstream model", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.selectOptions(
      screen.getByLabelText("创建 账号预设"),
      "agentrouter-claude",
    );

    expect(screen.getByLabelText("Base URL")).toHaveValue("https://ps.air-outer.com");
    expect(screen.getByLabelText("API 账号名称")).toHaveValue("AgentRouter Claude");
    // All six role rows, including Subagent and the catch-all.
    for (const row of [1, 2, 3, 4, 5, 6]) {
      expect(screen.getByLabelText(`上游模型 ${row}`)).toHaveValue("claude-opus-5");
    }
    // 1M stays for the user to tick: an upstream without the tier answers 503.
    expect(screen.getByLabelText("声明支持 1M 1")).not.toBeChecked();
  });

  it("applies the GoRouter Claude preset to every role", async () => {
    renderScreen("claude");

    await userEvent.click(await screen.findByRole("button", { name: "新增账号" }));
    await userEvent.click(screen.getByRole("button", { name: "API 账号" }));
    await userEvent.selectOptions(
      screen.getByLabelText("创建 账号预设"),
      "gorouter-claude",
    );

    expect(screen.getByLabelText("Base URL")).toHaveValue("https://gorouter.app");
    expect(screen.getByLabelText("API 账号名称")).toHaveValue("GoRouter");
    for (const row of [1, 2, 3, 4, 5, 6]) {
      expect(screen.getByLabelText(`上游模型 ${row}`)).toHaveValue("claude-opus-5");
    }
    expect(
      screen.getByText("已套用 GoRouter 预设，通常只需填写 API Key。"),
    ).toBeInTheDocument();
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
        model_mappings_json: "[{\"from\":\"claude-sonnet-alias\",\"to\":\"provider-sonnet\",\"label\":\"Sonnet\"}]",
        fetched_models_json: "[]",
        preview_json: null,
        batch_id: null,
        responses_custom_tool_compat: false,
        user_agent: null,
        relay_balance_provider: null,
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
    expect(screen.getByLabelText("请求模型 6")).toHaveValue("claude-model");

    // Only Haiku lacks the flag — it has no 1M context tier. Subagent and the
    // fallback keep it: the proxy strips the [1m] suffix before resolving a
    // mapping, so claude-subagent[1m] matches the same entry.
    expect(screen.getByLabelText("声明支持 1M 1")).toBeInTheDocument();
    expect(screen.queryByLabelText("声明支持 1M 4")).not.toBeInTheDocument();
    expect(screen.getByLabelText("声明支持 1M 5")).toBeInTheDocument();
    expect(screen.getByLabelText("声明支持 1M 6")).toBeInTheDocument();
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
    // Both rows can declare 1M — the suffix is stripped before mapping lookup,
    // and the beta marker the proxy sends is what actually enables the window.
    await userEvent.click(screen.getByLabelText("声明支持 1M 5"));
    await userEvent.click(screen.getByRole("button", { name: "保存账号" }));

    await waitFor(() =>
      expect(createApiRouteCredential).toHaveBeenCalledWith(
        expect.objectContaining({
          // Appended after Haiku, with no label key on either row.
          model_mappings_json:
            "[{\"from\":\"claude-subagent\",\"to\":\"provider-haiku\",\"supports_1m\":true},{\"from\":\"claude-model\",\"to\":\"provider-sonnet\"}]",
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
            "[{\"from\":\"claude-sonnet-alias\",\"to\":\"provider-sonnet-1m\",\"label\":\"Sonnet\",\"supports_1m\":true}]",
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
    await openFormTab("高级");
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
    await openFormTab("高级");
    const checkbox = await screen.findByLabelText("兼容 custom 工具（Responses 中转）");
    expect(checkbox).toBeChecked();
    await userEvent.click(checkbox);
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const payload = vi.mocked(updateRouteCredential).mock.calls[0][1];
    const config = JSON.parse(payload.config_json);
    expect(config.responses_custom_tool_compat).toBe(false);
  });

  it("turns the per-turn reminder on with a custom text and writes both keys", async () => {
    const api = {
      ...credentialsFixture[1],
      config_json: JSON.stringify({
        base_url: "https://api.example.com/v1",
        interface_format: "openai",
        model_mappings: [],
      }),
    };
    vi.mocked(listRouteCredentials).mockResolvedValue([api]);
    vi.mocked(updateRouteCredential).mockResolvedValue(api);

    renderScreen();
    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    await openFormTab("高级");

    // The text field only exists once the box is ticked.
    expect(screen.queryByLabelText("纠偏提醒内容")).not.toBeInTheDocument();
    await userEvent.click(await screen.findByLabelText("每轮追加纠偏提醒"));
    await userEvent.type(screen.getByLabelText("纠偏提醒内容"), "Answer in Japanese.");
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const config = JSON.parse(vi.mocked(updateRouteCredential).mock.calls[0][1].config_json);
    expect(config.turn_reminder).toBe(true);
    expect(config.turn_reminder_text).toBe("Answer in Japanese.");
  });

  it("hydrates the per-turn reminder and clears both keys when switched off", async () => {
    const api = {
      ...credentialsFixture[1],
      config_json: JSON.stringify({
        base_url: "https://api.example.com/v1",
        interface_format: "openai",
        model_mappings: [],
        turn_reminder: true,
        turn_reminder_text: "请用简体中文回复。",
      }),
    };
    vi.mocked(listRouteCredentials).mockResolvedValue([api]);
    vi.mocked(updateRouteCredential).mockResolvedValue(api);

    renderScreen();
    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    await openFormTab("高级");

    const checkbox = await screen.findByLabelText("每轮追加纠偏提醒");
    expect(checkbox).toBeChecked();
    expect(screen.getByLabelText("纠偏提醒内容")).toHaveValue("请用简体中文回复。");

    await userEvent.click(checkbox);
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const config = JSON.parse(vi.mocked(updateRouteCredential).mock.calls[0][1].config_json);
    // Removed, not written as false: an account that opted out carries no trace.
    expect(config).not.toHaveProperty("turn_reminder");
    expect(config).not.toHaveProperty("turn_reminder_text");
  });

  it("leaves the reminder out of an empty-text save so the default applies", async () => {
    const api = {
      ...credentialsFixture[1],
      config_json: JSON.stringify({
        base_url: "https://api.example.com/v1",
        interface_format: "openai",
        model_mappings: [],
      }),
    };
    vi.mocked(listRouteCredentials).mockResolvedValue([api]);
    vi.mocked(updateRouteCredential).mockResolvedValue(api);

    renderScreen();
    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    await openFormTab("高级");
    await userEvent.click(await screen.findByLabelText("每轮追加纠偏提醒"));
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const config = JSON.parse(vi.mocked(updateRouteCredential).mock.calls[0][1].config_json);
    expect(config.turn_reminder).toBe(true);
    // No text key at all — the proxy falls back to its own default.
    expect(config).not.toHaveProperty("turn_reminder_text");
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
    await openFormTab("高级");

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
    await openFormTab("其他");
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
    await openFormTab("故障处理");

    expect(screen.getByLabelText("额外重试次数")).toHaveValue(2);
    expect(screen.getByLabelText("重试间隔（毫秒）")).toHaveValue(200);
    expect(screen.getByLabelText("异常触发次数")).toHaveValue(10);
    expect(screen.getByLabelText("失败冷却（秒）")).toHaveValue(10);
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
    await openFormTab("故障处理");
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
    await openFormTab("故障处理");
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
    await openFormTab("其他");
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

  it("saves the relay balance provider chosen in the advanced tab", async () => {
    vi.mocked(updateRouteCredential).mockResolvedValue(credentialsFixture[1]);
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    await openFormTab("高级");
    await userEvent.click(screen.getByLabelText("编辑 余额查询 new-api"));
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const lastCall = vi.mocked(updateRouteCredential).mock.calls.at(-1);
    const config = JSON.parse(lastCall![1].config_json);
    expect(config.relay_balance).toEqual({ provider: "new_api" });
  });

  it("refuses a custom balance query with no request URL", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    await openFormTab("高级");
    await userEvent.click(screen.getByLabelText("编辑 余额查询 自定义"));
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    expect(
      (await screen.findAllByText(/自定义余额查询需要填写请求 URL/)).length,
    ).toBeGreaterThan(0);
    expect(updateRouteCredential).not.toHaveBeenCalled();
  });

  it("saves the declared paths of a custom balance query", async () => {
    vi.mocked(updateRouteCredential).mockResolvedValue(credentialsFixture[1]);
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 API Account" }));
    await openFormTab("高级");
    await userEvent.click(screen.getByLabelText("编辑 余额查询 自定义"));
    await userEvent.type(
      screen.getByLabelText("编辑 余额查询请求 URL"),
      "https://panel.example.com/api/billing",
    );
    await userEvent.type(
      screen.getByLabelText("编辑 余额查询剩余额度路径"),
      "data.left",
    );
    await userEvent.type(screen.getByLabelText("编辑 余额查询换算除数"), "1000");
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const lastCall = vi.mocked(updateRouteCredential).mock.calls.at(-1);
    expect(JSON.parse(lastCall![1].config_json).relay_balance).toEqual({
      provider: "custom",
      endpoint: "https://panel.example.com/api/billing",
      remaining_path: "data.left",
      divisor: 1000,
    });
  });

  it("shows the stored balance on the account row and re-queries it from the row action", async () => {
    const relayAccount = {
      ...credentialsFixture[1],
      config_json: JSON.stringify({
        base_url: "https://panel.example.com/v1",
        interface_format: "openai",
        model_mappings: [],
        relay_balance: { provider: "new_api" },
        relay_balance_snapshot: {
          provider: "new_api",
          remaining: 37.7,
          used: 12.3,
          limit: 50,
          unit: "USD",
          source_url: "https://panel.example.com/api/usage/token/",
          checked_at: "2026-09-02T12:00:00Z",
        },
      }),
    };
    vi.mocked(listRouteCredentials).mockImplementation(async () => [
      credentialsFixture[0],
      relayAccount,
    ]);
    vi.mocked(refreshRouteCredentialRelayBalance).mockResolvedValue({
      credential: relayAccount,
      updated: true,
      source: "new_api",
      message: null,
    });
    renderScreen();

    expect(
      await screen.findByTestId(`credential-relay-balance-${relayAccount.id}`),
    ).toHaveTextContent("余额 $37.70");

    await userEvent.click(screen.getByRole("button", { name: "查询 API Account 余额" }));
    await waitFor(() =>
      expect(refreshRouteCredentialRelayBalance).toHaveBeenCalledWith(relayAccount.id),
    );
  });

  it("leaves the balance action off accounts that have not enabled querying", async () => {
    renderScreen();

    expect(await screen.findByRole("button", { name: "编辑 API Account" })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "查询 API Account 余额" }),
    ).not.toBeInTheDocument();
  });

  it("saves per-account scheduled recovery settings", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 Team Account" }));
    await openFormTab("故障处理");
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

  it("saves the failure cooldown and error status toggles into the failure policy", async () => {
    vi.mocked(updateRouteCredential).mockResolvedValue(credentialsFixture[0]);
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 Team Account" }));
    await openFormTab("故障处理");

    const cooldownToggle = screen.getByLabelText("启用失败冷却");
    const errorStatusToggle = screen.getByLabelText("启用异常状态标记");
    expect(cooldownToggle).not.toBeChecked();
    expect(errorStatusToggle).toBeChecked();

    await userEvent.click(cooldownToggle);
    await userEvent.click(errorStatusToggle);
    fireEvent.change(screen.getByLabelText("失败冷却（秒）"), { target: { value: "25" } });
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const payload = vi.mocked(updateRouteCredential).mock.calls[0][1];
    const config = JSON.parse(payload.config_json);
    expect(config.failure_policy.cooldown_enabled).toBe(true);
    expect(config.failure_policy.cooldown_seconds).toBe(25);
    expect(config.failure_policy.error_status_enabled).toBe(false);
  });

  it("rejects a failure cooldown outside the supported range", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 Team Account" }));
    await openFormTab("故障处理");
    fireEvent.change(screen.getByLabelText("失败冷却（秒）"), { target: { value: "0" } });
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    expect(
      (await screen.findAllByText("失败冷却需在 1 到 86400 秒之间。")).length,
    ).toBeGreaterThan(0);
    expect(updateRouteCredential).not.toHaveBeenCalled();
  });

  it("shows the transient failure count as its own status tag until a request succeeds", async () => {
    vi.mocked(listRouteCredentials).mockResolvedValue([
      { ...credentialsFixture[0], transient_failure_count: 3 },
      { ...credentialsFixture[1], transient_failure_count: 0 },
    ]);

    renderScreen();

    const failingRow = await screen.findByLabelText("放置在 Team Account 前");
    expect(within(failingRow).getByText("错误 3 次")).toBeInTheDocument();
    expect(within(failingRow).queryByText("正常")).not.toBeInTheDocument();

    const healthyRow = screen.getByLabelText("放置在 API Account 前");
    expect(within(healthyRow).getByText("正常")).toBeInTheDocument();
    expect(within(healthyRow).queryByText(/错误 \d+ 次/)).not.toBeInTheDocument();
  });

  it("keeps a revoked account's status tag even while transient failures are counted", async () => {
    vi.mocked(listRouteCredentials).mockResolvedValue([
      { ...credentialsFixture[0], status: "revoked", transient_failure_count: 2 },
    ]);

    renderScreen();

    const row = await screen.findByLabelText("放置在 Team Account 前");
    expect(within(row).getByText("已失效")).toBeInTheDocument();
    expect(within(row).queryByText("错误 2 次")).not.toBeInTheDocument();
  });

  it("saves official account User-Agent into config headers", async () => {
    vi.mocked(updateRouteCredential).mockResolvedValue(credentialsFixture[0]);
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "编辑 Team Account" }));
    await openFormTab("高级");
    await userEvent.selectOptions(screen.getByLabelText("编辑 User-Agent 预设"), "browser");
    await userEvent.click(screen.getByRole("button", { name: "保存修改" }));

    await waitFor(() => expect(updateRouteCredential).toHaveBeenCalled());
    const payload = vi.mocked(updateRouteCredential).mock.calls[0][1];
    const config = JSON.parse(payload.config_json);
    expect(config.headers["User-Agent"]).toContain("Mozilla/5.0");
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
    expect(screen.queryByText("用量总览")).not.toBeInTheDocument();

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
        // Plain `curl` in Bash; on Windows shells it is an Invoke-WebRequest alias.
        expect.stringContaining("curl 'https://127.0.0.1:43111/responses'"),
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

    // PowerShell 5.1 strips quotes out of native-command arguments, so neither of
    // the two forms above survives it — the body has to reach curl over stdin.
    await userEvent.click(screen.getByLabelText("打开算力池测试菜单"));
    await userEvent.click(screen.getByLabelText("复制 PowerShell curl 执行语句"));
    await waitFor(() => {
      const command = vi.mocked(navigator.clipboard.writeText).mock.lastCall?.[0] ?? "";
      expect(command).toContain("$body = '{\"model\"");
      expect(command).toContain("$body | curl.exe 'https://127.0.0.1:43111/responses'");
      expect(command).toContain("--data-binary '@-'");
      // The 5.1 default pipe encoding turns non-ASCII bodies into `?`.
      expect(command).toContain("$OutputEncoding = [System.Text.UTF8Encoding]::new($false);");
      // --data-raw would put the JSON back into argument parsing, undoing the fix.
      expect(command).not.toContain("--data-raw");
    });
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

  it("nudges to re-write config when pending edits would change the file", async () => {
    // Config is only written on demand, so a mapping or global-client-config
    // edit sits unapplied until the user writes again.
    vi.mocked(routeConfigWriteIsStale).mockResolvedValue(true);
    renderScreen();

    await screen.findByText("本地代理：未启动");
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    expect(await screen.findByText("配置已变更，需重新写入")).toBeInTheDocument();
    expect(screen.getByLabelText("写入路由配置文件")).toHaveAttribute(
      "title",
      "配置已变更，需重新写入才会生效",
    );

    // Writing clears the nudge.
    vi.mocked(routeConfigWriteIsStale).mockResolvedValue(false);
    await userEvent.click(screen.getByLabelText("写入路由配置文件"));
    // The button now only opens the dialog; the write happens on confirm.
    await screen.findByText("选择要写入的客户端");
    await userEvent.click(screen.getByRole("button", { name: "写入" }));
    await waitFor(() =>
      expect(screen.queryByText("配置已变更，需重新写入")).not.toBeInTheDocument(),
    );
  });

  it("clears route config write results after a short delay", async () => {
    renderScreen();

    await screen.findByText("本地代理：未启动");
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("写入路由配置文件"));
    await screen.findByText("选择要写入的客户端");

    vi.useFakeTimers();
    fireEvent.click(screen.getByRole("button", { name: "写入" }));
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

  it("opens the client dialog instead of writing immediately", async () => {
    renderScreen();

    await screen.findByText("本地代理：未启动");
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("写入路由配置文件"));

    expect(await screen.findByText("选择要写入的客户端")).toBeInTheDocument();
    // The dialog is the confirmation step, so nothing is written yet.
    expect(writeRouteProxyConfigs).not.toHaveBeenCalled();
  });

  it("writes the selected clients and persists the choice", async () => {
    renderScreen();

    await screen.findByText("本地代理：未启动");
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("写入路由配置文件"));
    await screen.findByText("选择要写入的客户端");
    await userEvent.click(await screen.findByRole("checkbox", { name: /ZCode/ }));
    await userEvent.click(screen.getByRole("button", { name: "写入" }));

    await waitFor(() =>
      expect(writeRouteProxyConfigs).toHaveBeenCalledWith("http://127.0.0.1:43111", "codex", [
        "codex",
        "zcode",
      ]),
    );
    // The choice is remembered so the next write does not need re-picking.
    await waitFor(() =>
      expect(saveSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          config_write_clients_json: JSON.stringify({ codex: ["codex", "zcode"] }),
        }),
      ),
    );
  });

  it("labels write results by client name rather than target key", async () => {
    vi.mocked(writeRouteProxyConfigs).mockResolvedValue([
      {
        operation_id: "operation-1",
        snapshot_id: "snapshot-1",
        target_app_id: "target-zcode",
        target_key: "zcode_codex",
        platform: "codex",
        path: "/home/u/.zcode/v2/config.json",
        status: "succeeded",
        before_hash: null,
        after_hash: "after-hash",
        error_code: null,
      },
    ]);
    renderScreen();

    await screen.findByText("本地代理：未启动");
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("写入路由配置文件"));
    await screen.findByText("选择要写入的客户端");
    await userEvent.click(screen.getByRole("button", { name: "写入" }));

    expect(await screen.findByText("配置写入结果")).toBeInTheDocument();
    // `zcode_codex` is an internal key and means nothing to the user.
    expect(screen.queryByText(/zcode_codex/)).not.toBeInTheDocument();
    expect(screen.getByText(/ZCode · codex/)).toBeInTheDocument();
    // The user very likely switched windows already, so repeat the notice here.
    expect(screen.getByText(/需重启 ZCode/)).toBeInTheDocument();
    // Same guarantee the existing outcome test makes: no credential in the panel.
    expect(screen.queryByText(/sk-ai-switch/)).not.toBeInTheDocument();
  });

  it("flags a client that failed while another one succeeded", async () => {
    // The backend resolves a group as long as one client got written, so a
    // partial failure never reaches onError. Without a banner the only trace is
    // the result panel, which used to clear itself after three seconds.
    vi.mocked(writeRouteProxyConfigs).mockResolvedValue([
      {
        operation_id: "operation-1",
        snapshot_id: "snapshot-1",
        target_app_id: "target-codex",
        target_key: "codex",
        platform: "codex",
        path: "/home/u/.codex/config.toml",
        status: "succeeded",
        before_hash: null,
        after_hash: "after-hash",
        error_code: null,
      },
      {
        operation_id: "operation-2",
        snapshot_id: null,
        target_app_id: "target-zcode",
        target_key: "zcode_codex",
        platform: "codex",
        path: "/home/u/.zcode/v2/config.json",
        status: "failed",
        before_hash: null,
        after_hash: null,
        error_code: "config.pool_models_empty",
      },
    ]);
    renderScreen();

    await screen.findByText("本地代理：未启动");
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("写入路由配置文件"));
    await screen.findByText("选择要写入的客户端");
    await userEvent.click(screen.getByRole("button", { name: "写入" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("没有写入成功");
    // Named by client, with the code to look up, not by internal target key.
    expect(alert).toHaveTextContent("ZCode");
    expect(alert).toHaveTextContent("config.pool_models_empty");
  });

  it("explains that a corrupt ZCode config was refused rather than overwritten", async () => {
    vi.mocked(writeRouteProxyConfigs).mockRejectedValue({
      code: "validation.route_config_existing_invalid",
      message: "Existing CLI configuration is invalid; refusing to overwrite it",
      details: "/home/u/.zcode/v2/config.json (JSON): syntax is invalid",
      recoverable: true,
    });
    renderScreen();

    await screen.findByText("本地代理：未启动");
    await userEvent.click(screen.getByLabelText("启动本地路由代理"));
    expect(await screen.findByText("本地代理：http://127.0.0.1:43111")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("写入路由配置文件"));
    await screen.findByText("选择要写入的客户端");
    await userEvent.click(screen.getByRole("button", { name: "写入" }));

    // The stakes are specific here: a bad parse makes ZCode fall back and lose
    // every provider, so the message has to say we did not touch the file.
    expect(
      await screen.findByText(/现有配置文件无法解析，已拒绝覆盖以免丢失你的 provider 配置/),
    ).toBeInTheDocument();
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

  // Per-model cooldown: the row badge, its hover detail and the drawer section.
  // Built per call rather than once: the cooling deadline is relative to now, and
  // this suite is long enough that a shared constant could expire mid-run.
  function modelStatesFixture(): RouteCredentialModelState[] {
    return [
      {
        route_credential_id: "cred-api-1",
        model_key: "upstream-sol",
        aliases: ["gpt-5.6-sol"],
        status: "ok",
        transient_failure_count: 2,
        cooldown_until: new Date(Date.now() + 45_000).toISOString(),
        semantic_failure_streak_count: 0,
        last_failure_kind: "upstream_status",
        last_failure_message: "upstream returned 429",
        last_failure_response_json: null,
        created_at: "2026-09-02T00:00:00Z",
        updated_at: "2026-09-02T00:00:00Z",
      },
      {
        route_credential_id: "cred-api-1",
        model_key: "upstream-glm",
        aliases: ["glm-5.3"],
        status: "ok",
        transient_failure_count: 0,
        cooldown_until: null,
        semantic_failure_streak_count: 0,
        last_failure_kind: null,
        last_failure_message: null,
        last_failure_response_json: null,
        created_at: "2026-09-02T00:00:00Z",
        updated_at: "2026-09-02T00:00:00Z",
      },
      {
        route_credential_id: "cred-api-1",
        model_key: "upstream-held",
        aliases: ["held-model"],
        status: "paused",
        transient_failure_count: 0,
        cooldown_until: null,
        semantic_failure_streak_count: 0,
        last_failure_kind: null,
        last_failure_message: null,
        last_failure_response_json: null,
        created_at: "2026-09-02T00:00:00Z",
        updated_at: "2026-09-02T00:00:00Z",
      },
    ];
  }

  function credentialsWithModelStates(
    states: RouteCredentialModelState[] = modelStatesFixture(),
  ): RouteCredential[] {
    return credentialsFixture.map((credential) =>
      credential.id === "cred-api-1" ? { ...credential, model_states: states } : credential,
    );
  }

  // The pool segment is empty by default, so seed membership before asserting on
  // a row rendered under 算力池.
  function renderPoolWithModelStates(credentials = credentialsWithModelStates()) {
    vi.mocked(listRouteCredentials).mockResolvedValue(credentials);
    poolStateByPlatform.set("codex", ["cred-api-1"]);
    return renderScreen("codex", "in_pool");
  }

  it("在账号行显示不可用模型的汇总徽章", async () => {
    renderPoolWithModelStates();

    const badge = await screen.findByTestId("credential-model-issues-cred-api-1");
    // One cooling model plus one paused model; the healthy one is not counted.
    expect(badge).toHaveTextContent("模型 2 不可用");
  });

  it("不为全部模型健康的账号显示模型徽章", async () => {
    renderPoolWithModelStates(credentialsWithModelStates([modelStatesFixture()[1]]));

    expect(await screen.findByText("API Account")).toBeInTheDocument();
    expect(screen.queryByTestId("credential-model-issues-cred-api-1")).toBeNull();
  });

  it("悬停徽章时展示逐模型明细", async () => {
    renderPoolWithModelStates();

    const detail = await screen.findByTestId("credential-model-detail-cred-api-1");
    expect(detail).toHaveTextContent("upstream-sol");
    // Aliases matter: the user configured "gpt-5.6-sol", not the upstream name.
    expect(detail).toHaveTextContent("gpt-5.6-sol");
    expect(detail).toHaveTextContent("upstream-held");
    expect(detail).toHaveTextContent("已暂停");
    expect(detail).not.toHaveTextContent("upstream-glm");
  });

  it("在编辑抽屉里列出全部已知模型并可暂停", async () => {
    vi.mocked(setRouteCredentialModelStatus).mockResolvedValue(credentialsWithModelStates()[1]);
    renderPoolWithModelStates();
    await userEvent.click(await screen.findByLabelText("编辑 API Account"));
    await openFormTab("故障处理");

    const section = await screen.findByLabelText("模型状态");
    // Healthy models are listed too, so a model can be paused before it fails.
    expect(section).toHaveTextContent("upstream-glm");

    await userEvent.click(screen.getByLabelText("暂停模型 upstream-glm"));
    expect(setRouteCredentialModelStatus).toHaveBeenCalledWith(
      "cred-api-1",
      "upstream-glm",
      "paused",
    );
  });

  it("可恢复已暂停的模型", async () => {
    vi.mocked(setRouteCredentialModelStatus).mockResolvedValue(credentialsWithModelStates()[1]);
    renderPoolWithModelStates();
    await userEvent.click(await screen.findByLabelText("编辑 API Account"));
    await openFormTab("故障处理");

    await userEvent.click(await screen.findByLabelText("恢复模型 upstream-held"));
    expect(setRouteCredentialModelStatus).toHaveBeenCalledWith(
      "cred-api-1",
      "upstream-held",
      "ok",
    );
  });

  it("暂停模型不会丢弃抽屉里未保存的修改", async () => {
    // The mutation hands back a fresh account row so the model list can update.
    // Re-hydrating the whole form from it would silently throw away whatever the
    // user has typed but not saved.
    vi.mocked(setRouteCredentialModelStatus).mockResolvedValue(credentialsWithModelStates()[1]);
    renderPoolWithModelStates();
    await userEvent.click(await screen.findByLabelText("编辑 API Account"));

    await userEvent.clear(screen.getByLabelText("编辑账号名称"));
    await userEvent.type(screen.getByLabelText("编辑账号名称"), "Renamed But Unsaved");

    await openFormTab("故障处理");
    await userEvent.click(await screen.findByLabelText("暂停模型 upstream-glm"));
    await waitFor(() => expect(setRouteCredentialModelStatus).toHaveBeenCalled());

    await openFormTab("基础");
    expect(screen.getByLabelText("编辑账号名称")).toHaveValue("Renamed But Unsaved");
  });

  it("可解除单个模型的冷却", async () => {
    vi.mocked(clearRouteCredentialModelState).mockResolvedValue(credentialsWithModelStates()[1]);
    renderPoolWithModelStates();
    await userEvent.click(await screen.findByLabelText("编辑 API Account"));
    await openFormTab("故障处理");

    await userEvent.click(await screen.findByLabelText("解除模型 upstream-sol"));
    expect(clearRouteCredentialModelState).toHaveBeenCalledWith("cred-api-1", "upstream-sol");
  });

  it("一次解除全部非暂停模型", async () => {
    vi.mocked(clearRouteCredentialModelState).mockResolvedValue(credentialsWithModelStates()[1]);
    renderPoolWithModelStates();
    await userEvent.click(await screen.findByLabelText("编辑 API Account"));
    await openFormTab("故障处理");

    await userEvent.click(await screen.findByLabelText("解除全部模型冷却"));
    // Only the cooling model: a paused one is the user's own decision, and a
    // healthy model has no state to clear.
    expect(clearRouteCredentialModelState).toHaveBeenCalledTimes(1);
    expect(clearRouteCredentialModelState).toHaveBeenCalledWith("cred-api-1", "upstream-sol");
  });

  it("已失效账号不显示模型徽章", async () => {
    renderPoolWithModelStates(
      credentialsFixture.map((credential) =>
        credential.id === "cred-api-1"
          ? { ...credential, status: "revoked" as const, model_states: modelStatesFixture() }
          : credential,
      ),
    );

    // The account itself is dead; per-model detail would only add noise.
    expect(await screen.findByText("API Account")).toBeInTheDocument();
    expect(screen.queryByTestId("credential-model-issues-cred-api-1")).toBeNull();
  });
});
