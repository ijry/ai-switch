import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { BarChart3, ChevronLeft, ChevronRight, List, Settings2 } from "lucide-react";
import { useState } from "react";
import { ModelPricingDialog } from "./ModelPricingDialog";
import { MotionNumber } from "../motion/MotionPrimitives";
import { UsageTrendChart } from "./UsageTrendChart";
import { getUsageOverview } from "../../lib/api/client";
import { formatCompactCount, formatCostMicros, formatExactCount } from "../../lib/usageFormat";
import { parseUsageMetadata, prettyJsonOrText } from "../../lib/usageMetadata";
import type {
  UsageOverviewGroupRow,
  UsageOverviewRow,
  UsageRowSource,
} from "../../lib/api/types";

const periods = [
  { key: "today", label: "当日" },
  { key: "24h", label: "24h" },
  { key: "week", label: "本周" },
  { key: "7d", label: "7d" },
  { key: "month", label: "本月" },
  { key: "30d", label: "30d" },
  { key: "all", label: "累计" },
] as const;

type Period = (typeof periods)[number]["key"];

const groupDimensions = [
  { key: "model", label: "模型", header: "模型", field: "by_model" },
  { key: "platform", label: "平台", header: "平台", field: "by_platform" },
  { key: "account", label: "账号", header: "账号", field: "by_account" },
  { key: "source", label: "来源", header: "来源", field: "by_source" },
] as const;

type GroupDimension = (typeof groupDimensions)[number]["key"];

/** The grouped numbers as a table, or the same numbers over time as bars. */
const groupViews = [
  { key: "list", label: "列表", Icon: List },
  { key: "chart", label: "图表", Icon: BarChart3 },
] as const;

type GroupView = (typeof groupViews)[number]["key"];

const pageSize = 20;

/**
 * The transcript scan is the expensive half of this query. A warm scan of a
 * ~1400-file corpus measured 1.1s, so a 10s interval would keep a blocking
 * thread busy over a tenth of the time for numbers that only change when a CLI
 * writes a turn.
 */
const refreshMs = 30_000;

/** Rows shown in a grouping table before the rest are summarized away. */
const groupRowLimit = 12;

function periodSince(period: Period, now = new Date()) {
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  switch (period) {
    case "today": {
      return today.toISOString();
    }
    case "24h": {
      const ms = now.getTime() - 24 * 60 * 60 * 1000;
      return new Date(ms).toISOString();
    }
    case "week": {
      const day = now.getDay();
      const daysFromMonday = day === 0 ? 6 : day - 1;
      const monday = new Date(today);
      monday.setDate(today.getDate() - daysFromMonday);
      return monday.toISOString();
    }
    case "7d": {
      const ms = now.getTime() - 7 * 24 * 60 * 60 * 1000;
      return new Date(ms).toISOString();
    }
    case "month": {
      const firstOfMonth = new Date(now.getFullYear(), now.getMonth(), 1);
      return firstOfMonth.toISOString();
    }
    case "30d": {
      const ms = now.getTime() - 30 * 24 * 60 * 60 * 1000;
      return new Date(ms).toISOString();
    }
    case "all": {
      return null;
    }
  }
}

function formatTime(value: string | null | undefined) {
  if (!value) {
    return "-";
  }
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

const sourceLabels: Record<UsageRowSource, string> = {
  matched: "匹配",
  session_only: "仅会话",
  proxy_only: "仅代理",
};

const sourceStyles: Record<UsageRowSource, string> = {
  matched: "bg-emerald-50 text-emerald-800 ring-1 ring-emerald-200",
  session_only: "bg-blue-50 text-blue-800 ring-1 ring-blue-200",
  proxy_only: "bg-amber-50 text-amber-800 ring-1 ring-amber-200",
};

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (error && typeof error === "object") {
    const record = error as { message?: unknown; details?: unknown };
    for (const field of [record.message, record.details]) {
      if (typeof field === "string" && field.trim()) {
        return field.trim();
      }
    }
  }
  return "读取用量失败";
}

function SummaryCard({
  label,
  value,
  title,
}: {
  label: string;
  value: string;
  title: string;
}) {
  return (
    <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
      <p className="text-[11px] font-medium text-stone-500">{label}</p>
      <p className="mt-1 text-lg font-semibold text-stone-950" title={title}>
        <MotionNumber value={value} />
      </p>
    </div>
  );
}

