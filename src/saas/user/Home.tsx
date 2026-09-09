import { useState } from "react";
import { ArrowUpRight, CalendarDays, Code2, Wallet } from "lucide-react";
import type { SaasUserClient } from "../api";
import type { LogResult, PageResult, SaasGroup, SaasPublicConfig, SaasUser, UserOverview } from "../types";
import { formatCount, formatMoney, integerToDecimal } from "../format";
import { useSaasLocale } from "../i18n";
import { LogTable } from "../components/Logs";
import { Button, Card, CopyButton, Empty, ErrorState, Field, Heading, Loading, Metric, Notice, useResource } from "../components/ui";

export function HomePage({ client, config, user, navigate }: { client: SaasUserClient; config: SaasPublicConfig; user: SaasUser; navigate: (path: string) => void }) {
  const { locale, text } = useSaasLocale();
  const overview = useResource(() => client.call<UserOverview>("overview"));
  const logs = useResource(() => client.call<LogResult>("logs.query", { page: 1, pageSize: 5 }));
  return <><Heading eyebrow={text("你的 AI 工作空间", "YOUR AI WORKSPACE")} title={text(`你好，${user.displayName || user.login}`, `Hello, ${user.displayName || user.login}`)} description={text("每次请求，都心中有数。", "A clear view of every request.")} action={<Button tone="primary" onClick={() => navigate("/api-keys")}>{text("管理 API 密钥", "Manage API keys")}<ArrowUpRight size={16} /></Button>} />
    {overview.loading ? <Loading /> : overview.error ? <ErrorState error={overview.error} retry={overview.reload} /> : overview.data && <>
      <div className="saas-metrics saas-metrics-three"><Metric label={text("账户余额 · USD", "Account balance · USD")} value={formatMoney(overview.data.balanceMicros, locale)} detail={<>{text("冻结", "Reserved")} {formatMoney(overview.data.frozenMicros, locale)} · <button type="button" className="saas-text-button" onClick={() => navigate("/recharge")}>{text("充值", "Add credit")} ↗</button></>} icon={<Wallet size={20} />} /><Metric label={text("今日用量", "Today’s usage")} value={formatMoney(overview.data.today?.costMicros, locale)} detail={`${formatCount(overview.data.today?.requestCount, locale)} ${text("次请求", "requests")}`} icon={<Code2 size={20} />} /><Metric label={text("本月用量", "This month")} value={formatMoney(overview.data.month?.costMicros, locale)} detail={`${formatCount(overview.data.month?.requestCount, locale)} ${text("次请求", "requests")}`} icon={<CalendarDays size={20} />} /></div>
      {overview.data.balanceMicros <= overview.data.frozenMicros && <Notice tone="warning">{text("可用余额不足。请充值后再发起新请求；已冻结金额等待结算。", "Your available balance is low. Add credit before making new requests; reserved funds are awaiting settlement.")}</Notice>}
      <div className="saas-home-grid"><Card title={text("最近请求", "Recent activity")} action={<Button tone="quiet" onClick={() => navigate("/usage")}>{text("查看全部", "View all")}<ArrowUpRight size={15} /></Button>}>{logs.loading ? <Loading /> : logs.error ? <ErrorState error={logs.error} retry={logs.reload} /> : <LogTable items={logs.data?.items || []} />}</Card><QuickStart client={client} config={config} navigate={navigate} /></div>
    </>}
  </>;
}

function QuickStart({ client, config, navigate }: { client: SaasUserClient; config: SaasPublicConfig; navigate: (path: string) => void }) {
  const { text } = useSaasLocale();
  const groups = useResource(() => client.call<PageResult<SaasGroup>>("groups"));
  const [selected, setSelected] = useState("");
  const available = groups.data?.items.filter(group => !group.isInternal && group.configured) || [];
  const group = available.find(item => item.id === selected) || available[0];
  const model = group?.models[0]?.model;
  const base = (config.publicBaseUrl || window.location.origin).replace(/\/$/, "");
  const code = model && group ? group.platform === "codex"
    ? `curl '${base}/v1/responses' \\\n  -H "Authorization: Bearer $SAAS_API_KEY" \\\n  -H 'Content-Type: application/json' \\\n  -d '${JSON.stringify({ model, input: "Hello", max_output_tokens: 256 })}'`
    : `curl '${base}/v1/messages' \\\n  -H "x-api-key: $SAAS_API_KEY" \\\n  -H 'anthropic-version: 2023-06-01' \\\n  -H 'Content-Type: application/json' \\\n  -d '${JSON.stringify({ model, max_tokens: 256, messages: [{ role: "user", content: "Hello" }] })}'` : "";
  return <Card title={text("几分钟，开始构建", "Your first request")}><p className="saas-muted">{text("选择分组，创建密钥，再连接你的工具。", "Choose a group, create a key, and connect your tools.")}</p>{groups.loading ? <Loading /> : groups.error ? <ErrorState error={groups.error} retry={groups.reload} /> : !group ? <Empty title={text("暂无可用分组", "No available groups")} description={text("管理员启用分组后即可开始使用。", "You can get started once an administrator enables a group.")} /> : <>
    <Field label={text("接入分组", "Connection group")}><select value={group.id} onChange={event => setSelected(event.target.value)}>{available.map(item => <option key={item.id} value={item.id}>{item.name} · {item.platform === "codex" ? "Codex" : "Claude"}</option>)}</select></Field>
    <div className="saas-code-heading"><span className="saas-mono">cURL · {integerToDecimal(group.multiplierMicros)}×</span><CopyButton value={code} /></div>{code ? <pre className="saas-code"><code>{code}</code></pre> : <Notice tone="warning">{text("此分组尚未配置模型。", "This group has no configured models.")}</Notice>}
    <p className="saas-caption">{text("将自己的密钥设置为 SAAS_API_KEY 环境变量。一个密钥只属于一个分组。", "Set SAAS_API_KEY to your own key. Each key is permanently bound to one group.")}</p><Button onClick={() => navigate("/api-keys")}>{text("创建你的密钥", "Create your key")}<ArrowUpRight size={15} /></Button>
  </>}</Card>;
}
