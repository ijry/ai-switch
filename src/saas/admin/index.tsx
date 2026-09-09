import { useState } from "react";
import { Activity, Boxes, ExternalLink, Gift, Layers3, ReceiptText, Settings2, Ticket, Users, Wallet } from "lucide-react";
import { adminCall } from "../api";
import type { LogResult, SaasConfig, SaasHostProps } from "../types";
import { getWebServerStatus } from "../../lib/api/client";
import { openExternal } from "../../lib/openExternal";
import { SaasFrame, useSaasLocale } from "../i18n";
import { RequestLogs } from "../components/Logs";
import { Button, Heading, useResource } from "../components/ui";
import { SettingsContent } from "../settings";
import { GroupsPanel } from "./Groups";
import { UsersPanel } from "./Users";
import { CodesPanel } from "./Codes";
import { LedgerPanel } from "./Ledger";
import { RechargesPanel } from "./Recharges";
import { GrowthAdminPanel } from "./Growth";
import { OperationsDashboard } from "./OperationsDashboard";
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
  return <div className="saas-admin-content"><Heading eyebrow={text("运营控制台", "OPERATIONS CONSOLE")} title={text("SaaS 管理", "SaaS administration")} description={text("用户、访问策略与账务，一个清晰的工作空间。", "Users, access policies, and accounting in one clear workspace.")} action={<Button disabled={site.loading || !site.data?.enabled || !site.data.url} onClick={() => { if (site.data?.url) void openExternal(site.data.url); }}><ExternalLink size={16} />{text("打开首页", "Open homepage")}</Button>} /><nav className="saas-admin-nav" aria-label={text("SaaS 管理", "SaaS administration")}>{tabs.map(item => <Button key={item.id} aria-current={tab === item.id ? "page" : undefined} tone="quiet" onClick={() => { setTab(item.id); if (item.id === "ledger") setLedgerUser(""); }}><item.icon size={16} />{item.label}</Button>)}</nav><div key={`${tab}:${ledgerUser}`} className="saas-admin-panel">{tab === "overview" ? <OperationsDashboard navigate={setTab} /> : tab === "users" ? <UsersPanel showLedger={userId => { setLedgerUser(userId); setTab("ledger"); }} /> : tab === "groups" ? <GroupsPanel /> : tab === "growth" ? <GrowthAdminPanel /> : tab === "recharges" ? <RechargesPanel /> : tab === "codes" ? <CodesPanel /> : tab === "ledger" ? <LedgerPanel initialUserId={ledgerUser} /> : tab === "logs" ? <RequestLogs admin query={payload => adminCall<LogResult>("logs.query", payload)} /> : <SettingsContent onConfigChanged={() => { site.reload(); onConfigChanged?.(); }} />}</div></div>;
}
