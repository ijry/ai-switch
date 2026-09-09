import { useState } from "react";
import { adminCall } from "../api";
import type { PageResult, SaasUser, SubscriptionPlan, UserSubscription } from "../types";
import { formatDate, formatMoney } from "../format";
import { useSaasLocale } from "../i18n";
import { ActionFeedback, Button, Card, Dialog, ErrorState, Field, Loading, Notice, Status, Table, useAction, useResource } from "../components/ui";

export function UserSubscriptions({ user, onClose }: { user: SaasUser; onClose: () => void }) {
  const { locale, text } = useSaasLocale();
  const subscriptions = useResource(() => adminCall<PageResult<UserSubscription>>("subscriptions.list", { userId: user.id }));
  const plans = useResource(() => adminCall<PageResult<SubscriptionPlan>>("subscriptions.plans.list", { page: 1, pageSize: 200 }));
  const [planId, setPlanId] = useState("");
  const [cancelling, setCancelling] = useState<UserSubscription | null>(null);
  const [reason, setReason] = useState("");
  const action = useAction();
  return <Dialog wide title={text("订阅管理", "Manage subscriptions") + " · " + (user.email || user.login)} onClose={onClose} busy={action.busy}>
    <Notice>{text("多个订阅可以并存。每日额度按 UTC 重置；取消只阻止新请求，不退还余额，已预占请求仍正常结算。", "Subscriptions can coexist. Daily quotas reset at UTC midnight. Cancellation blocks new requests without a refund; reserved requests still settle normally.")}</Notice>
    {subscriptions.loading ? <Loading /> : subscriptions.error ? <ErrorState error={subscriptions.error} retry={subscriptions.reload} /> : <Table items={subscriptions.data?.items || []} rowKey={item => item.id} columns={[
      { label: text("套餐 / 来源", "Plan / source"), render: item => <><strong>{item.planName}</strong><small>{item.source}</small></> },
      { label: text("每日额度 / 今日使用", "Daily quota / used today"), render: item => <>{formatMoney(item.dailyQuotaMicros, locale)}<small>{formatMoney(item.todayUsedMicros, locale)} · {text("预占", "reserved")} {formatMoney(item.todayFrozenMicros, locale)}</small></> },
      { label: text("有效期", "Validity"), render: item => <>{formatDate(item.startsAt, locale)}<small>{formatDate(item.expiresAt, locale)}</small></> },
      { label: text("状态", "Status"), render: item => <Status value={item.status === "active" && Date.parse(item.expiresAt) <= Date.now() ? "expired" : item.status} /> },
      { label: text("操作", "Actions"), render: item => <Button tone="danger" disabled={action.busy || item.status !== "active" || Date.parse(item.expiresAt) <= Date.now()} onClick={() => { setCancelling(item); setReason(""); action.clear(); }}>{text("取消订阅", "Cancel subscription")}</Button> },
    ]} />}
    {cancelling ? <Card title={text("确认取消", "Confirm cancellation")}><form className="saas-form" onSubmit={event => { event.preventDefault(); void action.run(() => adminCall("subscriptions.cancel", { userId: user.id, id: cancelling.id, reason: reason.trim() }), () => { setCancelling(null); subscriptions.reload(); }); }}><p>{cancelling.planName}</p><Field label={text("取消原因", "Cancellation reason")}><input required maxLength={1024} value={reason} onChange={event => setReason(event.target.value)} /></Field><div className="saas-actions"><Button disabled={action.busy} onClick={() => setCancelling(null)}>{text("返回", "Back")}</Button><Button type="submit" tone="danger" busy={action.busy} disabled={!reason.trim()}>{text("确认取消订阅", "Confirm cancellation")}</Button></div></form></Card> : <Card title={text("发放订阅", "Grant subscription")}>{plans.loading ? <Loading /> : plans.error ? <ErrorState error={plans.error} retry={plans.reload} /> : <form className="saas-form" onSubmit={event => { event.preventDefault(); void action.run(() => adminCall("subscriptions.grant", { userId: user.id, planId }), () => { setPlanId(""); subscriptions.reload(); }); }}><Field label={text("订阅套餐", "Subscription plan")}><select required value={planId} onChange={event => setPlanId(event.target.value)}><option value="">{text("请选择套餐", "Select a plan")}</option>{plans.data?.items.filter(plan => plan.status === "active").map(plan => <option key={plan.id} value={plan.id}>{plan.name} · {plan.durationDays} {text("天", "days")} · {formatMoney(plan.dailyQuotaMicros, locale)}/{text("日", "day")}</option>)}</select></Field><Notice>{text("后台发放不扣除用户余额，也不计入现金收入。", "Administrator grants do not debit the wallet or count as cash receipts.")}</Notice><Button type="submit" tone="primary" busy={action.busy} disabled={!planId || user.status !== "active"}>{text("确认发放", "Grant subscription")}</Button></form>}</Card>}
    <ActionFeedback action={action} />
  </Dialog>;
}
