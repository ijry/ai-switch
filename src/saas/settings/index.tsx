import { useState, type FormEvent } from "react";
import { Database, ExternalLink, Github, Save, Server, Settings2 } from "lucide-react";
import { adminCall } from "../api";
import type { SaasActivationStatus, SaasConfig, SaasConfigUpdate, SaasHostProps, SaasLogConfig } from "../types";
import { decimalToInteger, integerToDecimal } from "../format";
import { SaasFrame, useSaasLocale } from "../i18n";
import { ActionFeedback, Button, Card, CheckField, CopyButton, Dialog, ErrorState, Field, Heading, Loading, Notice, useAction, useResource } from "../components/ui";
import { QQ_GROUP_NAME, QQ_GROUP_URL } from "../../components/about/catalog";
import { getWebServerStatus } from "../../lib/api/client";
import { openExternal } from "../../lib/openExternal";
import "../saas.css";

export function SaasSettings(props: SaasHostProps) {
  return (
    <SaasFrame embedded>
      <SaasSettingsContent {...props} />
    </SaasFrame>
  );
}

function SaasSettingsContent(props: SaasHostProps) {
  const { text } = useSaasLocale();
  return (
    <div className="saas-admin-content">
      <Heading
        description={text("这里只控制插件是否启用；登录、计费和运营配置统一在 SaaS 管理面板维护。", "This switch only controls plugin availability. Manage sign-in, billing, and operations in the SaaS panel.")}
        eyebrow={text("内置插件", "BUILT-IN PLUGIN")}
        title={text("SaaS 插件", "SaaS plugin")}
      />
      <SaasPluginSwitch {...props} />
    </div>
  );
}

export function SaasPluginSwitch({ onConfigChanged }: SaasHostProps) {
  const { text } = useSaasLocale();
  const resource = useResource(async () => {
    const [config, activation] = await Promise.all([
      adminCall<SaasConfig>("config.get"),
      adminCall<SaasActivationStatus>("activation.status"),
    ]);
    return { config, activation };
  });
  const [unlockDialogOpen, setUnlockDialogOpen] = useState(false);
  const action = useAction();
  function setEnabled(enabled: boolean) {
    void action.run(
      () => adminCall<SaasConfig>("config.save", { enabled }),
      () => { resource.reload(); onConfigChanged?.(); },
    );
  }
  return <>{resource.loading ? <Loading /> : resource.error ? <ErrorState error={resource.error} retry={resource.reload} /> : resource.data && <Card><div className="saas-settings-switch"><CheckField label={text("启用 SaaS 插件", "Enable SaaS plugin")} checked={resource.data.config.enabled} disabled={action.busy} onChange={enabled => { if (enabled && !resource.data?.activation.unlocked) setUnlockDialogOpen(true); else setEnabled(enabled); }} hint={text("启用后显示 SaaS 管理导航并开放用户站点。", "Shows the SaaS administration navigation and opens the user portal.")} /></div><ActionFeedback action={action} /></Card>}{unlockDialogOpen && <ActivationDialog onClose={() => setUnlockDialogOpen(false)} onUnlocked={() => { setUnlockDialogOpen(false); setEnabled(true); }} />}</>;
}

export function SettingsContent({ onConfigChanged }: SaasHostProps) {
  const { text } = useSaasLocale();
  const resource = useResource(async () => {
    const [config, webStatus] = await Promise.all([
      adminCall<SaasConfig>("config.get"),
      getWebServerStatus(),
    ]);
    return { config, webStatus };
  });
  return <><Heading eyebrow={text("服务配置", "SERVICE CONFIGURATION")} title={text("SaaS 设置", "SaaS settings")} description={text("集中配置用户登录、计费、增长和服务基础设施。", "Configure user sign-in, billing, growth, and service infrastructure in one place.")} />{resource.loading ? <Loading /> : resource.error ? <ErrorState error={resource.error} retry={resource.reload} /> : resource.data && <ConfigForm initial={resource.data.config} webBaseUrl={resource.data.webStatus.baseUrl} onConfigChanged={onConfigChanged} />}</>;
}

