import { useState, type FormEvent } from "react";
import { Download, Plus } from "lucide-react";
import { adminCall } from "../api";
import type { CreatedCodes, PageResult, RedemptionCode, SubscriptionPlan } from "../types";
import { decimalToInteger, downloadText, formatDate, formatMoney } from "../format";
import { useSaasLocale } from "../i18n";
import { ActionFeedback, Button, Card, CopyButton, Dialog, ErrorState, Field, Loading, Notice, Pagination, Status, Table, useAction, useResource } from "../components/ui";

export function CodesPanel() {
  const { locale, text } = useSaasLocale();
  const [page, setPage] = useState(1);
  const [draft, setDraft] = useState({ status: "", batchId: "" });
  const [filters, setFilters] = useState(draft);
  const codes = useResource(() => adminCall<PageResult<RedemptionCode>>("codes.list", { ...(filters.status ? { status: filters.status } : {}), ...(filters.batchId ? { batchId: filters.batchId } : {}), page, pageSize: 20 }), [filters, page]);
  const plans = useResource(() => adminCall<PageResult<SubscriptionPlan>>("subscriptions.plans.list", { page: 1, pageSize: 200 }), []);
  const [creating, setCreating] = useState(false);
  const [secrets, setSecrets] = useState<CreatedCodes | null>(null);
  const [selected, setSelected] = useState<RedemptionCode | null>(null);
  const action = useAction();
  const plaintext = secrets?.items.map(code => code.plaintextCode).join("\n") || "";
  const planName = (planId?: string | null) => plans.data?.items.find(plan => plan.id === planId)?.name || planId || "";

  return <Card title={text("兑换码", "Redemption codes")} action={<Button tone="primary" onClick={() => setCreating(true)}><Plus size={16} />{text("生成兑换码", "Generate codes")}</Button>}>
    <form className="saas-filters" onSubmit={event => { event.preventDefault(); setFilters(draft); setPage(1); codes.reload(); }}>
      <Field label={text("状态", "Status")}><select value={draft.status} onChange={event => setDraft({ ...draft, status: event.target.value })}><option value="">{text("全部", "All")}</option><option value="active">{text("未使用", "Active")}</option><option value="redeemed">{text("已兑换", "Redeemed")}</option><option value="disabled">{text("停用", "Disabled")}</option></select></Field>
      <Field label={text("生成批次 ID", "Generation batch ID")}><input value={draft.batchId} onChange={event => setDraft({ ...draft, batchId: event.target.value })} /></Field>
      <Button type="submit" busy={codes.loading}>{text("查询", "Search")}</Button>
    </form>
    {codes.loading ? <Loading /> : codes.error ? <ErrorState error={codes.error} retry={codes.reload} /> : codes.data && <>
      <Table items={codes.data.items} rowKey={code => code.id} columns={[
        { label: text("脱敏标识 / 批次", "Masked code / batch"), render: code => <><code>{code.prefix}…{code.suffix}</code><small className="saas-mono saas-muted">{code.batchId}</small></> },
        { label: text("内容", "Content"), render: code => code.subscriptionPlanId ? planName(code.subscriptionPlanId) : formatMoney(code.amountMicros, locale) },
        { label: text("有效期", "Expires"), render: code => code.expiresAt ? formatDate(code.expiresAt, locale) : text("永不过期", "Never") },
        { label: text("状态 / 使用者", "Status / redeemed by"), render: code => <><Status value={code.status === "active" && code.expiresAt && Date.parse(code.expiresAt) <= Date.now() ? "expired" : code.status} />{code.usedBy && <small>{code.usedBy} · {formatDate(code.usedAt, locale)}</small>}</> },
        { label: text("操作", "Actions"), render: code => code.status === "active" ? <Button tone="danger" onClick={() => { setSelected(code); action.clear(); }}>{text("停用", "Disable")}</Button> : "—" },
      ]} />
      <Pagination page={page} total={codes.data.total} onPage={setPage} />
    </>}
    {creating && <CodeCreator plans={plans.data?.items || []} onClose={() => setCreating(false)} onCreated={result => { setCreating(false); setSecrets(result); codes.reload(); }} />}
    {secrets && <Dialog title={text("保存本批次兑换码", "Save this code batch")} onClose={() => setSecrets(null)}>
      <Notice tone="warning">{text("明文仅展示一次。关闭后只能查询脱敏标识，无法恢复或重新导出。", "Plaintext codes are shown once. After closing, only masked identifiers remain; the codes cannot be recovered or exported again.")}</Notice>
      <p>{text("批次：", "Batch: ")}<code>{secrets.batchId}</code></p>
      <pre className="saas-secret"><code>{plaintext}</code></pre>
      <div className="saas-row saas-wrap"><CopyButton value={plaintext} /><Button onClick={() => void action.run(async () => downloadText("saas-codes-" + secrets.batchId + ".txt", plaintext))}><Download size={16} />{text("导出明文", "Export plaintext")}</Button></div>
      <ActionFeedback action={action} />
      <div className="saas-actions"><Button tone="primary" onClick={() => setSecrets(null)}>{text("我已保存，关闭", "I’ve saved them — close")}</Button></div>
    </Dialog>}
    {selected && <Dialog title={text("停用兑换码", "Disable redemption code")} onClose={() => setSelected(null)} busy={action.busy}>
      <p><code>{selected.prefix}…{selected.suffix}</code> · {selected.subscriptionPlanId ? planName(selected.subscriptionPlanId) : formatMoney(selected.amountMicros, locale)}</p>
      <Notice tone="warning">{text("停用后不可兑换，请确认此码尚未发给用户。", "This code will no longer be redeemable. Verify whether it has already been distributed.")}</Notice>
      <ActionFeedback action={action} />
      <div className="saas-actions"><Button onClick={() => setSelected(null)} disabled={action.busy}>{text("取消", "Cancel")}</Button><Button tone="danger" busy={action.busy} onClick={() => void action.run(() => adminCall("codes.disable", { id: selected.id }), () => { setSelected(null); codes.reload(); })}>{text("确认停用", "Confirm disable")}</Button></div>
    </Dialog>}
  </Card>;
}

