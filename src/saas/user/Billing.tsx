import { useRef, useState, type FormEvent } from "react";
import { Gift, Wallet } from "lucide-react";
import type { SaasUserClient } from "../api";
import type { PageResult, RechargeOrder, RedemptionResult, SaasPublicConfig, UserOverview } from "../types";
import { decimalToInteger, estimateRecharge, formatDate, formatMoney, integerToDecimal } from "../format";
import { useSaasLocale } from "../i18n";
import { ActionFeedback, Button, Card, Dialog, Empty, ErrorState, Field, Heading, Loading, Notice, Pagination, Status, Table, useAction, useResource } from "../components/ui";

export function RechargeDetails({ order }: { order: RechargeOrder }) {
  const { locale, text } = useSaasLocale();
  return <dl className="saas-details">
    <dt>{text("订单编号", "Order ID")}</dt><dd className="saas-mono">{order.id}</dd>
    <dt>{text("用户", "User")}</dt><dd>{order.githubLogin || order.userId}</dd>
    <dt>{text("人民币金额", "CNY amount")}</dt><dd>{formatMoney(order.amountCnyFen,locale,"CNY")}</dd>
    <dt>{text("汇率快照", "Exchange-rate snapshot")}</dt><dd>1 USD = {integerToDecimal(order.exchangeRateMicros)} CNY</dd>
    <dt>{text("到账金额", "Credit amount")}</dt><dd><strong>{formatMoney(order.creditMicros,locale)}</strong></dd>
    <dt>{text("状态", "Status")}</dt><dd><Status value={order.status} /></dd>
    <dt>{text("创建时间", "Created")}</dt><dd>{formatDate(order.createdAt,locale)}</dd>
    <dt>{text("付款备注", "Payment note")}</dt><dd className="saas-preserve">{order.note || "—"}</dd>
    <dt>{text("审核说明", "Review reason")}</dt><dd className="saas-preserve">{order.reason || order.reviewNote || "—"}</dd>
  </dl>;
}

export function RechargePage({ client, config }: { client: SaasUserClient; config: SaasPublicConfig }) {
  const { locale, text } = useSaasLocale();
  const [amount, setAmount] = useState("");
  const [note, setNote] = useState("");
  const [page, setPage] = useState(1);
  const [detail, setDetail] = useState<RechargeOrder | null>(null);
  const [cancel, setCancel] = useState<RechargeOrder | null>(null);
  const request = useRef<{ signature: string; id: string } | null>(null);
  const orders = useResource(() => client.call<PageResult<RechargeOrder>>("recharges.list",{ page,pageSize:20 }),[page]);
  const action = useAction();
  const cancelAction = useAction();
  let estimate: number | undefined;
  try { estimate = estimateRecharge(decimalToInteger(amount,2),config.exchangeRateMicros); } catch { estimate = undefined; }
  function submit(event: FormEvent) {
    event.preventDefault();
    void action.run(() => {
      const amountCnyFen = decimalToInteger(amount,2);
      if (amountCnyFen<=0) throw new Error(text("充值金额必须大于零。", "The recharge amount must be greater than zero."));
      const signature = JSON.stringify([amountCnyFen,note.trim()]);
      if (request.current?.signature!==signature) request.current = { signature,id:crypto.randomUUID() };
      return client.call<RechargeOrder>("recharges.create",{ requestId:request.current.id,amountCnyFen,note:note.trim() });
    }, order => {
      request.current=null; setAmount(""); setNote(""); setDetail(order); orders.reload();
      action.setSuccess(text("申请已提交，待管理员确认收款后入账。", "Request submitted. Credit is added after your payment is verified."));
    });
  }
  return <>
    <Heading eyebrow={text("为下一次构建蓄能", "FUEL YOUR NEXT BUILD")} title={text("账户充值", "Add credit")} description={text("人民币支付，美元记账。每笔订单都有独立汇率快照。", "Pay in CNY, spend in USD. Every order locks its exchange rate.")} />
    <div className="saas-billing-grid"><Card title={text("人工充值申请", "Manual recharge request")}><form className="saas-form" onSubmit={submit}>
      <Field label={text("充值金额 (CNY)", "Recharge amount (CNY)")}><input required inputMode="decimal" value={amount} onChange={event=>setAmount(event.target.value)} /></Field>
      <div className="saas-conversion"><span>{text("预计到账", "Estimated credit")}</span><strong>{formatMoney(estimate,locale)}</strong><small>1 USD = {integerToDecimal(config.exchangeRateMicros)} CNY</small></div>
      <Field label={text("付款备注 / 交易参考号", "Payment note / transfer reference")}><textarea maxLength={1000} rows={3} value={note} onChange={event=>setNote(event.target.value)} /></Field>
      <Notice>{text("提交申请不会自动扣款。请与管理员确认付款方式，收到款项后才能入账。", "Submitting does not charge a payment method. Confirm payment instructions with the administrator; credit requires payment verification.")}</Notice>
      <ActionFeedback action={action} /><Button tone="primary" type="submit" busy={action.busy} disabled={estimate==null || estimate<=0}><Wallet size={16} />{text("提交充值申请", "Submit recharge request")}</Button>
    </form></Card><Card title={text("充值说明", "How it works")}><ol className="saas-steps">
      <li><span>01</span><div><strong>{text("提交金额与备注", "Submit amount and note")}</strong><p>{text("系统保存汇率和美元到账金额。", "The server records the exchange rate and USD credit.")}</p></div></li>
      <li><span>02</span><div><strong>{text("联系管理员完成付款", "Arrange payment with the administrator")}</strong><p>{config.rechargeInstructions || text("付款时注明订单编号，方便核对。", "Include the order ID with your payment.")}</p></div></li>
      <li><span>03</span><div><strong>{text("审核后到账", "Receive credit after approval")}</strong><p>{text("审核状态和到账金额可在下方查看。", "Track approval status and credited amounts below.")}</p></div></li>
    </ol></Card></div>
    <Card title={text("充值记录", "Recharge history")}>{orders.loading ? <Loading /> : orders.error ? <ErrorState error={orders.error} retry={orders.reload} /> : orders.data && <>
      <Table items={orders.data.items} rowKey={order=>order.id} columns={[
        { label:text("订单 / 时间","Order / date"),render:order=><><code>{order.id}</code><small>{formatDate(order.createdAt,locale)}</small></> },
        { label:text("人民币 / 美元","CNY / USD"),render:order=><>{formatMoney(order.amountCnyFen,locale,"CNY")} / {formatMoney(order.creditMicros,locale)}</> },
        { label:text("状态","Status"),render:order=><Status value={order.status} /> },
        { label:text("操作","Actions"),render:order=><div className="saas-actions"><Button onClick={()=>setDetail(order)}>{text("详情","Details")}</Button>{order.status==="pending" && <Button onClick={()=>{cancelAction.clear();setCancel(order);}}>{text("取消申请","Cancel request")}</Button>}</div> },
      ]} /><Pagination page={page} total={orders.data.total} onPage={setPage} />
    </>}</Card>
    {detail && <Dialog title={text("充值详情","Recharge details")} onClose={()=>setDetail(null)}><RechargeDetails order={detail} /><Button onClick={()=>setDetail(null)}>{text("完成","Done")}</Button></Dialog>}
    {cancel && <Dialog title={text("取消待审核申请","Cancel pending request")} busy={cancelAction.busy} onClose={()=>setCancel(null)}><RechargeDetails order={cancel} /><Notice tone="warning">{text("已付款请先联系管理员，不要直接取消申请。","Contact the administrator before cancelling an order you have already paid.")}</Notice><ActionFeedback action={cancelAction} /><Button tone="danger" busy={cancelAction.busy} onClick={()=>void cancelAction.run(()=>client.call("recharges.cancel",{ id:cancel.id }),()=>{setCancel(null);orders.reload();})}>{text("确认取消","Confirm cancellation")}</Button></Dialog>}
  </>;
}

