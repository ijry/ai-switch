import { useState, type FormEvent } from "react";
import { Check, Pencil, Plus, X } from "lucide-react";
import { adminCall } from "../api";
import type { CreatedInviteCodes, InviteCode, InviteReward, PageResult, SubscriptionPlan } from "../types";
import { decimalToInteger, formatDate, formatMoney, integerToDecimal } from "../format";
import { useSaasLocale } from "../i18n";
import { ActionFeedback, Button, Card, CopyButton, Dialog, ErrorState, Field, Loading, Notice, Pagination, Status, Table, useAction, useResource } from "../components/ui";

type PlanDraft = { id?: string; name: string; kind: SubscriptionPlan["kind"]; durationDays: string; quota: string; price: string; status: string };
const emptyPlan: PlanDraft = { name: "", kind: "month", durationDays: "30", quota: "", price: "", status: "active" };

export function GrowthAdminPanel() {
  const { locale, text } = useSaasLocale();
  const plans = useResource(() => adminCall<PageResult<SubscriptionPlan>>("subscriptions.plans.list", { page: 1, pageSize: 100 }));
  const inviteCodes = useResource(() => adminCall<PageResult<InviteCode>>("invites.codes.list", { page: 1, pageSize: 50 }));
  const rewards = useResource(() => adminCall<PageResult<InviteReward>>("invites.rewards.list", { status: "pending", page: 1, pageSize: 100 }));
  const [planDraft, setPlanDraft] = useState<PlanDraft | null>(null);
  const [inviteOpen, setInviteOpen] = useState(false);
  const [inviteDraft, setInviteDraft] = useState({ count: 1, maxUses: "", expires: "" });
  const [inviteSecrets, setInviteSecrets] = useState<CreatedInviteCodes | null>(null);
  const [selectedInvite, setSelectedInvite] = useState<InviteCode | null>(null);
  const action = useAction();

  function savePlan(event: FormEvent) {
    event.preventDefault();
    if (!planDraft) return;
    void action.run(() => adminCall("subscriptions.plans.save", {
      ...(planDraft.id ? { id: planDraft.id } : {}),
      name: planDraft.name.trim(),
      kind: planDraft.kind,
      durationDays: Number(planDraft.durationDays),
      quotaMicros: decimalToInteger(planDraft.quota),
      priceMicros: decimalToInteger(planDraft.price),
      status: planDraft.status,
    }), () => { setPlanDraft(null); plans.reload(); });
  }

  function createInviteCodes(event: FormEvent) {
    event.preventDefault();
    void action.run(() => adminCall<CreatedInviteCodes>("invites.codes.create", {
      count: inviteDraft.count,
      maxUses: inviteDraft.maxUses ? Number(inviteDraft.maxUses) : null,
      expiresAt: inviteDraft.expires ? new Date(inviteDraft.expires).toISOString() : null,
    }), result => { setInviteOpen(false); setInviteSecrets(result); inviteCodes.reload(); });
  }

  function review(id: string, status: string) {
    void action.run(() => adminCall("invites.rewards.review", { id, status, reason: status === "approved" ? "Approved by administrator" : "Rejected by administrator" }), rewards.reload);
  }

  function disableInvite(code: InviteCode) {
    void action.run(() => adminCall("invites.codes.disable", { id: code.id }), () => { setSelectedInvite(null); inviteCodes.reload(); });
  }

  return <div className="saas-stack">
    <Card title={text("订阅套餐", "Subscription plans")} action={<Button tone="primary" onClick={() => setPlanDraft(emptyPlan)}><Plus size={15} />{text("添加套餐", "Add plan")}</Button>}>
      {plans.loading ? <Loading /> : plans.error ? <ErrorState error={plans.error} retry={plans.reload} /> : <Table items={plans.data?.items || []} rowKey={plan => plan.id} columns={[
        { label: text("套餐", "Plan"), render: plan => <><strong>{plan.name}</strong><small>{plan.kind} · {plan.durationDays} {text("天", "days")}</small></> },
        { label: text("每日额度", "Daily quota"), render: plan => formatMoney(plan.dailyQuotaMicros, locale) },
        { label: text("售价", "Price"), render: plan => formatMoney(plan.priceMicros, locale) },
        { label: text("状态", "Status"), render: plan => <Status value={plan.status} /> },
        { label: text("操作", "Actions"), render: plan => <Button onClick={() => setPlanDraft({ id: plan.id, name: plan.name, kind: plan.kind, durationDays: String(plan.durationDays), quota: integerToDecimal(plan.dailyQuotaMicros), price: integerToDecimal(plan.priceMicros), status: plan.status })}><Pencil size={14} />{text("编辑", "Edit")}</Button> },
      ]} />}
    </Card>

    <Card title={text("邀请码", "Invite codes")} action={<Button tone="primary" onClick={() => setInviteOpen(true)}><Plus size={15} />{text("生成邀请码", "Generate codes")}</Button>}>
      {inviteCodes.loading ? <Loading /> : inviteCodes.error ? <ErrorState error={inviteCodes.error} retry={inviteCodes.reload} /> : <Table items={inviteCodes.data?.items || []} rowKey={code => code.id} columns={[
        { label: text("脱敏标识", "Masked code"), render: code => <code>{code.prefix}…{code.suffix}</code> },
        { label: text("使用次数", "Uses"), render: code => String(code.usedCount) + "/" + String(code.maxUses ?? "∞") },
        { label: text("有效期", "Expires"), render: code => code.expiresAt ? formatDate(code.expiresAt, locale) : text("永不过期", "Never") },
        { label: text("状态", "Status"), render: code => <Status value={code.status === "active" && code.expiresAt && Date.parse(code.expiresAt) <= Date.now() ? "expired" : code.status} /> },
        { label: text("操作", "Actions"), render: code => code.status === "active" ? <Button tone="danger" onClick={() => { setSelectedInvite(code); action.clear(); }}>{text("停用", "Disable")}</Button> : "—" },
      ]} />}
      <Pagination page={1} total={inviteCodes.data?.total ?? 0} onPage={() => inviteCodes.reload()} />
    </Card>

    <Card title={text("待审核邀请奖励", "Pending referral rewards")}>
      {rewards.loading ? <Loading /> : rewards.error ? <ErrorState error={rewards.error} retry={rewards.reload} /> : <Table items={rewards.data?.items || []} rowKey={reward => reward.id} columns={[
        { label: text("类型 / 时间", "Type / created"), render: reward => <>{reward.kind}<small>{formatDate(reward.createdAt, locale)}</small></> },
        { label: text("邀请人 / 被邀请人", "Inviter / invitee"), render: reward => <code>{reward.inviterId} → {reward.inviteeId}</code> },
        { label: text("奖励", "Reward"), render: reward => formatMoney(reward.amountMicros, locale) },
        { label: text("操作", "Actions"), render: reward => <div className="saas-row"><Button tone="primary" onClick={() => review(reward.id, "approved")}><Check size={14} />{text("通过", "Approve")}</Button><Button tone="danger" onClick={() => review(reward.id, "rejected")}><X size={14} />{text("拒绝", "Reject")}</Button></div> },
      ]} />}
    </Card>

    {planDraft && <Dialog title={planDraft.id ? text("编辑订阅套餐", "Edit subscription plan") : text("添加订阅套餐", "Add subscription plan")} onClose={() => setPlanDraft(null)} busy={action.busy}>
      <form className="saas-form" onSubmit={savePlan}>
        <Field label={text("名称", "Name")}><input required value={planDraft.name} onChange={event => setPlanDraft({ ...planDraft, name: event.target.value })} /></Field>
        <div className="saas-form-grid">
          <Field label={text("类型", "Type")}><select value={planDraft.kind} onChange={event => setPlanDraft({ ...planDraft, kind: event.target.value as SubscriptionPlan["kind"] })}>{([["trial", "体验卡"], ["day", "日卡"], ["week", "周卡"], ["month", "月卡"], ["quarter", "季度卡"], ["year", "年卡"]] as const).map(([value, label]) => <option key={value} value={value}>{text(label, value)}</option>)}</select></Field>
          <Field label={text("有效天数", "Duration days")}><input type="number" min={1} max={3660} required value={planDraft.durationDays} onChange={event => setPlanDraft({ ...planDraft, durationDays: event.target.value })} /></Field>
          <Field label={text("每日额度 USD", "Daily quota USD")}><input required inputMode="decimal" value={planDraft.quota} onChange={event => setPlanDraft({ ...planDraft, quota: event.target.value })} /></Field>
          <Field label={text("售价 USD", "Price USD")}><input required inputMode="decimal" value={planDraft.price} onChange={event => setPlanDraft({ ...planDraft, price: event.target.value })} /></Field>
          <Field label={text("状态", "Status")}><select value={planDraft.status} onChange={event => setPlanDraft({ ...planDraft, status: event.target.value })}><option value="active">{text("启用", "Active")}</option><option value="disabled">{text("停用", "Disabled")}</option></select></Field>
        </div>
        <Notice>{text("额度按 UTC 自然日重置；有效期只控制订阅可用时间。", "Quota resets each UTC day; the duration only controls validity.")}</Notice>
        <ActionFeedback action={action} />
        <div className="saas-actions"><Button onClick={() => setPlanDraft(null)}>{text("取消", "Cancel")}</Button><Button tone="primary" type="submit" busy={action.busy}>{text("保存", "Save")}</Button></div>
      </form>
    </Dialog>}

    {inviteOpen && <Dialog title={text("生成邀请码", "Generate invite codes")} onClose={() => setInviteOpen(false)} busy={action.busy}>
      <form className="saas-form" onSubmit={createInviteCodes}>
        <div className="saas-form-grid">
          <Field label={text("数量", "Quantity")}><input type="number" min={1} max={200} required value={inviteDraft.count} onChange={event => setInviteDraft({ ...inviteDraft, count: Number(event.target.value) })} /></Field>
          <Field label={text("单码可用次数", "Uses per code")} hint={text("留空表示不限次数。", "Leave empty for unlimited uses.")}><input type="number" min={1} value={inviteDraft.maxUses} onChange={event => setInviteDraft({ ...inviteDraft, maxUses: event.target.value })} /></Field>
        </div>
        <Field label={text("有效期（本地时间）", "Expiration (local time)")}><input type="datetime-local" value={inviteDraft.expires} onChange={event => setInviteDraft({ ...inviteDraft, expires: event.target.value })} /></Field>
        <ActionFeedback action={action} />
        <div className="saas-actions"><Button onClick={() => setInviteOpen(false)}>{text("取消", "Cancel")}</Button><Button tone="primary" type="submit" busy={action.busy}>{text("生成", "Generate")}</Button></div>
      </form>
    </Dialog>}

    {inviteSecrets && <Dialog title={text("保存邀请码", "Save invite codes")} onClose={() => setInviteSecrets(null)}>
      <Notice tone="warning">{text("明文仅显示一次，请立即保存。", "Plaintext codes are shown only once. Save them now.")}</Notice>
      <pre className="saas-secret"><code>{inviteSecrets.items.map(code => code.code).join("\n")}</code></pre>
      <div className="saas-actions"><CopyButton value={inviteSecrets.items.map(code => code.code).join("\n")} /><Button tone="primary" onClick={() => setInviteSecrets(null)}>{text("我已保存", "Saved")}</Button></div>
    </Dialog>}

    {selectedInvite && <Dialog title={text("停用邀请码", "Disable invite code")} onClose={() => setSelectedInvite(null)} busy={action.busy}>
      <p><code>{selectedInvite.prefix}…{selectedInvite.suffix}</code></p>
      <Notice tone="warning">{text("停用后不能再用于 GitHub 注册。", "Disabled codes cannot be used for GitHub registration.")}</Notice>
      <ActionFeedback action={action} />
      <div className="saas-actions"><Button onClick={() => setSelectedInvite(null)}>{text("取消", "Cancel")}</Button><Button tone="danger" busy={action.busy} onClick={() => disableInvite(selectedInvite)}>{text("确认停用", "Confirm disable")}</Button></div>
    </Dialog>}
    <ActionFeedback action={action} />
  </div>;
}