function CodeCreator({ plans, onClose, onCreated }: { plans: SubscriptionPlan[]; onClose: () => void; onCreated: (result: CreatedCodes) => void }) {
  const { locale, text } = useSaasLocale();
  const [type, setType] = useState<"balance" | "subscription">("balance");
  const [amount, setAmount] = useState("");
  const [planId, setPlanId] = useState("");
  const [count, setCount] = useState(1);
  const [expires, setExpires] = useState("");
  const action = useAction();
  let amountMicros: number | undefined;
  try { amountMicros = decimalToInteger(amount); } catch {}

  function create(event: FormEvent) {
    event.preventDefault();
    void action.run(async () => {
      const credit = type === "balance" ? decimalToInteger(amount) : 0;
      if (type === "balance" && credit <= 0) throw new Error(text("面额必须大于零。", "Credit must be positive."));
      if (type === "subscription" && !planId) throw new Error(text("请选择订阅套餐。", "Select a subscription plan."));
      if (expires && Date.parse(expires) <= Date.now()) throw new Error(text("有效期必须晚于现在。", "Expiration must be in the future."));
      return adminCall<CreatedCodes>("codes.create", { count, amountMicros: credit, subscriptionPlanId: type === "subscription" ? planId : null, expiresAt: expires ? new Date(expires).toISOString() : null });
    }, onCreated);
  }

  return <Dialog title={text("生成兑换码", "Generate redemption codes")} onClose={onClose} busy={action.busy}>
    <form onSubmit={create} className="saas-form">
      <div className="saas-form-grid">
        <Field label={text("兑换类型", "Redemption type")}><select value={type} onChange={event => setType(event.target.value as "balance" | "subscription")}><option value="balance">{text("余额", "Balance")}</option><option value="subscription">{text("订阅套餐", "Subscription plan")}</option></select></Field>
        {type === "balance"
          ? <Field label={text("单码面额 (USD)", "Credit per code (USD)")}><input inputMode="decimal" required value={amount} onChange={event => setAmount(event.target.value)} /></Field>
          : <Field label={text("订阅套餐", "Subscription plan")}><select required value={planId} onChange={event => setPlanId(event.target.value)}><option value="">{text("请选择", "Select")}</option>{plans.map(plan => <option key={plan.id} value={plan.id}>{plan.name} · {text("每日", "daily")} {formatMoney(plan.dailyQuotaMicros, locale)}</option>)}</select></Field>}
      </div>
      <Field label={text("数量（1–100）", "Quantity (1–100)")}><input type="number" min={1} max={100} required value={count} onChange={event => setCount(Number(event.target.value))} /></Field>
      <Field label={text("有效期（本地时间）", "Expiration (local time)")}><input type="datetime-local" value={expires} onChange={event => setExpires(event.target.value)} /></Field>
      <Notice>{type === "balance" ? count + " × " + formatMoney(amountMicros, locale) : count + " × " + (plans.find(plan => plan.id === planId)?.name || text("订阅套餐", "Subscription plan"))} · {text("生成不代表已入账，用户成功兑换后才入账。", "Generating codes does not credit a balance. Credit is posted only after successful redemption.")}</Notice>
      <ActionFeedback action={action} />
      <div className="saas-actions"><Button onClick={onClose} disabled={action.busy}>{text("取消", "Cancel")}</Button><Button tone="primary" type="submit" busy={action.busy}>{text("确认生成", "Confirm generation")}</Button></div>
    </form>
  </Dialog>;
}
