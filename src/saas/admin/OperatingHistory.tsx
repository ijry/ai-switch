import { useState } from "react";
import { adminCall } from "../api";
import type { OperatingReport, OperatingRow } from "../types";
import { downloadText, formatMoney } from "../format";
import { useSaasLocale } from "../i18n";
import { Button, Card, ErrorState, Field, Loading, Notice, Table, useResource } from "../components/ui";
import { CostEstimate, monthStart, percent, utcDay } from "./OperatingShared";
export function OperatingHistory({ focus, onBack }: { focus: string; onBack: () => void }) {
  const { locale, text } = useSaasLocale();
  const [draft, setDraft] = useState({ from: monthStart(), to: utcDay(), granularity: "day" });
  const [filters, setFilters] = useState(draft);
  const report = useResource(() => adminCall<OperatingReport>("statistics", filters), [filters]);
  const [page, setPage] = useState(1);
  const rows = report.data?.items || [];
  const columns = [
    { label: text("日期 UTC", "Period UTC"), render: (row: OperatingRow) => row.period },
    { label: text("充值实收 CNY", "Cash recharges CNY"), render: (row: OperatingRow) => formatMoney(row.rechargeCnyFen, locale, "CNY") },
    { label: text("充值到账 USD", "Recharge wallet credit USD"), render: (row: OperatingRow) => formatMoney(row.rechargeCreditMicros, locale) },
    { label: text("余额消耗", "Wallet consumption"), render: (row: OperatingRow) => formatMoney(row.walletUsageMicros, locale) },
    { label: text("订阅消耗 / 使用率", "Subscription consumption / utilization"), render: (row: OperatingRow) => <>{formatMoney(row.subscriptionUsageMicros, locale)}<small>{percent(row.subscriptionUsageMicros, row.quotaMicros)}</small></> },
    { label: text("每日额度累计", "Accumulated daily quotas"), render: (row: OperatingRow) => formatMoney(row.quotaMicros, locale) },
    { label: text("套餐余额购买", "Plan wallet purchases"), render: (row: OperatingRow) => formatMoney(row.subscriptionSalesMicros, locale) },
    { label: text("人工入账", "Manual wallet credit"), render: (row: OperatingRow) => formatMoney(row.manualCreditMicros, locale) },
    { label: text("奖励与兑换", "Rewards & redemption"), render: (row: OperatingRow) => formatMoney(row.rewardMicros, locale) },
    { label: text("请求 / 新用户", "Requests / new users"), render: (row: OperatingRow) => row.requestCount + " / " + row.newUsers },
  ];
  function shortcut(months: number) {
    const start = new Date(); start.setUTCDate(1); start.setUTCMonth(start.getUTCMonth() - months);
    const next = { from: start.toISOString().slice(0, 10), to: utcDay(), granularity: months > 1 ? "month" : "day" };
    setDraft(next); setFilters(next); setPage(1);
  }
  return <><div className="saas-section-heading"><div><h2>{text("历史运营统计", "Operating history")}</h2><p className="saas-muted">{focus} · UTC</p></div><Button onClick={onBack}>{text("返回总览", "Back to overview")}</Button></div>
    <Card><form className="saas-filters" onSubmit={event => { event.preventDefault(); setFilters({ ...draft }); setPage(1); report.reload(); }}>
      <Field label={text("开始日期", "From")}><input type="date" required max={utcDay()} value={draft.from} onChange={event => setDraft({ ...draft, from: event.target.value })} /></Field>
      <Field label={text("结束日期", "To")}><input type="date" required min={draft.from} max={utcDay()} value={draft.to} onChange={event => setDraft({ ...draft, to: event.target.value })} /></Field>
      <Field label={text("统计粒度", "Aggregation")}><select value={draft.granularity} onChange={event => setDraft({ ...draft, granularity: event.target.value })}><option value="day">{text("按日", "Daily")}</option><option value="month">{text("按月", "Monthly")}</option></select></Field><Button type="submit" busy={report.loading}>{text("查询", "Query")}</Button>
    </form><div className="saas-actions"><Button onClick={() => shortcut(0)}>{text("本月", "This month")}</Button><Button onClick={() => shortcut(2)}>{text("近三个月", "Last 3 months")}</Button><Button onClick={() => shortcut(11)}>{text("近十二个月", "Last 12 months")}</Button><Button disabled={!rows.length} onClick={() => { if (!report.data) return; const fields = Object.keys(report.data.totals) as (keyof OperatingRow)[]; downloadText("saas-operations.csv", fields.join(",") + "\n" + rows.map(row => fields.map(field => row[field]).join(",")).join("\n")); }}>{text("导出 CSV（原始分/微美元）", "Export CSV (raw cents / micro-USD)")}</Button></div></Card>
    <Notice>{text("日期包含首尾日。充值按审核到账时间，消耗按结算时间；使用率=区间订阅消耗÷每日额度累计。跨日预占延迟结算可能与额度日口径不同。人工入账没有收款凭据，不计入现金收入。", "Inclusive UTC dates. Recharges use approval time; consumption uses settlement time. Utilization divides subscription consumption by daily quota totals. Cross-day settlement can differ from quota-day usage. Manual credits are not cash receipts.")}</Notice>
    {report.loading ? <Loading /> : report.error ? <ErrorState error={report.error} retry={report.reload} /> : report.data?.totals && <>
      <Card title={text("区间合计", "Period totals")}><Table items={[report.data.totals]} rowKey={row => row.period} columns={columns} /></Card>
      <Card title={text("历史明细", "History details")}><Table items={rows.slice((page-1)*31,page*31)} rowKey={row => row.period} columns={columns} /><div className="saas-pagination"><Button disabled={page===1} onClick={() => setPage(page-1)}>{text("上一页", "Previous")}</Button><span>{page} / {Math.max(1,Math.ceil(rows.length/31))}</span><Button disabled={page*31>=rows.length} onClick={() => setPage(page+1)}>{text("下一页", "Next")}</Button></div></Card>
      <CostEstimate key={JSON.stringify(filters)} row={report.data.totals} />
    </>}
  </>;
}