function GroupTable({
  header,
  rows,
}: {
  header: string;
  rows: UsageOverviewGroupRow[];
}) {
  const shown = rows.slice(0, groupRowLimit);
  const hidden = rows.length - shown.length;
  return (
    <div className="overflow-hidden rounded-xl border border-stone-200">
      <div className="grid grid-cols-[1.6fr_0.6fr_0.8fr_0.8fr_0.8fr] gap-2 border-b border-stone-100 bg-stone-50 px-3 py-2 text-[11px] font-medium text-stone-500">
        <span>{header}</span>
        <span className="text-right">请求</span>
        <span className="text-right">输入</span>
        <span className="text-right">输出</span>
        <span className="text-right">费用</span>
      </div>
      {shown.length === 0 ? (
        <p className="px-3 py-3 text-[12px] text-stone-500">当前筛选范围内没有数据。</p>
      ) : (
        <div className="divide-y divide-stone-100">
          {shown.map((row) => (
            <div
              className="grid grid-cols-[1.6fr_0.6fr_0.8fr_0.8fr_0.8fr] gap-2 px-3 py-2 text-[12px]"
              key={row.key}
            >
              <span className="truncate text-stone-800" title={row.key}>
                {row.key}
              </span>
              <span className="text-right text-stone-600" title={formatExactCount(row.request_count)}>
                {formatCompactCount(row.request_count)}
              </span>
              <span className="text-right text-stone-600" title={formatExactCount(row.input_tokens)}>
                {formatCompactCount(row.input_tokens)}
              </span>
              <span className="text-right text-stone-600" title={formatExactCount(row.output_tokens)}>
                {formatCompactCount(row.output_tokens)}
              </span>
              <span className="text-right text-stone-800">{formatCostMicros(row.cost_micros)}</span>
            </div>
          ))}
        </div>
      )}
      {hidden > 0 ? (
        <p className="border-t border-stone-100 px-3 py-2 text-[11px] text-stone-500">
          另有 {hidden} 项未显示。
        </p>
      ) : null}
    </div>
  );
}

function detailId(rowId: string) {
  return `usage-request-detail-${rowId}`;
}

/** A transcript row has no HTTP status, so it reports the outcome in words. */
function statusText(row: UsageOverviewRow) {
  const outcome = row.success ? "成功" : "失败";
  return row.status ? `${row.status} · ${outcome}` : outcome;
}

function costText(row: UsageOverviewRow) {
  if (!row.price_source) {
    return "无价格";
  }
  const origin = row.price_source === "upstream" ? "上游价格" : "本地价格表估算";
  return `${formatCostMicros(row.cost_micros)}（${origin}）`;
}

function DetailField({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div>
      <p className="text-[11px] font-medium text-stone-500">{label}</p>
      <p
        className={
          mono
            ? "mt-0.5 break-all font-mono text-[11px] text-stone-700"
            : "mt-0.5 break-all text-stone-800"
        }
      >
        {value}
      </p>
    </div>
  );
}

function DetailBlock({ label, body }: { label: string; body: string }) {
  return (
    <div className="mt-3">
      <p className="text-[11px] font-medium text-stone-500">{label}</p>
      <pre className="mt-1 max-h-56 overflow-auto rounded-lg border border-stone-200 bg-white p-2 font-mono text-[11px] leading-relaxed text-stone-700">
        {body}
      </pre>
    </div>
  );
}

/**
 * Everything known about one request, including the raw proxy metadata.
 *
 * Session-only rows come from a CLI transcript that never touched this proxy, so
 * they have no metadata to show; saying that outright beats an empty panel that
 * reads like a loading failure.
 */
