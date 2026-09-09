import { useState, type FormEvent } from "react";
import { KeyRound, Pencil, Plus, RotateCw, ShieldCheck, Trash2 } from "lucide-react";
import type { SaasUserClient } from "../api";
import type { KeySecret, PageResult, SaasGroup, SaasKey } from "../types";
import { decimalToInteger, formatDate, formatMoney, integerToDecimal } from "../format";
import { useSaasLocale } from "../i18n";
import { ActionFeedback, Button, Card, CopyButton, Dialog, Empty, ErrorState, Field, Heading, Loading, Notice, Pagination, Status, Table, useAction, useResource } from "../components/ui";

export function KeysPage({ client }: { client: SaasUserClient }) {
  const { locale, text } = useSaasLocale();
  const [page, setPage] = useState(1);
  const keys = useResource(() => client.call<PageResult<SaasKey>>("keys.list", { page, pageSize: 20 }), [page]);
  const groups = useResource(() => client.call<PageResult<SaasGroup>>("groups"));
  const [editor, setEditor] = useState<SaasKey | "new" | null>(null);
  const [secret, setSecret] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<{ key: SaasKey; kind: "rotate" | "delete" | "toggle" } | null>(null);
  const action = useAction();
  function changed(result?: KeySecret) { setEditor(null); setConfirmation(null); keys.reload(); if (result?.plaintextKey) setSecret(result.plaintextKey); }
  function confirm() {
    if (!confirmation) return;
    const { key, kind } = confirmation;
    void action.run(async () => kind === "rotate" ? client.call<KeySecret>("keys.rotate", { id: key.id }) : client.call<void>("keys.update", kind === "delete" ? { id: key.id, status: "revoked" } : { id: key.id, status: key.status === "active" ? "disabled" : "active" }), result => changed(result || undefined));
  }
  return <><Heading eyebrow={text("连接你的工具", "CONNECT YOUR TOOLS")} title={text("API 密钥", "API keys")} description={text("为每个应用创建独立密钥，权限与费用清晰分离。", "Separate keys for separate apps. Clear permissions, clear spending.")} action={<Button tone="primary" onClick={() => setEditor("new")}><Plus size={16} />{text("创建 API 密钥", "Create API key")}</Button>} />
    <Notice><ShieldCheck size={15} />{text("密钥固定绑定一个分组。明文仅在创建或轮换时展示一次，请妥善保存。", "Each key is bound to one group. The full secret is only shown once after creation or rotation. Store it securely.")}</Notice>
    {!confirmation && <ActionFeedback action={action} />}
    <Card title={text("你的密钥", "Your keys")}>{keys.loading ? <Loading /> : keys.error ? <ErrorState error={keys.error} retry={keys.reload} /> : keys.data && <><Table items={keys.data.items} rowKey={key => key.id} empty={<Empty title={text("创建第一个密钥", "Create your first key")} description={text("从应用到命令行，一把密钥即可开始。", "From apps to the command line, start with a dedicated key.")} action={<Button onClick={() => setEditor("new")}><KeyRound size={16} />{text("创建 API 密钥", "Create API key")}</Button>} />} columns={[
      { label: text("名称 / 标识", "Name / identifier"), render: key => <><strong>{key.name}</strong><small className="saas-mono saas-muted">{key.prefix}…</small></> },
      { label: text("分组", "Group"), render: key => key.groupName || groups.data?.items.find(group => group.id === key.groupId)?.name || key.groupId },
      { label: text("已用 / 总限额", "Used / total limit"), render: key => <>{formatMoney(key.spentMicros, locale)}<small className="saas-muted">/ {key.limitMicros == null ? text("不限额", "Unlimited") : formatMoney(key.limitMicros, locale)}</small></> },
      { label: text("有效期", "Expires"), render: key => <>{key.expiresAt ? formatDate(key.expiresAt, locale) : text("永不过期", "Never")}<small className="saas-muted">{text("最近使用：", "Last used: ")}{formatDate(key.lastUsedAt, locale)}</small></> },
      { label: text("状态", "Status"), render: key => <Status value={key.status !== "active" ? "disabled" : key.expiresAt && Date.parse(key.expiresAt) <= Date.now() ? "expired" : "active"} /> },
      { label: text("操作", "Actions"), render: key => <div className="saas-row saas-wrap"><Button tone="quiet" aria-label={`${text("编辑", "Edit")} ${key.name}`} onClick={() => setEditor(key)}><Pencil size={15} /></Button><Button tone="quiet" onClick={() => { action.clear(); setConfirmation({ key, kind: "toggle" }); }}>{key.status === "active" ? text("停用", "Disable") : text("启用", "Enable")}</Button><Button tone="quiet" aria-label={`${text("轮换", "Rotate")} ${key.name}`} onClick={() => { action.clear(); setConfirmation({ key, kind: "rotate" }); }}><RotateCw size={15} /></Button><Button tone="quiet" aria-label={`${text("删除", "Delete")} ${key.name}`} onClick={() => { action.clear(); setConfirmation({ key, kind: "delete" }); }}><Trash2 size={15} /></Button></div> },
    ]} /><Pagination page={page} total={keys.data.total} onPage={setPage} /></>}</Card>
    {editor && <KeyEditor client={client} existing={editor === "new" ? null : editor} groups={groups.data?.items || []} groupsLoading={groups.loading} groupsError={groups.error} onClose={() => setEditor(null)} onSaved={changed} />}
    {confirmation && <Dialog title={confirmation.kind === "rotate" ? text("轮换 API 密钥", "Rotate API key") : confirmation.kind === "delete" ? text("删除 API 密钥", "Delete API key") : (confirmation.key.status === "active") ? text("停用 API 密钥", "Disable API key") : text("启用 API 密钥", "Enable API key")} onClose={() => setConfirmation(null)} busy={action.busy}><p><strong>{confirmation.key.name}</strong> · <code>{confirmation.key.prefix}…</code></p><Notice tone="warning">{confirmation.kind === "rotate" ? text("旧密钥会立即失效。请在新密钥显示后更新所有使用它的应用。", "The old secret stops working immediately. Update every app using this key after saving the new secret.") : confirmation.kind === "delete" ? text("此操作不可撤销。请求和账务历史仍会保留。", "This cannot be undone. Request and accounting history is retained.") : text("此变更将立即影响新请求。", "This change affects new requests immediately.")}</Notice><ActionFeedback action={action} /><div className="saas-actions"><Button onClick={() => setConfirmation(null)} disabled={action.busy}>{text("返回", "Back")}</Button><Button tone={confirmation.kind === "delete" ? "danger" : "primary"} busy={action.busy} onClick={confirm}>{text("确认", "Confirm")}</Button></div></Dialog>}
    {secret && <Dialog title={text("保存你的新密钥", "Save your new key")} onClose={() => setSecret(null)}><Notice tone="warning">{text("离开此窗口后无法再次查看明文。不要将密钥分享给他人或提交到代码仓库。", "You cannot view this secret again after closing. Never share it or commit it to source control.")}</Notice><pre className="saas-secret"><code>{secret}</code></pre><CopyButton value={secret} /><div className="saas-actions"><Button tone="primary" onClick={() => setSecret(null)}>{text("我已保存，关闭", "I’ve saved it — close")}</Button></div></Dialog>}
  </>;
}

