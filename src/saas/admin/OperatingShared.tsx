import { useState } from "react";
import type { OperatingRow } from "../types";
import { formatMoney } from "../format";
import { useSaasLocale } from "../i18n";
import { Card, Field, Notice } from "../components/ui";
export const utcDay = () => new Date().toISOString().slice(0, 10);
export const monthStart = () => utcDay().slice(0, 7) + "-01";
export const consumption = (row?: OperatingRow) => row ? row.walletUsageMicros + row.subscriptionUsageMicros : undefined;
export const percent = (used: number, quota: number) => quota > 0 ? (used / quota * 100).toFixed(1) + "%" : "—";
export function CostEstimate({ row }: { row: OperatingRow }) {
  const { locale, text } = useSaasLocale();
  const [ratio, setRatio] = useState("");
  const [exchange, setExchange] = useState("");
  const [fixed, setFixed] = useState("");
  const valid = [ratio, exchange, fixed].every(value => value.trim() !== "" && Number.isFinite(Number(value)) && Number(value) >= 0) && Number(exchange) > 0;
  const cost = valid ? Math.round((consumption(row) || 0) / 1000000 * Number(ratio) * Number(exchange) * 100 + Number(fixed) * 100) : undefined;
  return <Card title={text("成本与现金结余测算（当前区间）", "Cost & cash remainder estimate (selected period)")}><div className="saas-form-grid">
    <Field label={text("上游成本 / 用户消耗系数", "Upstream cost / consumption ratio")} hint={text("自行输入，如 0.3；不是系统测得的真实成本。", "Enter an assumption, e.g. 0.3; not measured cost.")}><input type="number" min={0} step="any" value={ratio} onChange={event => setRatio(event.target.value)} /></Field>
    <Field label={text("成本汇率 CNY / USD", "Cost exchange rate CNY / USD")}><input type="number" min={0} step="any" value={exchange} onChange={event => setExchange(event.target.value)} /></Field>
    <Field label={text("区间固定开支 CNY", "Fixed expenses for this period CNY")}><input type="number" min={0} step="any" value={fixed} onChange={event => setFixed(event.target.value)} /></Field>
  </div><dl className="saas-details"><dt>{text("测算总成本", "Estimated total cost")}</dt><dd>{formatMoney(cost, locale, "CNY")}</dd><dt>{text("实收减测算成本", "Cash receipts less estimated cost")}</dt><dd>{formatMoney(cost == null ? undefined : row.rechargeCnyFen - cost, locale, "CNY")}</dd></dl><Notice>{text("仅供测算，不等同于利润：充值包含预付款，消耗可能来自往期余额或赠额。输入只用于当前页面，不保存；未填写时不显示虚假零成本。", "Estimate, not profit: receipts include prepayments, and consumption may use older balances or gifts. Inputs are not saved. Blank assumptions do not imply zero cost.")}</Notice></Card>;
}
