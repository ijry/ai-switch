import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getModelPriceConfigs, getRouteProxyKey, getRouteProxyStatus, getUsageOverview, saveModelPriceConfigs } from "../src/lib/api/client";
import { createQueryClient } from "../src/lib/query/queryClient";
import { UsageOverviewPanel } from "../src/components/accounts/UsageOverviewPanel";
import type { UsageOverview, UsageOverviewRow } from "../src/lib/api/types";

vi.mock("../src/lib/api/client", () => ({
  getModelPriceConfigs: vi.fn(),
  getRouteProxyKey: vi.fn(),
  getRouteProxyStatus: vi.fn(),
  getUsageOverview: vi.fn(),
  saveModelPriceConfigs: vi.fn(),
}));

function rowFixture(overrides: Partial<UsageOverviewRow> = {}): UsageOverviewRow {
  return {
    id: "row-1",
    source: "matched",
    occurred_at: "2026-08-19T14:04:50Z",
    provider: "claude",
    model: "claude-opus-5",
    account_id: "cred-1",
    account_name: "Team Account",
    source_label: "route_proxy",
    path: "/v1/messages",
    status: "200",
    success: true,
    input_tokens: 120,
    output_tokens: 30,
    cache_write_tokens: 10,
    cache_read_tokens: 40,
    cost_micros: 4_200,
    price_source: "upstream",
    upstream_response_id: "msg_a",
    metadata_json: null,
    ...overrides,
  };
}

function groupRow(key: string, cost: number) {
  return {
    key,
    request_count: 152,
    input_tokens: 1_000_000,
    output_tokens: 2_000,
    cache_write_tokens: 0,
    cache_read_tokens: 0,
    cost_micros: cost,
  };
}

function overviewFixture(overrides: Partial<UsageOverview> = {}): UsageOverview {
  return {
    totals: {
      request_count: 11_254,
      input_tokens: 5_584_802_591,
      output_tokens: 129_897_022,
      cache_write_tokens: 318_626_507,
      cache_read_tokens: 19_115_772_272,
      cost_micros: 16_248_905_925,
    },
    rows: [rowFixture()],
    groups: {
      // Deliberately a different model id from rowFixture's `claude-opus-5`:
      // the collapse test asserts this text is absent before the user clicks,
      // which only means something if the row list cannot supply it.
      by_model: [groupRow("claude-haiku-4-5", 3_357_030_000)],
      by_platform: [groupRow("claude", 3_357_030_000)],
      by_account: [groupRow("未经代理", 1_000_000)],
      by_source: [groupRow("匹配", 3_357_030_000)],
    },
    row_count: 1,
    page: 1,
    page_size: 20,
    integrity: {
      scanned_file_count: 1_186,
      truncated: false,
      unpriced_request_count: 3,
      estimated_price_request_count: 12,
      unmatchable_proxy_row_count: 5,
    },
    ...overrides,
  };
}

function renderPanel() {
  return render(
    <QueryClientProvider client={createQueryClient()}>
      <UsageOverviewPanel />
    </QueryClientProvider>,
  );
}

