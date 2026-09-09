import { useState, type FormEvent } from "react";
import { Search } from "lucide-react";
import type { LogResult, RequestLog } from "../types";
import { formatCount, formatDate, formatMoney } from "../format";
import { useSaasLocale } from "../i18n";
import { Button, Card, Dialog, ErrorState, Field, Loading, Notice, Pagination, Status, Table, useResource } from "./ui";

export function LogTable({ items }: { items: RequestLog[] }) {
  const { locale, text } = useSaasLocale();
  const [detail, setDetail] = useState<RequestLog | null>(null);
  return <><Table items={items} rowKey={item => item.requestId} columns={[
    { label: text("时间 / 请求", "Time / request"), render: item => <><Button tone="quiet" onClick={() => setDetail(item)}>{formatDate(item.createdAt, locale)}</Button><small className="saas-mono saas-muted">{item.requestId}</small></> },
    { label: text("模型", "Model"), render: item => <><strong>{item.model}</strong><small className="saas-muted">{item.endpoint}</small></> },
    { label: text("输入 / 缓存 / 输出", "Input / cache / output"), render: item => <span className="saas-tabular">{formatCount(item.inputTokens, locale)} / {formatCount(item.cacheReadTokens, locale)} / {formatCount(item.outputTokens, locale)}</span> },
    { label: text("费用", "Cost"), render: item => formatMoney(item.amountUsdMicros, locale) },
    { label: text("状态", "Status"), render: item => <Status value={item.settlementStatus} /> },
  ]} />{detail && <Dialog title={text("请求详情", "Request details")} onClose={() => setDetail(null)}><dl className="saas-details"><dt>{text("请求 ID", "Request ID")}</dt><dd className="saas-mono">{detail.requestId}</dd><dt>{text("密钥 ID", "Key ID")}</dt><dd>{detail.keyId}</dd>{detail.userId && <><dt>{text("用户 ID", "User ID")}</dt><dd>{detail.userId}</dd></>}<dt>{text("模型", "Model")}</dt><dd>{detail.model}</dd><dt>{text("耗时", "Duration")}</dt><dd>{formatCount(detail.durationMs, locale)} ms</dd><dt>{text("费用", "Cost")}</dt><dd>{formatMoney(detail.amountUsdMicros, locale)}</dd><dt>{text("状态", "Status")}</dt><dd><Status value={detail.settlementStatus} /></dd>{detail.errorCode && <><dt>{text("错误码", "Error code")}</dt><dd>{detail.errorCode}</dd></>}</dl><Notice>{text("仅展示脱敏元数据，不保存提示词、响应正文或密钥。", "Only redacted metadata is shown. Prompts, response bodies, and secrets are not stored.")}</Notice></Dialog>}</>;
}

export function RequestLogs({ query, admin = false }: { query: (payload: unknown) => Promise<LogResult>; admin?: boolean }) {
  const { text } = useSaasLocale();
  const [draft, setDraft] = useState({ from: new Date(Date.now() - 6 * 86400000).toISOString().slice(0, 10), to: new Date().toISOString().slice(0, 10), model: "", keyId: "", userId: "", status: "" });
  const [filters, setFilters] = useState(draft);
  const [page, setPage] = useState(1);
  const resource = useResource(() => query({ ...filters, model: filters.model || undefined, keyId: filters.keyId || undefined, userId: admin ? filters.userId || undefined : undefined, status: filters.status ? Number(filters.status) : undefined, from: `${filters.from}T00:00:00Z`, to: `${filters.to}T23:59:59Z`, page, pageSize: 20 }), [filters, page]);
  const [invalid, setInvalid] = useState(false);
  function search(event: FormEvent) { event.preventDefault(); if (draft.from > draft.to) { setInvalid(true); return; } setInvalid(false); setFilters(draft); setPage(1); resource.reload(); }
  return <Card title={text("请求记录", "Request records")}><form className="saas-filters" onSubmit={search}>
    <Field label={text("开始日期 (UTC)", "From (UTC)")}><input type="date" required value={draft.from} onChange={event => setDraft({ ...draft, from: event.target.value })} /></Field>
    <Field label={text("结束日期 (UTC)", "To (UTC)")}><input type="date" required value={draft.to} onChange={event => setDraft({ ...draft, to: event.target.value })} /></Field>
    <Field label={text("模型", "Model")}><input value={draft.model} onChange={event => setDraft({ ...draft, model: event.target.value })} /></Field>
    <Field label={text("密钥 ID", "Key ID")}><input value={draft.keyId} onChange={event => setDraft({ ...draft, keyId: event.target.value })} /></Field>
    {admin && <Field label={text("用户 ID", "User ID")}><input value={draft.userId} onChange={event => setDraft({ ...draft, userId: event.target.value })} /></Field>}

    <Field label={text("状态", "Status")}><select value={draft.status} onChange={event => setDraft({ ...draft, status: event.target.value })}><option value="">{text("全部", "All")}</option>{[200,400,401,403,429,500,502,503,504].map(status => <option key={status} value={status}>{status}</option>)}</select></Field>
    <Button type="submit" busy={resource.loading}><Search size={16} />{text("查询", "Search")}</Button>
  </form>{invalid && <Notice tone="error">{text("结束日期不能早于开始日期。", "The end date must not precede the start date.")}</Notice>}
  <p className="saas-caption">{text("日志异步落盘，可能晚于余额和聚合更新。日期使用 UTC。", "Logs are written asynchronously and may lag behind balances and aggregates. Dates use UTC.")}{resource.data?.driver && ` · ${resource.data.driver}`}</p>
  {resource.loading ? <Loading /> : resource.error ? <ErrorState error={resource.error} retry={resource.reload} /> : resource.data && <>{resource.data.warning && <Notice tone="warning">{resource.data.warning}</Notice>}{resource.data.dropped != null && resource.data.dropped > 0 && <Notice tone="warning">{text("丢弃的日志数：", "Dropped log records: ")}{resource.data.dropped}</Notice>}<LogTable items={resource.data.items} /><Pagination page={page} total={resource.data.total} onPage={setPage} /></>}
  </Card>;
}