export function RedeemPage({ client }: { client: SaasUserClient }) {
  const { locale,text } = useSaasLocale();
  const [code,setCode] = useState("");
  const overview = useResource(()=>client.call<UserOverview>("overview"));
  const action = useAction();
  function submit(event:FormEvent) {
    event.preventDefault();
    void action.run(()=>client.call<RedemptionResult>("redeem",{ code:code.trim() }),result=>{
      setCode(""); overview.reload(); action.setSuccess(result.subscription ? text("兑换成功，已开通订阅 ","Redeemed. Subscription activated: ") + result.subscription.planName : text("兑换成功，到账 ","Redeemed. Credited ") + formatMoney(result.amountMicros, locale));
    });
  }
  return <><Heading eyebrow={text("兑换额度","REDEEM CREDIT")} title={text("兑换码","Redemption codes")} description={text("每个兑换码只能使用一次；余额码立即更新余额，订阅码立即开通套餐。","Each code can be redeemed once. Balance codes update your wallet; subscription codes activate a plan.")} />
    <Card><form className="saas-form" onSubmit={submit}><Field label={text("兑换码","Redemption code")}><input required maxLength={200} autoComplete="off" value={code} onChange={event=>setCode(event.target.value)} /></Field><ActionFeedback action={action} /><Button tone="primary" type="submit" busy={action.busy} disabled={!code.trim()}><Gift size={16} />{text("立即兑换","Redeem now")}</Button></form></Card>
    <Card title={text("兑换记录","Redemption history")}>{overview.loading ? <Loading /> : overview.error ? <ErrorState error={overview.error} retry={overview.reload} /> : <Table items={overview.data?.redemptionHistory || []} rowKey={entry=>entry.id} empty={<Empty />} columns={[
      { label:text("时间","Date"),render:entry=>formatDate(entry.createdAt,locale) },
      { label:text("到账金额","Credit"),render:entry=>formatMoney(entry.amountMicros,locale) },
      { label:text("流水编号","Ledger ID"),render:entry=><code>{entry.id}</code> },
    ]} />}</Card></>;
}
