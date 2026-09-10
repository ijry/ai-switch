import { useState, type FormEvent } from "react";
import { Layers3, Pencil, Plus } from "lucide-react";
import { adminCall } from "../api";
import type { PageResult, SaasCatalog, SaasGroup } from "../types";
import { decimalToInteger, formatCount, formatMoney, integerToDecimal } from "../format";
import { useSaasLocale } from "../i18n";
import { ActionFeedback, Button, Card, CheckField, Dialog, Empty, ErrorState, Field, Loading, Notice, Pagination, Table, useAction, useResource } from "../components/ui";

export function GroupsPanel() {
  const { locale, text } = useSaasLocale();
  const [page, setPage] = useState(1);
  const [editor, setEditor] = useState<SaasGroup | null>(null);
  const [adding, setAdding] = useState(false);
  const groups = useResource(() => adminCall<PageResult<SaasGroup>>("groups.list", { page, pageSize: 20 }), [page]);
  return <>
    <div className="saas-section-heading"><div><h2>{text("分组与定价", "Groups & pricing")}</h2>
      <p className="saas-muted">{text("这里只显示已完成 SaaS 配置的分组。点击添加，从智能体分组中选择并配置模型价格。", "Only groups with completed SaaS settings appear here. Add an agent group, then configure its model pricing.")}</p></div><Button tone="primary" onClick={() => setAdding(true)}><Plus size={16} />{text("添加分组", "Add group")}</Button></div>
    {groups.loading ? <Loading /> : groups.error ? <ErrorState error={groups.error} retry={groups.reload} /> : groups.data && <>
      {groups.data.items.length ? <div className="saas-group-grid">{groups.data.items.map(group => <Card key={group.id} title={<span className="saas-row"><Layers3 size={19} />{group.name}</span>}>
        <div className="saas-group-summary"><span className="saas-badge">{group.platform === "codex" ? "Codex" : group.platform === "gemini" ? "Gemini" : "Claude"}</span><strong>{integerToDecimal(group.multiplierMicros)}×</strong><span>{formatCount(group.availableAccountCount, locale)} {text("可用账号", "available accounts")}</span></div>
        <p className="saas-caption">{group.isActive ? text("普通路由已激活", "Active for ordinary routing") : text("普通路由未激活", "Inactive for ordinary routing")}</p>
        {group.availableAccountCount === 0 && <Notice tone="warning">{text("没有可用账号，不会回退到其他分组。", "No eligible accounts. Requests never fall back to another group.")}</Notice>}
        <Table items={group.models} rowKey={model => model.model} columns={[
          { label: text("模型映射", "Model mapping"), render: model => <code>{model.model} → {model.upstreamModel}</code> },
          { label: text("输入 / 缓存 / 输出 · USD/百万", "Input / cache / output · USD/M"), render: model => <span className="saas-tabular">{formatMoney(model.inputPriceMicros,locale)} / {formatMoney(model.cachePriceMicros,locale)} / {formatMoney(model.outputPriceMicros,locale)}</span> },
          { label: text("生图 · USD/张", "Image · USD/image"), render: model => <span className="saas-tabular">{(model.imagePriceMicros ?? 0) > 0 ? formatMoney(model.imagePriceMicros ?? 0,locale) : "—"}</span> },
        ]} />
        <div className="saas-actions"><Button onClick={() => setEditor(group)}><Pencil size={15} />{text("编辑 SaaS 配置", "Edit SaaS settings")}</Button></div>
      </Card>)}</div> : <Card><Empty title={text("尚未添加分组", "No groups added")} description={text("点击“添加分组”选择一个智能体分组并完成定价配置。", "Add an agent group and complete its pricing settings.")} /></Card>}
      <Pagination page={page} total={groups.data.total} onPage={setPage} />
    </>}
    {adding && <GroupPicker onClose={() => setAdding(false)} onSelect={group => { setAdding(false); setEditor(group); }} />}
    {editor && <GroupEditor existing={editor} onClose={() => setEditor(null)} onSaved={() => { setEditor(null); groups.reload(); }} />}
  </>;
}