function RequestDetail({ row }: { row: UsageOverviewRow }) {
  const metadata = row.metadata_json ? parseUsageMetadata(row.metadata_json) : null;
  const mapping =
    metadata?.requestedModel &&
    metadata.upstreamModel &&
    metadata.requestedModel !== metadata.upstreamModel
      ? `${metadata.requestedModel} → ${metadata.upstreamModel}`
      : null;
  return (
    <div
      aria-label={`请求 ${row.id} 详情`}
      className="border-t border-stone-100 bg-stone-50 px-3 py-3"
      id={detailId(row.id)}
    >
      <div className="flex items-center justify-between gap-2">
        <p className="text-[12px] font-semibold text-stone-800">请求详情</p>
        <p className="break-all font-mono text-[11px] text-stone-500">{row.id}</p>
      </div>
      <div className="mt-3 grid gap-2 text-[12px] sm:grid-cols-2 lg:grid-cols-3">
        <DetailField label="时间" value={formatTime(row.occurred_at)} />
        <DetailField
          label="来源"
          value={
            row.source_label
              ? `${sourceLabels[row.source]} · ${row.source_label}`
              : sourceLabels[row.source]
          }
        />
        <DetailField label="平台" value={row.provider} />
        <DetailField label="模型" value={row.model} />
        {mapping ? <DetailField label="模型映射" value={mapping} /> : null}
        <DetailField label="账号" value={row.account_name ?? "未经代理"} />
        <DetailField label="账号 ID" mono value={row.account_id ?? "-"} />
        <DetailField label="请求路径" mono value={row.path ?? "-"} />
        <DetailField label="状态" value={statusText(row)} />
        <DetailField label="输入 Token" value={formatExactCount(row.input_tokens)} />
        <DetailField label="输出 Token" value={formatExactCount(row.output_tokens)} />
        <DetailField label="缓存写入 Token" value={formatExactCount(row.cache_write_tokens)} />
        <DetailField label="缓存读取 Token" value={formatExactCount(row.cache_read_tokens)} />
        <DetailField label="费用" value={costText(row)} />
        <DetailField label="上游响应 ID" mono value={row.upstream_response_id ?? "-"} />
        {metadata?.durationMs ? (
          <DetailField label="耗时" value={`${metadata.durationMs} ms`} />
        ) : null}
        {metadata?.targetUrl ? (
          <DetailField label="上游地址" mono value={metadata.targetUrl} />
        ) : null}
        {metadata?.traceId ? <DetailField label="追踪 ID" mono value={metadata.traceId} /> : null}
      </div>
      {metadata?.errorMessage ? (
        <DetailBlock body={metadata.errorMessage} label="错误信息" />
      ) : null}
      {metadata?.responseBody ? (
        <DetailBlock body={prettyJsonOrText(metadata.responseBody)} label="上游原始响应" />
      ) : null}
      {metadata ? (
        <DetailBlock
          body={metadata.formatted}
          label={metadata.valid ? "metadata_json" : "metadata_json 无法解析，显示原始内容。"}
        />
      ) : (
        <p className="mt-3 text-[11px] text-stone-500">
          这条记录只来自本机 CLI 会话文件，没有经过本代理，因此没有代理元数据可展示。
        </p>
      )}
    </div>
  );
}

