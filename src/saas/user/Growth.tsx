import { useState } from "react";
import { CalendarCheck, Gift, KeyRound, Repeat2 } from "lucide-react";
import type { SaasUserClient } from "../api";
import type { ExternalKeyStatus, InviteOverview, InviteReward, PageResult, SaasPublicConfig, SubscriptionPlan, UserSubscription } from "../types";
import { formatDate, formatMoney } from "../format";
import { useSaasLocale } from "../i18n";
import { ActionFeedback, Button, Card, CopyButton, Empty, ErrorState, Heading, Loading, Notice, Status, Table, useAction, useResource } from "../components/ui";

export function GrowthPage({ client, config }: { client: SaasUserClient; config: SaasPublicConfig }) {
  const { locale, text } = useSaasLocale();
  const subscriptions = useResource(() => client.call<PageResult<UserSubscription>>("subscriptions.list"));
  const plans = useResource(() => client.call<PageResult<SubscriptionPlan>>("subscriptions.plans"));
  const invites = useResource(() => client.call<InviteOverview>("invites.overview"));
  const rewards = useResource(() => client.call<PageResult<InviteReward>>("invites.rewards", { page: 1, pageSize: 50 }));
  const action = useAction();
  function purchase(planId: string) { void action.run(() => client.call("subscriptions.purchase", { planId, requestId: crypto.randomUUID() }), () => { subscriptions.reload(); plans.reload(); }); }
  function checkin() { void action.run(() => client.call("checkin"), () => subscriptions.reload()); }
  return <><Heading eyebrow={text("权益中心", "BENEFITS")} title={text("订阅与奖励", "Subscriptions & rewards")} description={text("订阅优先抵扣，每日额度于 UTC 00:00 重置，多份订阅按最早到期顺序使用。", "Subscriptions are charged first; daily quota resets at 00:00 UTC, earliest expiry first.")} />
    <ActionFeedback action={action} />
    {config.checkinEnabled && <Card title={<span className="saas-row"><CalendarCheck size={18} />{text("每日签到", "Daily check-in")}</span>} action={<Button tone="primary" busy={action.busy} onClick={checkin}>{text("立即签到", "Check in")}</Button>}><p>{text("每日可获得", "Earn daily")} {formatMoney(config.checkinRewardMicros, locale)}。</p></Card>}
    <Card title={text("我的订阅", "My subscriptions")}>{subscriptions.loading ? <Loading /> : subscriptions.error ? <ErrorState error={subscriptions.error} retry={subscriptions.reload} /> : subscriptions.data?.items.length ? <Table items={subscriptions.data.items} rowKey={item => item.id} columns={[{ label:text("套餐", "Plan"), render:item => <><strong>{item.planName}</strong><small>{formatDate(item.expiresAt, locale)}</small></> },{ label:text("可用额度", "Available"), render:item => formatMoney(item.availableMicros, locale) },{ label:text("状态", "Status"), render:item => <Status value={item.status} /> }]} /> : <Empty title={text("暂无订阅", "No subscriptions")} description={text("购买套餐或使用订阅兑换码后会显示在这里。", "Purchased and redeemed plans appear here.")} />}</Card>
    <Card title={text("订阅套餐", "Subscription plans")}>{plans.loading ? <Loading /> : plans.error ? <ErrorState error={plans.error} retry={plans.reload} /> : <div className="saas-group-grid">{plans.data?.items.map(plan => <Card key={plan.id}><h3>{plan.name}</h3><p>{plan.durationDays} {text("天", "days")} · {text("每日", "daily")} {formatMoney(plan.dailyQuotaMicros, locale)}</p><strong>{formatMoney(plan.priceMicros, locale)}</strong><Button onClick={() => purchase(plan.id)} busy={action.busy}>{text("余额购买", "Buy with balance")}</Button></Card>)}</div>}</Card>
    {config.inviteEnabled && <><Card title={<span className="saas-row"><Gift size={18} />{text("邀请奖励", "Referral rewards")}</span>}>{invites.loading ? <Loading /> : invites.error ? <ErrorState error={invites.error} retry={invites.reload} /> : <><p>{text("你的推荐码", "Your referral code")}: <code>{invites.data?.referralCode}</code> <CopyButton value={invites.data?.referralCode || ""} /></p><p>{text("待结算", "Pending")} {formatMoney(invites.data?.pendingMicros, locale)} · {text("已结算", "Settled")} {formatMoney(invites.data?.settledMicros, locale)}</p><Notice>{text("邀请奖励需管理员审核后才会进入余额。", "Referral rewards enter your balance only after administrator approval.")}</Notice></>}</Card><Card title={text("奖励明细", "Reward history")}>{rewards.loading ? <Loading /> : rewards.error ? <ErrorState error={rewards.error} retry={rewards.reload} /> : <Table items={rewards.data?.items || []} rowKey={item => item.id} columns={[{ label:text("类型", "Type"), render:item => item.kind },{ label:text("奖励", "Reward"), render:item => formatMoney(item.amountMicros, locale) },{ label:text("状态", "Status"), render:item => <Status value={item.status} /> },{ label:text("时间", "Created"), render:item => formatDate(item.createdAt, locale) }]} />}</Card></>}
  </>;
}

export function AccountApiKeyCard({ client }: { client: SaasUserClient }) {
  const { text } = useSaasLocale();
  const key = useResource(() => client.call<ExternalKeyStatus>("external-key.status"));
  const [secret, setSecret] = useState("");
  const action = useAction();
  function rotate() { void action.run(() => client.call<ExternalKeyStatus>("external-key.rotate"), result => { setSecret(result.plaintextKey || ""); key.reload(); }); }
  function revoke() { void action.run(() => client.call("external-key.revoke"), () => { setSecret(""); key.reload(); }); }
  return <Card title={<span className="saas-row"><KeyRound size={18} />{text("账户查询安全密钥", "Account query security key")}</span>}><p className="saas-muted">{text("用于兼容 sub2api 的 /v1/usage 与 /v1/subscriptions；仅保存摘要。", "Used by the sub2api-compatible /v1/usage and /v1/subscriptions endpoints; only a digest is stored.")}</p>{key.loading ? <Loading /> : key.error ? <ErrorState error={key.error} retry={key.reload} /> : <p>{key.data?.configured ? <><Status value="active" /> <code>{key.data.prefix}…</code></> : text("尚未创建", "Not created")}</p>}{secret && <Notice tone="warning">{text("明文仅显示一次：", "Shown once: ")}<code>{secret}</code> <CopyButton value={secret} /></Notice>}<ActionFeedback action={action} /><div className="saas-actions"><Button tone="primary" onClick={rotate} busy={action.busy}><Repeat2 size={15} />{key.data?.configured ? text("轮换密钥", "Rotate key") : text("创建密钥", "Create key")}</Button>{key.data?.configured && <Button tone="danger" onClick={revoke}>{text("撤销", "Revoke")}</Button>}</div></Card>;
}
