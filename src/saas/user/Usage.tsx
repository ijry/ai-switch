import { useState, type FormEvent } from "react";
import type { SaasUserClient } from "../api";
import type { LogResult, PageResult, SaasKey, UsageResult } from "../types";
import { formatCount, formatMoney } from "../format";
import { useSaasLocale } from "../i18n";
import { RequestLogs } from "../components/Logs";
import { Button, Card, Empty, ErrorState, Field, Heading, Loading, Metric, Notice, Table, useResource } from "../components/ui";

export function UsagePage({ client }: { client: SaasUserClient }) {
  const { locale, text } = useSaasLocale();
  const [draft, setDraft] = useState({ from: new Date(Date.now() - 29 * 86400000).toISOString().slice(0, 10), to: new Date().toISOString().slice(0, 10), keyId: "", model: "" });
  const [filters, setFilters] = useState(draft);
  const [invalid, setInvalid] = useState(false);
  const keys = useResource(() => client.call<PageResult<SaasKey>>("keys.list", { page: 1, pageSize: 100 }));
  const usage = useResource(() => client.call<UsageResult>("usage", { ...filters, keyId: filters.keyId || undefined, model: filters.model || undefined, from: `${filters.from}T00:00:00Z`, to: `${filters.to}T23:59:59Z` }), [filters]);
  function apply(event: FormEvent) { event.preventDefault(); if (draft.from > draft.to) { setInvalid(true); return; } setInvalid(false); setFilters(draft); usage.reload(); }
  const maximum = Math.max(1, ...(usage.data?.buckets || []).map(bucket => bucket.costMicros));
  return <><Heading eyebrow={text("透明计量", "TRANSPARENT METERING")} title={text("用量与请求", "Usage & requests")} description={text("按时间、密钥和模型了解真实消耗。", "Understand your actual consumption by date, key, and model.")} />
    <Card><form className="saas-filters" onSubmit={apply}><Field label={text("开始日期", "Start date")}><input type="date" required value={draft.from} onChange={event => setDraft({ ...draft, from: event.target.value })} /></Field><Field label={text("结束日期", "End date")}><input type="date" required value={draft.to} onChange={event => setDraft({ ...draft, to: event.target.value })} /></Field><Field label={text("API 密钥", "API key")}><select value={draft.keyId} onChange={event => setDraft({ ...draft, keyId: event.target.value })}><option value="">{text("全部密钥", "All keys")}</option>{keys.data?.items.map(key => <option key={key.id} value={key.id}>{key.name}</option>)}</select></Field><Field label={text("模型", "Model")}><input value={draft.model} onChange={event => setDraft({ ...draft, model: event.target.value })} /></Field><Button type="submit" tone="primary" busy={usage.loading}>{text("应用筛选", "Apply filters")}</Button></form>{keys.error != null && <ErrorState error={keys.error} retry={keys.reload} />}{invalid && <Notice tone="error">{text("日期范围无效。", "Invalid date range.")}</Notice>}</Card>
    {usage.loading ? <Loading /> : usage.error ? <ErrorState error={usage.error} retry={usage.reload} /> : usage.data && <><div className="saas-metrics"><Metric label={text("总费用", "Total cost")} value={formatMoney(usage.data.totals?.costMicros, locale)} /><Metric label={text("请求数", "Requests")} value={formatCount(usage.data.totals?.requestCount, locale)} /><Metric label={text("输入 / 缓存 token", "Input / cached tokens")} value={`${formatCount(usage.data.totals?.inputTokens, locale)} / ${formatCount(usage.data.totals?.cacheReadTokens, locale)}`} /><Metric label={text("输出 token", "Output tokens")} value={formatCount(usage.data.totals?.outputTokens, locale)} /></div><Card title={text("每日费用 · USD", "Daily spend · USD")}>{usage.data.buckets?.length ? <><div className="saas-chart" role="img" aria-label={text("每日实际费用柱状图；下方表格提供完整数据。", "Daily actual spend chart. The table below contains the full data.")}>{usage.data.buckets.map(bucket => <div className="saas-chart-column" key={bucket.date} title={`${bucket.date}: ${formatMoney(bucket.costMicros, locale)}`}><span style={{ height: `${Math.max(bucket.costMicros > 0 ? 2 : 0, bucket.costMicros / maximum * 100)}%` }} /><small>{bucket.date.slice(5, 10)}</small></div>)}</div><details><summary>{text("查看图表数据", "View chart data")}</summary><Table items={usage.data.buckets} rowKey={bucket => bucket.date} columns={[{ label: text("日期", "Date"), render: bucket => bucket.date }, { label: text("请求", "Requests"), render: bucket => formatCount(bucket.requestCount, locale) }, { label: text("费用", "Cost"), render: bucket => formatMoney(bucket.costMicros, locale) }]} /></details></> : <Empty description={text("所选时间范围内没有用量。", "No usage in the selected date range.")} />}</Card></>}
    <RequestLogs query={payload => client.call<LogResult>("logs.query", payload)} />
  </>;
}