function KeyEditor({ client, existing, groups, groupsLoading, groupsError, onClose, onSaved }: { client: SaasUserClient; existing: SaasKey | null; groups: SaasGroup[]; groupsLoading: boolean; groupsError?: unknown; onClose: () => void; onSaved: (result?: KeySecret) => void }) {
  const { text } = useSaasLocale();
  const [name, setName] = useState(existing?.name || "");
  const [groupId, setGroupId] = useState(existing?.groupId || "");
  const [quota, setQuota] = useState(existing?.limitMicros == null ? "" : integerToDecimal(existing.limitMicros));
  const [expiresAt, setExpiresAt] = useState(existing?.expiresAt ? new Date(Date.parse(existing.expiresAt) - new Date(existing.expiresAt).getTimezoneOffset() * 60000).toISOString().slice(0, 16) : "");
  const action = useAction();
  function submit(event: FormEvent) { event.preventDefault(); void action.run(async () => {
    if (expiresAt && Date.parse(expiresAt) <= Date.now()) throw new Error(text("有效期必须晚于现在。", "Expiration must be in the future."));
    const payload = { name: name.trim(), limitMicros: quota ? decimalToInteger(quota) : null, expiresAt: expiresAt ? new Date(expiresAt).toISOString() : null };
    if (!payload.name) throw new Error(text("请输入密钥名称。", "Enter a key name."));
    if (existing) { await client.call("keys.update", { id: existing.id, ...payload }); return undefined; }
    return client.call<KeySecret>("keys.create", { ...payload, groupId });
  }, onSaved); }
  return <Dialog title={existing ? text("编辑密钥", "Edit key") : text("创建 API 密钥", "Create API key")} onClose={onClose} busy={action.busy}><form onSubmit={submit} className="saas-form"><Field label={text("密钥名称", "Key name")}><input required maxLength={80} value={name} onChange={event => setName(event.target.value)} /></Field>{existing ? <Field label={text("固定分组", "Fixed group")}><input readOnly value={existing.groupName || groups.find(group => group.id === groupId)?.name || groupId} /></Field> : <Field label={text("分组", "Group")} hint={text("创建后无法更换分组。", "The group cannot be changed after creation.")}><select required value={groupId} disabled={groupsLoading} onChange={event => setGroupId(event.target.value)}><option value="">{text("请选择分组", "Select a group")}</option>{groups.filter(group => !group.isInternal && group.configured).map(group => <option key={group.id} value={group.id}>{group.name} · {group.platform}</option>)}</select></Field>}{groupsError != null && <ErrorState error={groupsError} />}<Field label={text("总费用限额 (USD)", "Total spending limit (USD)")} hint={text("留空不限额；0 表示不能消费。", "Leave empty for unlimited; 0 allows no spending.")}><input inputMode="decimal" value={quota} onChange={event => setQuota(event.target.value)} /></Field><Field label={text("有效期（本地时间）", "Expires at (local time)")} hint={text("留空永不过期。", "Leave empty for no expiration.")}><input type="datetime-local" value={expiresAt} onChange={event => setExpiresAt(event.target.value)} /></Field><ActionFeedback action={action} /><div className="saas-actions"><Button onClick={onClose} disabled={action.busy}>{text("取消", "Cancel")}</Button><Button type="submit" tone="primary" busy={action.busy} disabled={!existing && (!groupId || groupsLoading || !!groupsError)}>{existing ? text("保存密钥", "Save key") : text("创建密钥", "Create key")}</Button></div></form></Dialog>;
}