function ConfigForm({ initial, webBaseUrl, onConfigChanged }: { initial: SaasConfig; webBaseUrl?: string | null } & SaasHostProps) {
  const { text } = useSaasLocale();
  const [config, setConfig] = useState(initial);
  const [exchangeRate, setExchangeRate] = useState(integerToDecimal(initial.exchangeRateMicros));
  const [checkinReward, setCheckinReward] = useState(integerToDecimal(initial.checkinRewardMicros ?? 0));
  const [inviteSignupReward, setInviteSignupReward] = useState(integerToDecimal(initial.inviteSignupRewardMicros ?? 0));
  const [inviteRechargeRate, setInviteRechargeRate] = useState(integerToDecimal(initial.inviteRechargeRateMicros ?? 0));
  const [secret, setSecret] = useState("");
  const action = useAction();
  const logs = config.logs || {};
  function change<Key extends keyof SaasConfig>(key: Key, value: SaasConfig[Key]) { setConfig(current => ({ ...current, [key]: value })); action.clear(); }
  function changeLog<Key extends keyof SaasLogConfig>(key: Key, value: SaasLogConfig[Key]) { setConfig(current => ({ ...current, logs: { ...current.logs, [key]: value } })); action.clear(); }
  const callback = config.publicBaseUrl ? `${config.publicBaseUrl.replace(/\/$/, "")}/api/saas/auth/github/callback` : "";
  const webBase = config.publicBaseUrl.trim() || webBaseUrl || "";
  const githubSetupUrl = `https://github.com/settings/applications/new?oauth_application[name]=${encodeURIComponent(config.siteName || "AI Switch")}&oauth_application[url]=${encodeURIComponent(webBase)}&oauth_application[callback_url]=${encodeURIComponent(webBase ? webBase.replace(/\/$/, "") + "/api/saas/auth/github/callback" : "")}`;
  function save(event: FormEvent) { event.preventDefault(); void action.run(async () => {
    const rate = exchangeRate.trim() ? decimalToInteger(exchangeRate) : null;
    if (rate === 0) throw new Error(text("汇率必须大于零。", "The exchange rate must be positive."));
    const payload: Omit<SaasConfigUpdate, "enabled"> = { registrationEnabled: config.registrationEnabled, passwordLoginEnabled: config.passwordLoginEnabled, siteName: config.siteName.trim(), publicBaseUrl: config.publicBaseUrl.trim(), githubClientId: config.githubClientId.trim(), githubClientSecretConfigured: config.githubClientSecretConfigured, exchangeRateMicros: rate, checkinEnabled: config.checkinEnabled, checkinRewardMicros: decimalToInteger(checkinReward), inviteEnabled: config.inviteEnabled, inviteRegistrationRequired: config.inviteRegistrationRequired, inviteSignupRewardMicros: decimalToInteger(inviteSignupReward), inviteRechargeRateMicros: decimalToInteger(inviteRechargeRate), logs, ...(secret ? { githubClientSecret: secret } : {}) };
    const saved = await adminCall<SaasConfig>("config.save", payload);
    return saved;
  }, saved => { setConfig(saved); setExchangeRate(integerToDecimal(saved.exchangeRateMicros)); setCheckinReward(integerToDecimal(saved.checkinRewardMicros ?? 0)); setInviteSignupReward(integerToDecimal(saved.inviteSignupRewardMicros ?? 0)); setInviteRechargeRate(integerToDecimal(saved.inviteRechargeRateMicros ?? 0)); setSecret(""); action.setSuccess(text("配置已保存，服务端已确认应用。", "Configuration saved and confirmed by the server.")); onConfigChanged?.(); }); }
  return <><form className="saas-form" onSubmit={save}><Card title={<span className="saas-row"><Settings2 size={19} />{text("站点与开关", "Site & availability")}</span>}><div className="saas-settings-switch"><CheckField label={text("启用邮箱密码登录", "Enable email and password sign-in")} checked={config.passwordLoginEnabled} onChange={value => change("passwordLoginEnabled", value)} hint={text("开启后首页显示邮箱和密码入口，仅管理员创建的邮箱账号可以登录。", "Shows email and password fields on the homepage. Only administrator-created email accounts can sign in.")} /><CheckField label={text("允许 GitHub 新用户注册", "Allow new GitHub registrations")} checked={config.registrationEnabled} onChange={value => change("registrationEnabled", value)} hint={text("只控制 GitHub 新用户注册，不影响已有 GitHub 用户。", "Controls new GitHub registrations only; existing GitHub users are unaffected.")} /></div><div className="saas-form-grid"><Field label={text("站点名称", "Site name")}><input required maxLength={120} value={config.siteName} onChange={event => change("siteName", event.target.value)} /></Field><Field label={text("公开站点地址", "Public site URL")} hint={<>{text("SaaS 用户前台与邮箱密码登录使用这个地址。当前 Web 服务：", "The SaaS user portal and email sign-in use this URL. Current web service: ")}<code>{webBaseUrl || text("未运行", "not running")}</code>{webBaseUrl && <Button type="button" onClick={() => change("publicBaseUrl", webBaseUrl)}>{text("使用当前地址", "Use current URL")}</Button>}</>}><input type="url" value={config.publicBaseUrl} onChange={event => change("publicBaseUrl", event.target.value)} /></Field></div><Notice>{text("管理入口固定为 /ai-switch-admin。用户 Cookie 与管理员 token 完全独立。", "The administrator entry is always /ai-switch-admin. User cookies are separate from administrator tokens.")}</Notice></Card>
    <Card title={<span className="saas-row"><Github size={19} />GitHub OAuth</span>} action={<Button disabled={!config.publicBaseUrl} onClick={() => void openExternal(githubSetupUrl)}><ExternalLink size={15} />{text("引导创建 GitHub 应用", "Create GitHub app")}</Button>}><Notice>{text("GitHub 不允许第三方静默创建 OAuth App。填写公开站点地址后，这里会打开已预填信息的官方创建页；GitHub 登录为可选项，不影响邮箱账号登录。", "GitHub does not allow third parties to silently create an OAuth App. After entering the public site URL, this opens the official form prefilled. GitHub sign-in is optional and does not affect email login.")}</Notice><div className="saas-form-grid"><Field label="GitHub Client ID"><input autoComplete="off" value={config.githubClientId} onChange={event => change("githubClientId", event.target.value)} /></Field><Field label="GitHub Client Secret" hint={config.githubClientSecretConfigured ? text("已配置。留空保留现有秘密，不回显原值。", "Already configured. Leave empty to preserve the stored secret; it is never displayed.") : text("可选；只在需要 GitHub 登录时配置。", "Optional; configure only when GitHub sign-in is needed.")}><input type="password" autoComplete="new-password" value={secret} onChange={event => { setSecret(event.target.value); action.clear(); }} /></Field></div><Field label={text("GitHub 回调地址", "GitHub callback URL")}><input readOnly value={callback} /></Field><CopyButton value={callback} /></Card>
    <Card title={text("汇率与账务", "Exchange rate & accounting")}><Field label={text("每 1 美元对应人民币", "CNY per 1 USD")} hint={text("可稍后配置；创建充值订单前必须填写。", "Optional for preview; required before creating recharge orders.")}><input inputMode="decimal" value={exchangeRate} onChange={event => setExchangeRate(event.target.value)} /></Field><p className="saas-muted">{text("美元使用整数微美元记账，人工充值使用人民币整数分。单价、倍率、并发、最大输出和超时在「分组与定价」中设置。", "USD uses integer microdollars; manual recharges use integer CNY cents. Configure model prices, multipliers, concurrency, maximum output, and timeouts under Groups & pricing.")}</p></Card>
    <Card title={<span className="saas-row"><Server size={19} />{text("日志队列", "Log queue")}</span>}><div className="saas-form-grid"><Field label={text("队列驱动", "Queue driver")}><select value={logs.queue || "memory"} onChange={event => changeLog("queue", event.target.value as "memory" | "redis")}><option value="memory">{text("进程内存（默认）", "In-process memory (default)")}</option><option value="redis">Redis</option></select></Field><Field label={text("最大排队记录数", "Maximum queued records")}><input type="number" min={1} required value={logs.maxRecords ?? 10000} onChange={event => changeLog("maxRecords", Number(event.target.value))} /></Field><Field label={text("队列字节上限", "Queue byte limit")}><input type="number" min={16384} required value={logs.maxBytes ?? 16777216} onChange={event => changeLog("maxBytes", Number(event.target.value))} /></Field><Field label={text("写入批次大小", "Write batch size")}><input type="number" min={1} required value={logs.batchSize ?? 100} onChange={event => changeLog("batchSize", Number(event.target.value))} /></Field></div>{logs.queue === "redis" && <Field label={text("Redis 连接环境变量名", "Redis connection environment variable")} hint={text("在服务端设置该环境变量为连接 URL，不能在此粘贴密码或连接串。", "Set this variable to the connection URL on the server. Do not paste passwords or a connection string here.")}><input required pattern="[A-Za-z_][A-Za-z0-9_]*" value={logs.redisUrlEnv || "AI_SWITCH_SAAS_REDIS_URL"} onChange={event => changeLog("redisUrlEnv", event.target.value)} /></Field>}<Notice tone="warning">{text("默认队列是有界进程内存，不兼容 Redis 协议。正常停止会排空；进程崩溃可能丢失未落盘日志，已提交账务不受影响。", "The default is a bounded in-process queue, not a Redis-protocol implementation. Graceful shutdown drains it; a crash can lose unwritten logs without losing committed accounting.")}</Notice></Card>

    <Card title={text("签到与邀请", "Check-in & referrals")}><div className="saas-settings-switch"><CheckField label={text("启用每日签到", "Enable daily check-in")} checked={!!config.checkinEnabled} onChange={value => change("checkinEnabled", value)} hint={text("用户每天可签到一次，奖励直接进入余额。", "Users can check in once per day; the reward enters their balance immediately.")} /><CheckField label={text("启用邀请功能", "Enable referrals")} checked={!!config.inviteEnabled} onChange={value => change("inviteEnabled", value)} hint={text("推荐码由用户前台生成；后台邀请码仍由管理员创建。", "Users generate referral codes; invite codes remain administrator-managed.")} /><CheckField label={text("GitHub 注册必须填写邀请码", "Require invite codes for GitHub registration")} checked={!!config.inviteRegistrationRequired} onChange={value => change("inviteRegistrationRequired", value)} hint={text("仅影响 GitHub 新注册，不影响已有用户和邮箱账号登录。", "Applies only to new GitHub registrations, not existing users or email sign-in.")} /></div><div className="saas-form-grid"><Field label={text("签到奖励 USD", "Check-in reward USD")}><input inputMode="decimal" value={checkinReward} onChange={event => setCheckinReward(event.target.value)} /></Field><Field label={text("注册邀请奖励 USD", "Signup referral reward USD")}><input inputMode="decimal" value={inviteSignupReward} onChange={event => setInviteSignupReward(event.target.value)} /></Field><Field label={text("充值返利 USD / USD", "Recharge rebate USD per USD")} hint={text("例如 0.10 表示邀请人可获得被邀请人充值金额的 10%。奖励需审核后到账。", "For example, 0.10 pays the inviter 10% of the invitee’s recharge. Rewards require review.")}><input inputMode="decimal" value={inviteRechargeRate} onChange={event => setInviteRechargeRate(event.target.value)} /></Field></div><Notice>{text("邀请奖励不会自动到账；管理员审核通过后才写入邀请人余额。", "Referral rewards are not credited automatically; they enter the inviter’s balance only after administrator approval.")}</Notice></Card>
    <Card title={<span className="saas-row"><Database size={19} />{text("请求日志存储", "Request log storage")}</span>}><div className="saas-form-grid"><Field label={text("存储驱动", "Storage driver")}><select value={logs.store || "file"} onChange={event => changeLog("store", event.target.value as "file" | "postgres")}><option value="file">{text("文件系统（默认）", "Filesystem (default)")}</option><option value="postgres">PostgreSQL</option></select></Field><Field label={text("保留天数", "Retention days")} hint={text("留空关闭自动清理。", "Leave empty to disable automatic cleanup.")}><input type="number" min={1} value={logs.retentionDays === null ? "" : logs.retentionDays ?? 30} onChange={event => changeLog("retentionDays", event.target.value ? Number(event.target.value) : null)} /></Field></div>{logs.store === "postgres" ? <div className="saas-form-grid"><Field label={text("PostgreSQL 连接环境变量名", "PostgreSQL connection environment variable")} hint={text("服务端环境变量包含完整连接串及 TLS 配置。浏览器不读取凭证。", "The server variable contains the connection string and TLS settings. The browser never reads credentials.")}><input required pattern="[A-Za-z_][A-Za-z0-9_]*" value={logs.postgresUrlEnv || "AI_SWITCH_SAAS_POSTGRES_URL"} onChange={event => changeLog("postgresUrlEnv", event.target.value)} /></Field><Field label={text("连接池上限", "Maximum connections")}><input type="number" required min={1} max={100} value={logs.postgresMaxConnections ?? 5} onChange={event => changeLog("postgresMaxConnections", Number(event.target.value))} /></Field></div> : <Field label={text("日志绝对目录", "Absolute log directory")} hint={text("留空使用 ~/ai-switch/logs/yyyy-mm-dd/hh.log；不是 ~/.ai-switch/logs。", "Leave empty for ~/ai-switch/logs/yyyy-mm-dd/hh.log, not ~/.ai-switch/logs.")}><input value={logs.directory || ""} onChange={event => changeLog("directory", event.target.value || null)} /></Field>}<details><summary>{text("高级超时配置", "Advanced timeouts")}</summary><div className="saas-form-grid"><Field label={text("入队超时（毫秒）", "Enqueue timeout (ms)")}><input required type="number" min={1} value={logs.enqueueTimeoutMs ?? 100} onChange={event => changeLog("enqueueTimeoutMs", Number(event.target.value))} /></Field><Field label={text("停机排空超时（毫秒）", "Shutdown drain timeout (ms)")}><input required type="number" min={1} value={logs.shutdownTimeoutMs ?? 10000} onChange={event => changeLog("shutdownTimeoutMs", Number(event.target.value))} /></Field></div></details><Notice>{text("切换驱动先验证连接，再接收新日志；不会自动迁移历史日志。查询仅使用当前驱动。余额仍在业务数据库，请求明细不写入 SQLite。", "A driver change validates the connection before accepting new logs. Historical logs are not migrated automatically; queries use the current driver. Balances remain in the business database, and request details never go to SQLite.")}</Notice></Card>
    <ActionFeedback action={action} /><div className="saas-save-bar"><span className="saas-muted">{text("仅在服务端成功保存后生效。", "Changes take effect only after the server confirms the save.")}</span><Button type="submit" tone="primary" busy={action.busy}><Save size={16} />{text("保存配置", "Save configuration")}</Button></div>
  </form></>;
}