describe("UsageOverviewPanel", () => {
  beforeEach(() => {
    vi.mocked(getUsageOverview).mockReset();
    vi.mocked(getUsageOverview).mockResolvedValue(overviewFixture());
    vi.mocked(getModelPriceConfigs).mockResolvedValue({});
    vi.mocked(saveModelPriceConfigs).mockReset();
    vi.mocked(saveModelPriceConfigs).mockResolvedValue(1);
    vi.mocked(getRouteProxyStatus).mockResolvedValue({ running: false, bind_host: "127.0.0.1" });
  });

  it("renders one set of totals with 万/百万/亿 units and the exact figure in a tooltip", async () => {
    renderPanel();

    // 11,254 requests: over the 万 threshold, so 1.1万 with the exact count on
    // hover. The point of the test is that there is ONE set of numbers now.
    expect(await screen.findByText("1.1万")).toBeInTheDocument();
    expect(screen.getByTitle("11,254")).toBeInTheDocument();
    // 5,584,802,591 input tokens.
    expect(screen.getByText("55.85亿")).toBeInTheDocument();
    expect(screen.getByTitle("5,584,802,591")).toBeInTheDocument();
    // Cost keeps a currency format rather than a 万/亿 unit.
    expect(screen.getByText("$16,248.91")).toBeInTheDocument();
  });

  it("opens the model price configuration from the settings icon", async () => {
    renderPanel();
    await screen.findByText("1.1万");
    await userEvent.click(screen.getByRole("button", { name: "配置模型价格" }));
    expect(await screen.findByRole("dialog", { name: "模型价格配置" })).toBeInTheDocument();
    expect(screen.getByText("配置各模型 Token 成本")).toBeInTheDocument();
  });

  it("keeps the grouping table collapsed until a dimension is clicked", async () => {
    renderPanel();

    await screen.findByText("1.1万");
    // The group row's model id differs from the list row's, so its absence here
    // proves the group table is genuinely not rendered rather than merely
    // duplicating text the list already shows.
    expect(screen.queryByText("claude-haiku-4-5")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "模型" }));

    expect(await screen.findByText("claude-haiku-4-5")).toBeInTheDocument();
  });

  it("collapses again when the active dimension is clicked a second time", async () => {
    renderPanel();
    await screen.findByText("1.1万");

    await userEvent.click(screen.getByRole("button", { name: "模型" }));
    expect(await screen.findByText("claude-haiku-4-5")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "模型" }));

    expect(screen.queryByText("claude-haiku-4-5")).not.toBeInTheDocument();
  });

  it("switches the grouping dimension without refetching", async () => {
    renderPanel();
    await screen.findByText("1.1万");
    const callsBefore = vi.mocked(getUsageOverview).mock.calls.length;

    await userEvent.click(screen.getByRole("button", { name: "模型" }));
    await userEvent.click(screen.getByRole("button", { name: "账号" }));

    expect(await screen.findByText("未经代理")).toBeInTheDocument();
    // All four dimensions arrive in one response, so flipping the control is
    // free — a refetch here would make the segmented control feel laggy.
    expect(vi.mocked(getUsageOverview).mock.calls.length).toBe(callsBefore);
  });

  it("labels each row with its source", async () => {
    vi.mocked(getUsageOverview).mockResolvedValue(
      overviewFixture({
        rows: [
          rowFixture({ id: "a", source: "matched" }),
          rowFixture({ id: "b", source: "session_only", account_name: null }),
          rowFixture({ id: "c", source: "proxy_only" }),
        ],
        row_count: 3,
      }),
    );

    renderPanel();

    expect(await screen.findByText("匹配")).toBeInTheDocument();
    expect(screen.getByText("仅会话")).toBeInTheDocument();
    expect(screen.getByText("仅代理")).toBeInTheDocument();
  });

  it("shows a status chip only on a failed row", async () => {
    vi.mocked(getUsageOverview).mockResolvedValue(
      overviewFixture({
        rows: [
          rowFixture({ id: "ok", status: "200", success: true }),
          rowFixture({ id: "bad", status: "401", success: false }),
        ],
        row_count: 2,
      }),
    );

    renderPanel();

    // A transcript row has no HTTP status at all, so a permanent status column
    // would be mostly blank; only failures earn a chip.
    expect(await screen.findByText("401")).toBeInTheDocument();
    expect(screen.queryByText("200")).not.toBeInTheDocument();
  });

  it("marks rows that never went through the proxy", async () => {
    vi.mocked(getUsageOverview).mockResolvedValue(
      overviewFixture({
        rows: [rowFixture({ source: "session_only", account_name: null, account_id: null })],
        // Drop the group fixture so 未经代理 can only come from the row.
        groups: { by_model: [], by_platform: [], by_account: [], by_source: [] },
      }),
    );

    renderPanel();

    expect(await screen.findByText("未经代理")).toBeInTheDocument();
  });

  it("states how complete the totals are", async () => {
    renderPanel();

    // The semantics are "my total spend", so anything that makes the figure a
    // floor rather than an exact number has to be said out loud.
    expect(await screen.findByText(/已扫描 1,186 个会话文件/)).toBeInTheDocument();
    expect(screen.getByText(/3 个请求的模型没有价格数据/)).toBeInTheDocument();
    expect(screen.getByText(/5 条代理记录无法与会话记录匹配/)).toBeInTheDocument();
  });

  it("requests the selected period", async () => {
    renderPanel();
    await screen.findByText("1.1万");

    await userEvent.click(screen.getByRole("button", { name: "累计" }));

    await waitFor(() => expect(getUsageOverview).toHaveBeenLastCalledWith(null, 1, 20));
  });

  it("pages within the panel", async () => {
    vi.mocked(getUsageOverview).mockResolvedValue(
      overviewFixture({ row_count: 42, page: 1, page_size: 20 }),
    );

    renderPanel();
    expect(await screen.findByText("1/3")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "下一页" }));

    await waitFor(() =>
      expect(getUsageOverview).toHaveBeenLastCalledWith(expect.any(String), 2, 20),
    );
  });

  it("expands one request into a detail panel and collapses it again", async () => {
    vi.mocked(getUsageOverview).mockResolvedValue(
      overviewFixture({
        rows: [
          rowFixture({
            metadata_json: JSON.stringify({
              path: "/v1/messages",
              target_url: "https://api.anthropic.com/v1/messages",
              status: 200,
              success: true,
              duration_ms: 1842,
              trace_id: "trace-1",
              requested_model: "claude-opus-5",
              upstream_model: "opus-upstream",
              response_body: '{"id":"msg_a"}',
            }),
          }),
        ],
      }),
    );

    renderPanel();
    const toggle = await screen.findByRole("button", { name: "查看请求 row-1 详情" });
    expect(screen.queryByLabelText("请求 row-1 详情")).not.toBeInTheDocument();

    await userEvent.click(toggle);

    const detail = await screen.findByLabelText("请求 row-1 详情");
    expect(within(detail).getByText("请求详情")).toBeInTheDocument();
    expect(within(detail).getByText("Team Account")).toBeInTheDocument();
    expect(within(detail).getByText("cred-1")).toBeInTheDocument();
    // Exact token counts: the row itself only shows a compacted total.
    expect(within(detail).getByText("120")).toBeInTheDocument();
    expect(within(detail).getByText("30")).toBeInTheDocument();
    expect(within(detail).getByText("$0.004200（上游价格）")).toBeInTheDocument();
    expect(within(detail).getByText("1842 ms")).toBeInTheDocument();
    expect(within(detail).getByText("claude-opus-5 → opus-upstream")).toBeInTheDocument();
    expect(within(detail).getByText(/"trace_id": "trace-1"/)).toBeInTheDocument();
    expect(within(detail).getByText(/"id": "msg_a"/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "隐藏请求 row-1 详情" }));

    expect(screen.queryByLabelText("请求 row-1 详情")).not.toBeInTheDocument();
  });

  it("shows the raw metadata when it cannot be parsed", async () => {
    vi.mocked(getUsageOverview).mockResolvedValue(
      overviewFixture({ rows: [rowFixture({ metadata_json: "{bad json" })] }),
    );

    renderPanel();
    await userEvent.click(await screen.findByRole("button", { name: "查看请求 row-1 详情" }));

    const detail = await screen.findByLabelText("请求 row-1 详情");
    expect(
      within(detail).getByText("metadata_json 无法解析，显示原始内容。"),
    ).toBeInTheDocument();
    expect(within(detail).getByText("{bad json")).toBeInTheDocument();
  });

  it("says why a transcript-only request has no proxy metadata", async () => {
    vi.mocked(getUsageOverview).mockResolvedValue(
      overviewFixture({
        rows: [
          rowFixture({
            source: "session_only",
            account_id: null,
            account_name: null,
            source_label: null,
            path: null,
            status: null,
            metadata_json: null,
          }),
        ],
      }),
    );

    renderPanel();
    await userEvent.click(await screen.findByRole("button", { name: "查看请求 row-1 详情" }));

    const detail = await screen.findByLabelText("请求 row-1 详情");
    // An empty panel would read as a load failure rather than as an absence.
    expect(within(detail).getByText(/没有经过本代理/)).toBeInTheDocument();
    expect(within(detail).getByText("成功")).toBeInTheDocument();
  });

  it("keeps only one detail panel open at a time", async () => {
    vi.mocked(getUsageOverview).mockResolvedValue(
      overviewFixture({
        rows: [rowFixture({ id: "a" }), rowFixture({ id: "b" })],
        row_count: 2,
      }),
    );

    renderPanel();
    await userEvent.click(await screen.findByRole("button", { name: "查看请求 a 详情" }));
    await userEvent.click(screen.getByRole("button", { name: "查看请求 b 详情" }));

    expect(await screen.findByLabelText("请求 b 详情")).toBeInTheDocument();
    expect(screen.queryByLabelText("请求 a 详情")).not.toBeInTheDocument();
  });

  it("reports a failure instead of rendering zeros", async () => {
    // Zeros would read as "you spent nothing", which is a different and wrong
    // statement from "the figure could not be loaded".
    vi.mocked(getUsageOverview).mockRejectedValue(new Error("scan failed"));

    renderPanel();

    // The shared query client retries once before surfacing an error, so this
    // waits past that backoff rather than the default 1s.
    expect(await screen.findByRole("alert", {}, { timeout: 5_000 })).toHaveTextContent(
      /scan failed|读取用量失败/,
    );
  });

  it("saves complete model prices and derives omitted cache prices", async () => {
    const user = userEvent.setup();
    const { ModelPricingDialog } = await import("../src/components/accounts/ModelPricingDialog");
    render(
      <QueryClientProvider client={createQueryClient()}>
        <ModelPricingDialog open onClose={vi.fn()} />
      </QueryClientProvider>,
    );

    await screen.findByText("暂无模型。请启动本地路由代理或手动添加模型。");
    await user.type(screen.getByRole("textbox", { name: "手动添加模型" }), "custom-model");
    await user.click(screen.getByRole("button", { name: "添加" }));
    await user.type(screen.getByRole("spinbutton", { name: "custom-model input_per_mtok" }), "2");
    await user.type(screen.getByRole("spinbutton", { name: "custom-model output_per_mtok" }), "8");
    await user.click(screen.getByRole("button", { name: "保存配置" }));

    await waitFor(() => expect(saveModelPriceConfigs).toHaveBeenCalledWith({
      "custom-model": {
        display_name: "",
        input_per_mtok: 2,
        output_per_mtok: 8,
        cache_read_per_mtok: 0.2,
        cache_write_per_mtok: 2.5,
      },
    }));
  });
});
