import { useState, type FormEvent } from "react";
import { adminCall } from "../api";
import type { LedgerEntry, PageResult } from "../types";
import { decimalToInteger, formatDate, formatMoney } from "../format";
import { useSaasLocale } from "../i18n";
import { ActionFeedback, Button, Card, Dialog, ErrorState, Field, Loading, Notice, Pagination, Status, Table, useAction, useResource } from "../components/ui";

export function LedgerPanel({ initialUserId = "" }: { initialUserId?: string }) {
  const { locale, text } = useSaasLocale();
  const [draft, setDraft] = useState({ userId: initialUserId, status: "" });
  const [filters, setFilters] = useState(draft);
  const [page, setPage] = useState(1);
  const ledger = useResource(() => adminCall<PageResult<LedgerEntry>>("ledger.list", { ...(filters.userId ? { userId: filters.userId } : {}), ...(filters.status ? { status: filters.status } : {}), page, pageSize: 20 }), [filters, page]);
  const [selected, setSelected] = useState<LedgerEntry | null>(null);
  const [detail, setDetail] = useState<LedgerEntry | null>(null);
  const kinds: Record<string, string> = { usage: text("用量扣费", "Usage charge"), recharge: text("人工充值", "Manual recharge"), redeem: text("兑换", "Redemption"), reservation: text("请求预占", "Reservation"), refund: text("释放", "Refund") };
  return <Card title={text("账务与待核对", "Ledger & reconciliation")}><form className="saas-filters" onSubmit={event => { event.preventDefault(); setFilters(draft); setPage(1); ledger.reload(); }}><Field label={text("用户 ID", "User ID")}><input value={draft.userId} onChange={event => setDraft({ ...draft, userId: event.target.value })} /></Field><Field label={text("账务状态", "Accounting status")}><select value={draft.status} onChange={event => setDraft({ ...draft, status: event.target.value })}><option value="">{text("全部", "All")}</option><option value="pending_review">{text("待核对", "Needs reconciliation")}</option><option value="reserved">{text("已预占", "Reserved")}</option><option value="settled">{text("已结算", "Settled")}</option><option value="refunded">{text("已释放", "Refunded")}</option></select></Field><Button type="submit" busy={ledger.loading}>{text("查询", "Search")}</Button></form><Notice>{text("不完整用量不会虚构扣费。待核对请求保留冻结金额，核实后再结算或释放。", "Incomplete usage is never billed as a fabricated amount. Requests awaiting review retain reserved funds until verified and settled or released.")}</Notice>
    {ledger.loading ? <Loading /> : ledger.error ? <ErrorState error={ledger.error} retry={ledger.reload} /> : ledger.data && <><Table items={ledger.data.items} rowKey={entry => entry.id} columns={[
      { label: text("记录 / 用户", "Record / user"), render: entry => <><Button tone="quiet" onClick={() => setDetail(entry)}>{entry.requestId || entry.id}</Button><small>{entry.userId}</small><small>{formatDate(entry.createdAt, locale)}</small></> },
      { label: text("类型", "Type"), render: entry => kinds[entry.kind] || entry.kind },
      { label: text("金额 / 冻结", "Amount / reserved"), render: entry => <>{formatMoney(entry.amountMicros, locale)}{entry.reservedMicros != null && <small>{formatMoney(entry.reservedMicros, locale)} {text("预占", "reserved")}</small>}</> },
      { label: text("变动后余额", "Balance after"), render: entry => formatMoney(entry.balanceAfterMicros, locale) },
      { label: text("状态", "Status"), render: entry => <Status value={entry.status || "settled"} /> },
      { label: text("操作", "Actions"), render: entry => entry.status === "pending_review" ? <Button tone="primary" onClick={() => setSelected(entry)}>{text("核对", "Reconcile")}</Button> : <Button tone="quiet" onClick={() => setDetail(entry)}>{text("详情", "Details")}</Button> },
    ]} /><Pagination page={page} total={ledger.data.total} onPage={setPage} /></>}
    {selected && <ReconcileDialog entry={selected} onClose={() => setSelected(null)} onSaved={() => { setSelected(null); ledger.reload(); }} />}
    {detail && <Dialog title={text("账务详情", "Ledger details")} onClose={() => setDetail(null)}><dl className="saas-details"><dt>{text("记录", "Record")}</dt><dd className="saas-mono">{detail.id}</dd><dt>{text("来源", "Source")}</dt><dd>{detail.sourceId || detail.requestId || "—"}</dd><dt>{text("用户", "User")}</dt><dd>{detail.userId}</dd><dt>{text("金额", "Amount")}</dt><dd>{formatMoney(detail.amountMicros, locale)}</dd><dt>{text("操作方", "Actor")}</dt><dd>{detail.actor || "—"}</dd><dt>{text("原因", "Reason")}</dt><dd>{detail.reason || "—"}</dd></dl></Dialog>}
  </Card>;
}

function ReconcileDialog({ entry, onClose, onSaved }: { entry: LedgerEntry; onClose: () => void; onSaved: () => void }) {
  const { locale, text } = useSaasLocale();
  const [mode, setMode] = useState("refund");
  const [amount, setAmount] = useState("");
  const [reason, setReason] = useState("");
  const action = useAction();
  function submit(event: FormEvent) { event.preventDefault(); void action.run(() => adminCall("ledger.reconcile", { requestId: entry.requestId || entry.id, action: mode, reason: reason.trim(), ...(mode === "settle" ? { amountMicros: decimalToInteger(amount) } : {}) }), onSaved); }
  return <Dialog title={text("人工账务核对", "Manual reconciliation")} onClose={onClose} busy={action.busy}><p className="saas-mono">{entry.requestId || entry.id}</p><p>{text("冻结金额：", "Reserved amount: ")}<strong>{formatMoney(entry.reservedMicros, locale)}</strong></p><form className="saas-form" onSubmit={submit}><Field label={text("核对结果", "Reconciliation result")}><select value={mode} onChange={event => setMode(event.target.value)}><option value="refund">{text("释放全部冻结金额（不扣费）", "Release reserved funds (no charge)")}</option><option value="settle">{text("按已核实金额结算", "Settle a verified charge")}</option></select></Field>{mode === "settle" && <Field label={text("已核实费用 (USD)", "Verified charge (USD)")}><input required inputMode="decimal" value={amount} onChange={event => setAmount(event.target.value)} /></Field>}<Field label={text("核对原因与凭据", "Reconciliation reason & evidence")}><textarea required maxLength={1000} value={reason} onChange={event => setReason(event.target.value)} /></Field><Notice tone="warning">{text("此操作直接影响用户余额，必须依据真实证据；确认后不能重复结算。", "This directly affects the user’s balance. Use verified evidence; the request cannot be settled twice.")}</Notice><ActionFeedback action={action} /><div className="saas-actions"><Button disabled={action.busy} onClick={onClose}>{text("取消", "Cancel")}</Button><Button type="submit" tone="primary" busy={action.busy} disabled={!reason.trim()}>{text("确认核对", "Confirm reconciliation")}</Button></div></form></Dialog>;
}