function ActivationDialog({ onClose, onUnlocked }: { onClose: () => void; onUnlocked: () => void }) {
  const { text } = useSaasLocale();
  const [activationCode, setActivationCode] = useState("");
  const action = useAction();
  function submit(event: FormEvent) {
    event.preventDefault();
    void action.run(
      () => adminCall<SaasActivationStatus>("activation.unlock", { activationCode: activationCode.trim() }),
      result => { if (result.unlocked) onUnlocked(); },
    );
  }
  return <Dialog title={text("解锁 SaaS", "Unlock SaaS")} onClose={onClose} busy={action.busy}><form onSubmit={submit}><Field label={text("内测码", "Beta access code")} hint={<>{text("请输入内测码完成一次性验证。内测码不会保存。", "Enter your beta access code for one-time verification. The code is never stored.")} <a href={QQ_GROUP_URL} target="_blank" rel="noreferrer">{text(`加入 QQ 群「${QQ_GROUP_NAME}」`, `Join QQ group “${QQ_GROUP_NAME}”`)}</a></>}><input autoFocus type="password" required autoComplete="off" value={activationCode} onChange={event => { setActivationCode(event.target.value); action.clear(); }} /></Field><ActionFeedback action={action} /><div className="saas-actions"><Button disabled={action.busy} onClick={onClose}>{text("取消", "Cancel")}</Button><Button type="submit" tone="primary" busy={action.busy}>{text("解锁", "Unlock")}</Button></div></form></Dialog>;
}
