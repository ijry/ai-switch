import { useState } from "react";
import { Activity, Boxes, Coins, ExternalLink, Gift, Layers3, ReceiptText, RefreshCw, Settings2, Ticket, Users, Wallet } from "lucide-react";
import { adminCall } from "../api";
import type { AdminOverview, LogResult, SaasConfig, SaasHostProps } from "../types";
import { getWebServerStatus } from "../../lib/api/client";
import { openExternal } from "../../lib/openExternal";
import { formatCount, formatMoney } from "../format";
import { SaasFrame, useSaasLocale } from "../i18n";
import { RequestLogs } from "../components/Logs";
import { Button, Card, ErrorState, Heading, Loading, Metric, Notice, useResource } from "../components/ui";
import { SettingsContent } from "../settings";
import { GroupsPanel } from "./Groups";
import { UsersPanel } from "./Users";
import { CodesPanel } from "./Codes";
import { LedgerPanel } from "./Ledger";
import { RechargesPanel } from "./Recharges";
import { GrowthAdminPanel } from "./Growth";
import "../saas.css";

type AdminTab = "overview" | "users" | "groups" | "growth" | "recharges" | "codes" | "ledger" | "logs" | "config";

export function SaasAdmin(props: SaasHostProps) { return <SaasFrame embedded><AdminContent {...props} /></SaasFrame>; }

function AdminContent({ onConfigChanged }: SaasHostProps) {
  const { text } = useSaasLocale();
  const [tab, setTab] = useState<AdminTab>("overview");
  const [ledgerUser, setLedgerUser] = useState("");
  const site = useResource(async () => {
    const [config, status] = await Promise.all([
      adminCall<SaasConfig>("config.get"),
      getWebServerStatus(),
    ]);
    return { enabled: config.enabled, url: config.publicBaseUrl || status.baseUrl || "" };
  });
  const tabs: { id: AdminTab; label: string; icon: typeof Users }[] = [
    { id: "overview", label: text("总览", "Overview"), icon: Boxes }, { id: "users", label: text("用户", "Users"), icon: Users }, { id: "groups", label: text("分组与定价", "Groups & pricing"), icon: Layers3 }, { id: "growth", label: text("订阅与邀请", "Subscriptions & referrals"), icon: Gift }, { id: "recharges", label: text("充值审核", "Recharge approval"), icon: Wallet }, { id: "codes", label: text("兑换码", "Redemption codes"), icon: Ticket }, { id: "ledger", label: text("账务核对", "Ledger & reconciliation"), icon: ReceiptText }, { id: "logs", label: text("请求记录", "Request logs"), icon: Activity }, { id: "config", label: text("配置", "Configuration"), icon: Settings2 },
  ];
  return <div className="saas-admin-content"><Heading eyebrow={text("运营控制台", "OPERATIONS CONSOLE")} title={text("SaaS 管理", "SaaS administration")} description={text("用户、访问策略与账务，一个清晰的工作空间。", "Users, access policies, and accounting in one clear workspace.")} action={<Button disabled={site.loading || !site.data?.enabled || !site.data.url} onClick={() => { if (site.data?.url) void openExternal(site.data.url); }}><ExternalLink size={16} />{text("打开首页", "Open homepage")}</Button>} /><nav className="saas-admin-nav" aria-label={text("SaaS 管理", "SaaS administration")}>{tabs.map(item => <Button key={item.id} aria-current={tab === item.id ? "page" : undefined} tone="quiet" onClick={() => { setTab(item.id); if (item.id === "ledger") setLedgerUser(""); }}><item.icon size={16} />{item.label}</Button>)}</nav><div key={`${tab}:${ledgerUser}`} className="saas-admin-panel">{tab === "overview" ? <AdminDashboard navigate={setTab} /> : tab === "users" ? <UsersPanel showLedger={userId => { setLedgerUser(userId); setTab("ledger"); }} /> : tab === "groups" ? <GroupsPanel /> : tab === "growth" ? <GrowthAdminPanel /> : tab === "recharges" ? <RechargesPanel /> : tab === "codes" ? <CodesPanel /> : tab === "ledger" ? <LedgerPanel initialUserId={ledgerUser} /> : tab === "logs" ? <RequestLogs admin query={payload => adminCall<LogResult>("logs.query", payload)} /> : <SettingsContent onConfigChanged={() => { site.reload(); onConfigChanged?.(); }} />}</div></div>;
}

function AdminDashboard({ navigate }: { navigate: (tab: AdminTab) => void }) {
  const { locale, text } = useSaasLocale();
  const overview = useResource(() => adminCall<AdminOverview>("overview"));
  return <><div className="saas-section-heading"><h2>{text("运营总览", "Service overview")}</h2><Button busy={overview.loading} onClick={overview.reload}><RefreshCw size={15} />{text("刷新", "Refresh")}</Button></div>{overview.loading ? <Loading /> : overview.error ? <ErrorState error={overview.error} retry={overview.reload} /> : overview.data && <><div className="saas-metrics"><Metric label={text("用户数", "Users")} value={formatCount(overview.data.userCount, locale)} icon={<Users size={19} />} /><Metric label={text("用户总余额", "Total user balance")} value={formatMoney(overview.data.balanceMicros, locale)} icon={<Wallet size={19} />} /><Metric label={text("今日费用", "Today’s charges")} value={formatMoney(overview.data.today?.costMicros, locale)} icon={<Coins size={19} />} /><Metric label={text("分组数", "Groups")} value={formatCount(overview.data.groupCount, locale)} icon={<Layers3 size={19} />} /></div><div className="saas-billing-grid"><Card title={text("需要你处理", "Needs your attention")}><div className="saas-attention-row"><div><strong>{text("待审核充值", "Pending recharges")}</strong><p className="saas-muted">{text("核实收款，确认到账。", "Verify payments and approve credit.")}</p></div><span className="saas-attention-number">{formatCount(overview.data.pendingRechargeCount, locale)}</span><Button onClick={() => navigate("recharges")}>{text("查看", "Review")}</Button></div><div className="saas-attention-row"><div><strong>{text("待核对请求", "Unreconciled requests")}</strong><p className="saas-muted">{formatMoney(overview.data.pendingReviewMicros, locale)} {text("冻结待确认", "reserved pending verification")}</p></div><span className="saas-attention-number">{formatCount(overview.data.pendingReviewCount, locale)}</span><Button onClick={() => navigate("ledger")}>{text("核对", "Reconcile")}</Button></div></Card><Card title={text("账务健康", "Accounting health")}><dl className="saas-details"><dt>{text("冻结总额", "Reserved funds")}</dt><dd>{formatMoney(overview.data.frozenMicros, locale)}</dd><dt>{text("欠费总额", "Outstanding debt")}</dt><dd>{formatMoney(overview.data.debtMicros, locale)}</dd><dt>{text("本月费用", "This month’s charges")}</dt><dd>{formatMoney(overview.data.month?.costMicros, locale)}</dd></dl><Notice>{text("业务账务独立持久化。请求日志异步保存；驱动故障不会改写已提交账务。", "Accounting is independently persisted. Request logs are asynchronous; a logging-driver failure does not rewrite committed accounting.")}</Notice></Card></div></>}</>;
}