function RequestRow({
  row,
  expanded,
  onToggle,
}: {
  row: UsageOverviewRow;
  expanded: boolean;
  onToggle: () => void;
}) {
  const tokens = row.input_tokens + row.output_tokens;
  const cache = row.cache_write_tokens + row.cache_read_tokens;
  return (
    <div className="bg-white" data-usage-request-row>
      {/* Narrow widths get two tidy lines instead of the seven columns folding into
          a ragged 2-or-3 column block that stranded 详情 mid-row: the timestamp with
          the badges and 详情 pinned to its right, then the labelled fields wrapping
          underneath. `lg:contents` dissolves that second grouping so its fields
          become columns of the wide grid in DOM order, and the `order`/`ml-auto`
          that arrange the narrow lines have to be reset there or they would
          rearrange the grid too. */}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2.5 text-[12px] text-stone-600 lg:grid lg:grid-cols-[1.3fr_1.5fr_0.8fr_0.8fr_1.1fr_0.8fr_auto] lg:gap-2 lg:items-center">
        <span className="order-1 font-medium text-stone-800 lg:order-none">
          {formatTime(row.occurred_at)}
        </span>
        <div className="order-4 flex w-full min-w-0 flex-wrap items-center gap-x-3 gap-y-1 lg:contents">
          <span className="min-w-0 truncate" title={row.model}>
            <span className="mr-1 text-[10px] text-stone-400 lg:hidden">模型</span>
            {row.model}
          </span>
          <span
            title={`输入 ${formatExactCount(row.input_tokens)}；输出 ${formatExactCount(row.output_tokens)}；缓存写入 ${formatExactCount(row.cache_write_tokens)}；缓存读取 ${formatExactCount(row.cache_read_tokens)}`}
          >
            <span className="mr-1 text-[10px] text-stone-400 lg:hidden">Token</span>
            {formatCompactCount(tokens)}
            {cache > 0 ? <span className="ml-1 text-stone-400">+{formatCompactCount(cache)}</span> : null}
          </span>
          <span title={row.price_source === "estimated" ? "按本地价格表估算" : undefined}>
            <span className="mr-1 text-[10px] text-stone-400 lg:hidden">费用</span>
            {row.price_source ? formatCostMicros(row.cost_micros) : "无价格"}
            {row.price_source === "estimated" ? <span className="text-stone-400">(估)</span> : null}
          </span>
          <span className="min-w-0 truncate" title={row.account_name ?? row.account_id ?? undefined}>
            <span className="mr-1 text-[10px] text-stone-400 lg:hidden">账号</span>
            {row.account_name ?? row.account_id ?? "未经代理"}
          </span>
        </div>
        <span className="order-2 ml-auto flex items-center gap-1.5 lg:order-none lg:ml-0">
          <span
            className={`rounded-md px-1.5 py-0.5 text-[11px] font-semibold ${sourceStyles[row.source]}`}
          >
            {sourceLabels[row.source]}
          </span>
          {!row.success && row.status ? (
            <span className="rounded-md bg-red-50 px-1.5 py-0.5 text-[11px] font-semibold text-red-700 ring-1 ring-red-200">
              {row.status}
            </span>
          ) : null}
        </span>
        <button
          aria-controls={detailId(row.id)}
          aria-expanded={expanded}
          aria-label={`${expanded ? "隐藏" : "查看"}请求 ${row.id} 详情`}
          className="order-3 shrink-0 rounded-lg border border-stone-200 bg-white px-2.5 py-1 text-[12px] font-semibold text-stone-700 motion-control hover:bg-stone-50 lg:order-none lg:justify-self-end"
          onClick={onToggle}
          type="button"
        >
          详情
        </button>
      </div>
      {expanded ? <RequestDetail row={row} /> : null}
    </div>
  );
}

