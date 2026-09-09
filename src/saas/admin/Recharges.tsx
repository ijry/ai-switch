import { useState } from "react";
import { Search } from "lucide-react";
import { adminCall } from "../api";
import type { PageResult, RechargeOrder } from "../types";
import { formatDate, formatMoney, integerToDecimal } from "../format";
import { useSaasLocale } from "../i18n";
import { RechargeDetails } from "../user/Billing";
import { ActionFeedback, Button, Card, Dialog, ErrorState, Field, Loading, Notice, Pagination, Status, Table, useAction, useResource } from "../components/ui";

export function RechargesPanel() {
  const { locale, text } = useSaasLocale();
  const [draft, setDraft] = useState({ userId: "", status: "pending" });
  const [filters, setFilters] = useState(draft);
  const [page, setPage] = useState(1);
  const orders = useResource(() => adminCall<PageResult<RechargeOrder>>("recharges.list", { ...(filters.userId ? { userId: filters.userId } : {}), ...(filters.status ? { status: filters.status } : {}), page, pageSize: 20 }), [filters, page]);
  const [selected, setSelected] = useState<RechargeOrder | null>(null);
  const [reason, setReason] = useState("");
  const action = useAction();
  function review(status: "approved" | "rejected") { if (!selected || !reason.trim()) return; void action.run(() => adminCall("recharges.review", { id: selected.id, status, reason: reason.trim() }), () => { setSelected(null); orders.reload(); }); }
  return <Card title={text("人工充值审核", "Manual recharge approval")}><form className="saas-filters" onSubmit={event => { event.preventDefault(); setFilters(draft); setPage(1); orders.reload(); }}><Field label={text("用户 ID", "User ID")}><input value={draft.userId} onChange={event => setDraft({ ...draft, userId: event.target.value })} /></Field><Field label={text("订单状态", "Order status")}><select value={draft.status} onChange={event => setDraft({ ...draft, status: event.target.value })}><option value="">{text("全部", "All")}</option><option value="pending">{text("待审核", "Pending")}</option><option value="approved">{text("已到账", "Approved")}</option><option value="rejected">{text("已拒绝", "Rejected")}</option><option value="cancelled">{text("已取消", "Cancelled")}</option></select></Field><Button type="submit" busy={orders.loading}><Search size={16} />{text("查询", "Search")}</Button></form>
    {orders.loading ? <Loading /> : orders.error ? <ErrorState error={orders.error} retry={orders.reload} /> : orders.data && <><Table items={orders.data.items} rowKey={order => order.id} columns={[
      { label: text("用户 / 订单", "User / order"), render: order => <><strong>{order.githubLogin || order.userId}</strong><small className="saas-mono">{order.id}</small><small>{formatDate(order.createdAt, locale)}</small></> },
      { label: text("人民币", "CNY payment"), render: order => formatMoney(order.amountCnyFen, locale, "CNY") },
      { label: text("美元 / 汇率快照", "USD / rate snapshot"), render: order => <><strong>{formatMoney(order.creditMicros, locale)}</strong><small>1 USD = {integerToDecimal(order.exchangeRateMicros)} CNY</small></> },
      { label: text("状态", "Status"), render: order => <Status value={order.status} /> },
      { label: text("操作", "Actions"), render: order => <Button onClick={() => { setSelected(order); setReason(""); action.clear(); }}>{order.status === "pending" ? text("审核", "Review") : text("详情", "Details")}</Button> },
    ]} /><Pagination page={page} total={orders.data.total} onPage={setPage} /></>}
    {selected && <Dialog title={text("核实收款与到账金额", "Verify payment & credit")} onClose={() => setSelected(null)} busy={action.busy}><RechargeDetails order={selected} />{selected.status === "pending" && <><Notice tone="warning">{text("确认实际收款后再批准。到账金额使用订单汇率快照，批准后不能重复入账。", "Approve only after verifying receipt of payment. Credit uses this order’s rate snapshot and cannot be posted twice.")}</Notice><Field label={text("审核原因 / 付款凭据", "Review reason / payment evidence")}><textarea required maxLength={1000} value={reason} onChange={event => setReason(event.target.value)} /></Field><ActionFeedback action={action} /><div className="saas-actions"><Button tone="danger" busy={action.busy} disabled={!reason.trim()} onClick={() => review("rejected")}>{text("拒绝申请", "Reject request")}</Button><Button tone="primary" busy={action.busy} disabled={!reason.trim()} onClick={() => review("approved")}>{text("批准并入账", "Approve & credit")} {formatMoney(selected.creditMicros, locale)}</Button></div></>}</Dialog>}
  </Card>;
}
