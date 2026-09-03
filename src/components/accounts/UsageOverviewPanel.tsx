import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Settings2 } from "lucide-react";
import { useMemo, useState } from "react";
import { ModelPricingDialog } from "./ModelPricingDialog";
import { getUsageOverview } from "../../lib/api/client";
import { formatCompactCount, formatExactCount } from "../../lib/usageFormat";
import type {
  UsageOverviewGroupRow,
  UsageOverviewRow,
  UsageRowSource,
} from "../../lib/api/types";

const periods = [
  { key: "today", label: "当日" },
  { key: "week", label: "本周" },
  { key: "month", label: "本月" },
  { key: "all", label: "累计" },
] as const;

type Period = (typeof periods)[number]["key"];

const groupDimensions = [
  { key: "model", label: "模型", header: "模型" },
  { key: "platform", label: "平台", header: "平台" },
  { key: "account", label: "账号", header: "账号" },
  { key: "source", label: "来源", header: "来源" },
] as const;

type GroupDimension = (typeof groupDimensions)[number]["key"];

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
  if (period === "all") {
    return null;
  }

  const start = new Date(now);
  start.setHours(0, 0, 0, 0);

  if (period === "week") {
    const day = start.getDay();
    const daysSinceMonday = day === 0 ? 6 : day - 1;
    start.setDate(start.getDate() - daysSinceMonday);
  }

  if (period === "month") {
    start.setDate(1);
  }

  return start.toISOString();
}

/**
 * Format a USD-micros total for a summary card.
 *
 * Fixed two-decimal formatting rendered any real amount under half a cent as
 * "$0.00", which is indistinguishable from having no cost data at all. Small
 * totals therefore get more decimals rather than being rounded away.
 */
function formatCostMicros(micros: number) {
  const dollars = micros / 1_000_000;
  if (dollars === 0) {
    return "$0.00";
  }
  if (Math.abs(dollars) < 0.01) {
    return `$${dollars.toFixed(6)}`;
  }
  if (Math.abs(dollars) < 1) {
    return `$${dollars.toFixed(4)}`;
  }
  return `$${dollars.toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
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
        {value}
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

function RequestRow({ row }: { row: UsageOverviewRow }) {
  const tokens = row.input_tokens + row.output_tokens;
  const cache = row.cache_write_tokens + row.cache_read_tokens;
  return (
    <div className="grid grid-cols-2 gap-2 px-3 py-2.5 text-[12px] text-stone-600 sm:grid-cols-3 lg:grid-cols-[1.3fr_1.5fr_0.8fr_0.8fr_1.1fr_0.8fr] lg:items-center">
      <span className="font-medium text-stone-800">{formatTime(row.occurred_at)}</span>
      <span className="truncate" title={row.model}>
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
      <span className="truncate" title={row.account_name ?? row.account_id ?? undefined}>
        <span className="mr-1 text-[10px] text-stone-400 lg:hidden">账号</span>
        {row.account_name ?? row.account_id ?? "未经代理"}
      </span>
      <span className="flex items-center gap-1.5">
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
    </div>
  );
}

export function UsageOverviewPanel() {
  const [period, setPeriod] = useState<Period>("today");
  const [page, setPage] = useState(1);
  const [dimension, setDimension] = useState<GroupDimension | null>(null);
  const [pricingOpen, setPricingOpen] = useState(false);

  const since = useMemo(() => periodSince(period), [period]);

  const query = useQuery({
    queryKey: ["usage-overview", since, page, pageSize],
    queryFn: () => getUsageOverview(since, page, pageSize),
    placeholderData: keepPreviousData,
    refetchInterval: refreshMs,
  });

  const overview = query.data;
  const totals = overview?.totals;
  const rowCount = overview?.row_count ?? 0;
  const pageCount = Math.max(1, Math.ceil(rowCount / pageSize));
  const cacheTotal = (totals?.cache_write_tokens ?? 0) + (totals?.cache_read_tokens ?? 0);

  const selectPeriod = (next: Period) => {
    setPeriod(next);
    setPage(1);
  };

  const activeGroup = groupDimensions.find((item) => item.key === dimension);
  const groupRows = (() => {
    if (!overview || !activeGroup) {
      return [];
    }
    switch (activeGroup.key) {
      case "model":
        return overview.groups.by_model;
      case "platform":
        return overview.groups.by_platform;
      case "account":
        return overview.groups.by_account;
      case "source":
        return overview.groups.by_source;
    }
  })();

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
            className="grid h-8 w-8 place-items-center rounded-lg border border-stone-200 bg-white text-stone-500 transition-colors hover:bg-stone-50 hover:text-stone-900"
            onClick={() => setPricingOpen(true)}
            title="配置模型价格"
            type="button"
          >
            <Settings2 aria-hidden="true" className="h-4 w-4" />
          </button>
          <div className="grid grid-cols-4 gap-1 rounded-xl bg-stone-100 p-1">
          {periods.map((item) => (
            <button
              className={`rounded-lg px-2.5 py-1.5 text-[12px] font-semibold transition-colors ${
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
          <div className="grid gap-2 sm:grid-cols-3 lg:grid-cols-5">
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
                    className={`rounded-lg px-2.5 py-1 text-[12px] font-semibold transition-colors ${
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
            </div>
            {activeGroup ? <GroupTable header={activeGroup.header} rows={groupRows} /> : null}
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
                  className="grid h-6 w-6 place-items-center rounded-md border border-stone-300 bg-white text-stone-700 transition-colors hover:bg-stone-100 disabled:cursor-not-allowed disabled:opacity-40"
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
                  className="grid h-6 w-6 place-items-center rounded-md border border-stone-300 bg-white text-stone-700 transition-colors hover:bg-stone-100 disabled:cursor-not-allowed disabled:opacity-40"
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
                  <RequestRow key={row.id} row={row} />
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