export function UsageOverviewPanel() {
  const [period, setPeriod] = useState<Period>("today");
  const [page, setPage] = useState(1);
  const [dimension, setDimension] = useState<GroupDimension | null>(null);
  const [groupView, setGroupView] = useState<GroupView>("list");
  const [pricingOpen, setPricingOpen] = useState(false);
  const [expandedRowId, setExpandedRowId] = useState<string | null>(null);

  const query = useQuery({
    // `since` is deliberately not part of the key. The rolling presets derive it
    // from `now`, so a key carrying it changes on every render, and each
    // settling fetch renders into the next one — an unbounded request loop where
    // every request is a full session-corpus scan. Recomputing it inside
    // `queryFn` keeps the window fresh on every poll while the key stays stable.
    queryKey: ["usage-overview", period, page, pageSize],
    queryFn: () => getUsageOverview(periodSince(period), page, pageSize),
    placeholderData: keepPreviousData,
    refetchInterval: refreshMs,
  });

  const overview = query.data;
  const totals = overview?.totals;
  const rowCount = overview?.row_count ?? 0;
  const pageCount = Math.max(1, Math.ceil(rowCount / pageSize));
  const cacheTotal = (totals?.cache_write_tokens ?? 0) + (totals?.cache_read_tokens ?? 0);
  const tokenTotal = (totals?.input_tokens ?? 0) + (totals?.output_tokens ?? 0) + cacheTotal;
  // Every prompt token is exactly one of three things — read from the cache, written
  // into it, or sent fresh — so their sum is the denominator, with no double counting.
  // Cache writes stay in it even though they are misses: a window spent building
  // caches genuinely is a poor hit rate. Output tokens are not part of a prompt and
  // are left out entirely, which would otherwise dilute the ratio with generation.
  const promptTokens =
    (totals?.input_tokens ?? 0) +
    (totals?.cache_write_tokens ?? 0) +
    (totals?.cache_read_tokens ?? 0);
  const cacheHitRate = promptTokens > 0 ? (totals?.cache_read_tokens ?? 0) / promptTokens : null;

  const selectPeriod = (next: Period) => {
    setPeriod(next);
    setPage(1);
  };

  const activeGroup = groupDimensions.find((item) => item.key === dimension);
  const groupRows = !overview || !activeGroup ? [] : overview.groups[activeGroup.field];
  const activeSeries = !overview || !activeGroup ? [] : overview.series[activeGroup.field];

  const integrityNotes = (() => {
    if (!overview) {
      return [];
    }
    const { integrity } = overview;
    const notes = [`已扫描 ${formatExactCount(integrity.scanned_file_count)} 个会话文件`];
    if (integrity.truncated) {
      notes.push("会话文件数量超过扫描上限，以下数字不完整");
    }
    if (integrity.unpriced_request_count > 0) {
      notes.push(
        `其中 ${formatExactCount(integrity.unpriced_request_count)} 个请求的模型没有价格数据，未计入费用`,
      );
    }
    if (integrity.estimated_price_request_count > 0) {
      notes.push(
        `${formatExactCount(integrity.estimated_price_request_count)} 个请求的费用按本地价格表估算`,
      );
    }
    if (integrity.unmatchable_proxy_row_count > 0) {
      notes.push(
        `${formatExactCount(integrity.unmatchable_proxy_row_count)} 条代理记录无法与会话记录匹配，对应请求可能被重复计入`,
      );
    }
    return notes;
  })();

  return (
    <div className="space-y-3 border-t border-stone-200/80 px-3 py-3">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-[13px] font-semibold text-stone-950">用量总览</p>
          <p className="text-[12px] text-stone-500">
            合并本机 CLI 会话记录与代理请求，同一请求只计一次
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            aria-label="配置模型价格"
            className="grid h-8 w-8 place-items-center rounded-lg border border-stone-200 bg-white text-stone-500 motion-control hover:bg-stone-50 hover:text-stone-900"
            onClick={() => setPricingOpen(true)}
            title="配置模型价格"
            type="button"
          >
            <Settings2 aria-hidden="true" className="h-4 w-4" />
          </button>
          <div className="grid grid-cols-7 gap-1 rounded-xl bg-stone-100 p-1">
          {periods.map((item) => (
            <button
              className={`rounded-lg px-2 py-1 text-[11px] font-semibold motion-control ${
                period === item.key
                  ? "bg-white text-stone-950 shadow-sm"
                  : "text-stone-500 hover:text-stone-900"
              }`}
              key={item.key}
              onClick={() => selectPeriod(item.key)}
              type="button"
            >
              {item.label}
            </button>
          ))}
          </div>
        </div>
      </div>

      {query.isError ? (
        <p className="rounded-lg bg-red-50 px-3 py-2 text-[12px] text-red-700" role="alert">
          {errorMessage(query.error)}
        </p>
      ) : !overview ? (
        <p className="text-[12px] text-stone-500" role="status">
          正在读取本机会话记录…首次扫描较慢，之后会走缓存。
        </p>
      ) : (
        <>
          <div className="grid gap-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-7">
            <SummaryCard
              label="请求"
              title={formatExactCount(totals?.request_count ?? 0)}
              value={formatCompactCount(totals?.request_count ?? 0)}
            />
            <SummaryCard
              label="输入 Token"
              title={formatExactCount(totals?.input_tokens ?? 0)}
              value={formatCompactCount(totals?.input_tokens ?? 0)}
            />
            <SummaryCard
              label="输出 Token"
              title={formatExactCount(totals?.output_tokens ?? 0)}
              value={formatCompactCount(totals?.output_tokens ?? 0)}
            />
            <SummaryCard
              label="缓存 Token"
              title={`写入 ${formatExactCount(totals?.cache_write_tokens ?? 0)}；读取 ${formatExactCount(totals?.cache_read_tokens ?? 0)}`}
              value={formatCompactCount(cacheTotal)}
            />
            <SummaryCard
              label="缓存命中率"
              title={
                cacheHitRate === null
                  ? "窗口内没有提示 Token，无从计算命中率"
                  : `缓存读取 ${formatExactCount(totals?.cache_read_tokens ?? 0)} ÷ 提示 ${formatExactCount(promptTokens)}（输入 + 缓存写入 + 缓存读取，不含输出）`
              }
              value={cacheHitRate === null ? "—" : `${(cacheHitRate * 100).toFixed(1)}%`}
            />
            <SummaryCard
              label="总计 Token"
              title={`${formatExactCount(tokenTotal)}（输入 + 输出 + 缓存写入 + 缓存读取）`}
              value={formatCompactCount(tokenTotal)}
            />
            <SummaryCard
              label="费用（USD）"
              title={`${((totals?.cost_micros ?? 0) / 1_000_000).toFixed(6)} USD`}
              value={formatCostMicros(totals?.cost_micros ?? 0)}
            />
          </div>

          {integrityNotes.length > 0 ? (
            <p className="text-[11px] text-stone-400">{integrityNotes.join("；")}。</p>
          ) : null}

          <div className="space-y-2">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-[12px] font-semibold text-stone-600">分组统计</span>
              <div className="flex gap-1 rounded-xl bg-stone-100 p-1">
                {groupDimensions.map((item) => (
                  <button
                    aria-pressed={dimension === item.key}
                    className={`rounded-lg px-2.5 py-1 text-[12px] font-semibold motion-control ${
                      dimension === item.key
                        ? "bg-white text-stone-950 shadow-sm"
                        : "text-stone-500 hover:text-stone-900"
                    }`}
                    key={item.key}
                    onClick={() => setDimension(dimension === item.key ? null : item.key)}
                    type="button"
                  >
                    {item.label}
                  </button>
                ))}
              </div>
              {activeGroup ? (
                <div className="ml-auto flex gap-1 rounded-lg bg-stone-100 p-0.5">
                  {groupViews.map(({ key, label, Icon }) => (
                    <button
                      aria-label={label}
                      aria-pressed={groupView === key}
                      className={`grid h-7 w-7 place-items-center rounded-md motion-control ${
                        groupView === key
                          ? "bg-white text-stone-950 shadow-sm"
                          : "text-stone-500 hover:text-stone-900"
                      }`}
                      key={key}
                      onClick={() => setGroupView(key)}
                      title={label}
                      type="button"
                    >
                      <Icon aria-hidden="true" className="h-3.5 w-3.5" />
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
            {activeGroup && groupView === "list" ? (
              <GroupTable header={activeGroup.header} rows={groupRows} />
            ) : null}
            {activeGroup && groupView === "chart" && overview ? (
              <UsageTrendChart
                buckets={overview.series.buckets}
                dimensionLabel={activeGroup.label}
                rows={activeSeries}
                stale={query.isFetching}
                undatedRequestCount={overview.series.undated_request_count}
                unit={overview.series.unit}
              />
            ) : null}
          </div>

          <div className="overflow-hidden rounded-xl border border-stone-200 bg-white">
            <div className="flex items-center justify-between gap-2 border-b border-stone-100 bg-stone-50 px-3 py-2">
              <p className="text-[12px] font-semibold text-stone-700">请求列表</p>
              <div className="flex items-center gap-1.5">
                <p className="text-[11px] font-medium text-stone-500">
                  {formatExactCount(rowCount)} 条
                </p>
                <button
                  aria-label="上一页"
                  className="grid h-6 w-6 place-items-center rounded-md border border-stone-300 bg-white text-stone-700 motion-control hover:bg-stone-100 disabled:cursor-not-allowed disabled:opacity-40"
                  disabled={page <= 1}
                  onClick={() => setPage((current) => Math.max(1, current - 1))}
                  type="button"
                >
                  <ChevronLeft aria-hidden="true" className="h-3.5 w-3.5" />
                </button>
                <span className="whitespace-nowrap font-mono text-[11px] text-stone-600">
                  {page}/{pageCount}
                </span>
                <button
                  aria-label="下一页"
                  className="grid h-6 w-6 place-items-center rounded-md border border-stone-300 bg-white text-stone-700 motion-control hover:bg-stone-100 disabled:cursor-not-allowed disabled:opacity-40"
                  disabled={page >= pageCount}
                  onClick={() => setPage((current) => Math.min(pageCount, current + 1))}
                  type="button"
                >
                  <ChevronRight aria-hidden="true" className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>
            {overview.rows.length === 0 ? (
              <p className="px-3 py-4 text-[12px] text-stone-500">当前筛选范围内暂无请求。</p>
            ) : (
              <div className="divide-y divide-stone-100">
                {overview.rows.map((row) => (
                  <RequestRow
                    expanded={expandedRowId === row.id}
                    key={row.id}
                    onToggle={() =>
                      setExpandedRowId((current) => (current === row.id ? null : row.id))
                    }
                    row={row}
                  />
                ))}
              </div>
            )}
          </div>
        </>
      )}
      {pricingOpen ? <ModelPricingDialog open onClose={() => setPricingOpen(false)} /> : null}
    </div>
  );
}