function GroupPicker({ onClose, onSelect }: { onClose: () => void; onSelect: (group: SaasGroup) => void }) {
  const { text } = useSaasLocale();
  const available = useResource(() => adminCall<PageResult<SaasGroup>>("groups.available", { page: 1, pageSize: 200 }), []);
  const [groupId, setGroupId] = useState("");
  const selected = available.data?.items.find(group => group.id === groupId);
  return <Dialog title={text("添加 SaaS 分组", "Add SaaS group")} onClose={onClose}>
    {available.loading ? <Loading /> : available.error ? <ErrorState error={available.error} retry={available.reload} /> : available.data?.items.length ? <div className="saas-form">
      <Field label={text("未添加的智能体分组", "Unconfigured agent group")} hint={text("内部分组和已配置分组不会出现在这里。", "Internal and already configured groups are excluded.")}><select autoFocus required value={groupId} onChange={event => setGroupId(event.target.value)}><option value="">{text("请选择分组", "Select a group")}</option>{available.data.items.map(group => <option key={group.id} value={group.id}>{group.name} · {group.platform === "codex" ? "Codex" : group.platform === "gemini" ? "Gemini" : "Claude"}</option>)}</select></Field>
      <div className="saas-actions"><Button onClick={onClose}>{text("取消", "Cancel")}</Button><Button tone="primary" disabled={!selected} onClick={() => selected && onSelect(selected)}>{text("配置价格", "Configure pricing")}</Button></div>
    </div> : <Empty title={text("没有可添加的分组", "No groups to add")} description={text("请先在智能体中创建一个非内部分组，或检查现有分组是否都已配置。", "Create a non-internal agent group, or check whether every group is already configured.")} />}
  </Dialog>;
}

type PriceDraft = { model: string; upstreamModel: string; input: string; cache: string; output: string; image: string };

