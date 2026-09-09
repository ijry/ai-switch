import { useState } from "react";
import { adminCall } from "../api";
import type { AdminOverview, OperatingReport } from "../types";
import { formatCount, formatMoney } from "../format";
import { useSaasLocale } from "../i18n";
import { Button, Card, ErrorState, Loading, Metric, Notice, useResource } from "../components/ui";
import { CostEstimate, consumption, monthStart, percent, utcDay } from "./OperatingShared";
import { OperatingHistory } from "./OperatingHistory";
type Destination = "users" | "growth" | "recharges" | "ledger" | "logs";
function Health({ overview, current, navigate }: { overview: AdminOverview; current?: OperatingReport["current"]; navigate: (tab: Destination) => void }) {
  const { locale, text } = useSaasLocale();
  return <div className="saas-billing-grid"><Card title={text("需要处理", "Needs attention")}>
    {([[text("待审核充值", "Pending recharges"), overview.pendingRechargeCount, "recharges"], [text("待审核邀请奖励", "Pending referral rewards"), current?.pendingRewards, "growth"], [text("7 天内到期订阅", "Subscriptions expiring in 7 days"), current?.expiringSubscriptions, "users"]] as const).map(([label, count, tab]) => <div className="saas-attention-row" key={tab}><div><strong>{label}</strong></div><span className="saas-attention-number">{formatCount(count, locale)}</span><Button onClick={() => navigate(tab)}>{text("查看", "View")}</Button></div>)}
  </Card><Card title={text("账户与账务健康", "Accounts & accounting health")}><dl className="saas-details">
    <dt>{text("用户数", "Users")}</dt><dd>{formatCount(overview.userCount, locale)}</dd>
    <dt>{text("用户总余额", "Total user balance")}</dt><dd>{formatMoney(overview.balanceMicros, locale)}</dd>
    <dt>{text("冻结金额", "Reserved funds")}</dt><dd>{formatMoney(overview.frozenMicros, locale)}</dd>
    <dt>{text("欠费金额", "Debt")}</dt><dd>{formatMoney(overview.debtMicros, locale)}</dd>
    <dt>{text("待核对金额", "Pending reconciliation")}</dt><dd>{formatMoney(overview.pendingReviewMicros, locale)}</dd>
  </dl><Notice>{text("余额与订阅额度是待履约权益，不能当作利润。用户消耗是计费金额，不是上游采购成本。", "Balances and quotas are outstanding entitlements, not profit. Customer consumption is billing, not upstream cost.")}</Notice></Card></div>;
}
export function OperationsDashboard({ navigate }: { navigate: (tab: Destination) => void }) {
  const { locale, text } = useSaasLocale();
  const [history, setHistory] = useState<string | null>(null);
  const overview = useResource(() => adminCall<AdminOverview>("overview"));
  const report = useResource(() => adminCall<OperatingReport>("statistics", { from: monthStart(), to: utcDay(), granularity: "day" }));
  if (history) return <OperatingHistory focus={history} onBack={() => { setHistory(null); overview.reload(); report.reload(); }} />;
  const data = report.data;
  const current = data?.current;
  const today = data?.items?.find(row => row.period === utcDay());
  const totals = data?.totals;
  const peak = Math.max(1, ...(data?.items || []).map(row => consumption(row) || 0));
  const more = (label: string) => <Button tone="quiet" aria-label={text("更多 · ", "More · ") + label} onClick={() => setHistory(label)}>{text("更多", "More")} →</Button>;
  return <><div className="saas-section-heading"><div><h2>{text("运营总览", "Service overview")}</h2><p className="saas-muted">UTC · {text("金额按实际账本汇总", "Amounts from accounting records")}</p></div><Button busy={overview.loading || report.loading} onClick={() => { overview.reload(); report.reload(); }}>{text("刷新", "Refresh")}</Button></div>
    {report.loading ? <Loading /> : report.error ? <ErrorState error={report.error} retry={report.reload} /> : current && <>
      <div className="saas-metrics">
        <Metric label={text("总有效订阅", "Active subscriptions")} value={formatCount(current.activeSubscriptions, locale)} detail={<>{text("今日额度使用率", "Today's quota utilization")} {percent(current.todayUsedMicros, current.todayQuotaMicros)} · {current.subscribedUsers} {text("位订阅用户", "subscribers")}{more(text("订阅", "Subscriptions"))}</>} />
        <Metric label={text("今日充值实收", "Today's cash recharges")} value={formatMoney(today?.rechargeCnyFen, locale, "CNY")} detail={<>{text("已审核到账，不含人工入账", "Approved receipts; excludes manual credits")}{more(text("充值", "Recharges"))}</>} />
        <Metric label={text("今日消耗金额", "Today's consumption")} value={formatMoney(consumption(today), locale)} detail={<>{text("余额", "Wallet")} {formatMoney(today?.walletUsageMicros, locale)} · {text("订阅", "Subscription")} {formatMoney(today?.subscriptionUsageMicros, locale)}{more(text("消耗", "Consumption"))}</>} />
        <Metric label={text("今日已结算请求", "Today's finalized requests")} value={formatCount(today?.requestCount, locale)} detail={<>{text("今日新增用户", "New users today")} {formatCount(today?.newUsers, locale)}{more(text("请求", "Requests"))}</>} />
      </div>
      <Card title={text("本月经营概况", "Month to date")} action={more(text("月度经营", "Monthly operations"))}><div className="saas-metrics">
        <Metric label={text("本月现金实收", "Cash receipts")} value={formatMoney(totals?.rechargeCnyFen, locale, "CNY")} />
        <Metric label={text("本月用户消耗", "User consumption")} value={formatMoney(consumption(totals), locale)} />
        <Metric label={text("套餐余额购买", "Wallet spent on plans")} value={formatMoney(totals?.subscriptionSalesMicros, locale)} detail={text("不重复计入现金收入", "Not additional cash income")} />
        <Metric label={text("人工入账 / 奖励兑换", "Manual credits / rewards")} value={formatMoney(totals?.manualCreditMicros, locale)} detail={formatMoney(totals?.rewardMicros, locale)} />
      </div></Card>
      <Card title={text("每日消耗走势", "Daily consumption trend")} action={more(text("消耗走势", "Consumption trend"))}><div className="saas-chart" role="img" aria-label={text("每日消耗，精确数据可在历史报表查看", "Daily consumption; exact values in the history report")}>{(data?.items || []).map(row => <div className="saas-chart-column" key={row.period} title={row.period + " · " + formatMoney(consumption(row), locale)}><span style={{ height: (consumption(row) || 0) / peak * 100 + "%" }} /><small>{row.period.slice(-2)}</small></div>)}</div></Card>
    </>}
    {overview.error ? <ErrorState error={overview.error} retry={overview.reload} /> : overview.loading ? <Loading /> : overview.data && <Health overview={overview.data} current={current} navigate={navigate} />}
    {totals && <CostEstimate row={totals} />}
  </>;
}