function GroupEditor({ existing, onClose, onSaved }: { existing: SaasGroup; onClose: () => void; onSaved: () => void }) {
  const { text } = useSaasLocale();
  const catalog = useResource(() => adminCall<SaasCatalog>("catalog", { groupId: existing.id }), [existing.id]);
  const action = useAction();
  const [multiplier, setMultiplier] = useState(integerToDecimal(existing.multiplierMicros));
  const [maxOutput, setMaxOutput] = useState(String(existing.maxOutputTokens));
  const [timeout, setTimeoutValue] = useState(String(existing.timeoutSeconds));
  const [concurrency, setConcurrency] = useState(String(existing.maxConcurrency));
  const [allowSubscription, setAllowSubscription] = useState(existing.allowSubscription ?? true);
  const [allowBalance, setAllowBalance] = useState(existing.allowBalance ?? true);
  const [prices, setPrices] = useState<PriceDraft[]>(existing.models.map(model => ({ model:model.model,upstreamModel:model.upstreamModel,input:integerToDecimal(model.inputPriceMicros),cache:integerToDecimal(model.cachePriceMicros),output:integerToDecimal(model.outputPriceMicros),image:integerToDecimal(model.imagePriceMicros ?? 0) })));
  const modelChoices = Array.from(new Set([...(catalog.data?.models ?? []),...prices.map(price => price.upstreamModel)]));
  function submit(event: FormEvent) {
    event.preventDefault();
    void action.run(() => {
      const maxOutputTokens = Number(maxOutput);
      const timeoutSeconds = Number(timeout);
      const maxConcurrency = Number(concurrency);
      if (![maxOutputTokens,timeoutSeconds,maxConcurrency].every(value => Number.isSafeInteger(value) && value > 0)) throw new Error(text("请求限制必须为正整数。", "Request limits must be positive integers."));
      if (!prices.length) throw new Error(text("请至少选择并配置一个模型。", "Select and configure at least one model."));
      if (!allowSubscription && !allowBalance) throw new Error(text("订阅和余额至少允许一种。", "Allow subscriptions, balance, or both."));
      return adminCall("groups.save", { id:existing.id,multiplierMicros:decimalToInteger(multiplier),maxOutputTokens,timeoutSeconds,maxConcurrency,allowSubscription,allowBalance,
        models:prices.map(price => ({ model:price.model.trim(),upstreamModel:price.upstreamModel,inputPriceMicros:decimalToInteger(price.input),cachePriceMicros:decimalToInteger(price.cache),outputPriceMicros:decimalToInteger(price.output),imagePriceMicros:decimalToInteger(price.image || "0") })) });
    },onSaved);
  }
  return <Dialog title={existing.configured ? text("编辑 SaaS 配置", "Edit SaaS settings") : text("添加 SaaS 分组", "Add SaaS group")} busy={action.busy} onClose={onClose}><form className="saas-form" onSubmit={submit}>
    <Field label={text("智能体分组", "Agent group")}><input readOnly value={`${existing.name} · ${existing.platform}`} /></Field>
    <Notice>{text("名称、成员、删除和内部标记请在智能体分组中操作。", "Manage names, membership, deletion, and internal visibility in the agent workspace.")}</Notice>
    <fieldset><legend>{text("计费来源", "Funding sources")}</legend><CheckField label={text("允许订阅额度", "Allow subscription quota")} checked={allowSubscription} onChange={setAllowSubscription} /><CheckField label={text("允许余额", "Allow wallet balance")} checked={allowBalance} onChange={setAllowBalance} /></fieldset>
    <div className="saas-form-grid"><Field label={text("分组倍率", "Group multiplier")}><input required inputMode="decimal" value={multiplier} onChange={event => setMultiplier(event.target.value)} /></Field>
      <Field label={text("最大输出 token", "Maximum output tokens")}><input required type="number" min={1} max={1000000} value={maxOutput} onChange={event => setMaxOutput(event.target.value)} /></Field>
      <Field label={text("超时（秒）", "Timeout (seconds)")}><input required type="number" min={1} max={600} value={timeout} onChange={event => setTimeoutValue(event.target.value)} /></Field>
      <Field label={text("用户 / Key 并发上限", "User / key concurrency limit")}><input required type="number" min={1} max={1000} value={concurrency} onChange={event => setConcurrency(event.target.value)} /></Field></div>
    {catalog.loading ? <Loading /> : catalog.error ? <ErrorState error={catalog.error} retry={catalog.reload} /> : null}
    <fieldset><legend>{text("允许的模型", "Allowed models")}</legend>{modelChoices.map(model => <CheckField key={model} label={model} checked={prices.some(price => price.upstreamModel===model)} onChange={checked => setPrices(current => checked ? [...current,{model,upstreamModel:model,input:"",cache:"",output:"",image:""}] : current.filter(price => price.upstreamModel!==model))} />)}</fieldset>
    {prices.map((price,index) => <div className="saas-form-grid" key={`${price.upstreamModel}:${index}`}>
      <Field label={text(`公开模型名 ${index+1}`, `Public model name ${index+1}`)}><input required value={price.model} onChange={event => setPrices(current => current.map((entry,position) => position===index ? {...entry,model:event.target.value}:entry))} /></Field>
      <Field label={text(`上游模型 ${index+1}`, `Upstream model ${index+1}`)}><input readOnly value={price.upstreamModel} /></Field>
      {(["input","cache","output"] as const).map((kind,position) => <Field key={kind} label={`${text(["输入价格","缓存价格","输出价格"][position],["Input price","Cache price","Output price"][position])} ${index+1} (USD/M)`}><input required inputMode="decimal" value={price[kind]} onChange={event => setPrices(current => current.map((entry,offset) => offset===index ? {...entry,[kind]:event.target.value}:entry))} /></Field>)}
      <Field label={`${text("生图价格", "Image price")} ${index+1} (USD/image)`}><input inputMode="decimal" placeholder="0" value={price.image} onChange={event => setPrices(current => current.map((entry,offset) => offset===index ? {...entry,image:event.target.value}:entry))} /></Field>
    </div>)}
    <ActionFeedback action={action} /><div className="saas-actions"><Button onClick={onClose}>{text("取消", "Cancel")}</Button><Button tone="primary" type="submit" busy={action.busy}>{text("保存配置", "Save settings")}</Button></div>
  </form></Dialog>;
}
