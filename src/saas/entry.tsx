import { useEffect, useRef, useState, type FormEvent, type MouseEvent } from "react";
import { ArrowRight, Boxes, ChartNoAxesCombined, Github, Gift, Home, KeyRound, LogOut, Mail, Menu, ShieldCheck, Ticket, UserRound, Wallet, X } from "lucide-react";
import { createUserClient } from "./api";
import { SaasFrame, useSaasLocale } from "./i18n";
import { ActionFeedback, Button, Card, Empty, ErrorState, Field, Loading, Notice, useAction, useResource } from "./components/ui";
import { HomePage } from "./user/Home";
import { UsagePage } from "./user/Usage";
import { KeysPage } from "./user/Keys";
import { RechargePage, RedeemPage } from "./user/Billing";
import { ProfilePage } from "./user/Profile";
import { GrowthPage } from "./user/Growth";
import { REPOSITORY_URL } from "../components/about/catalog";
import "./saas.css";

export function SaasPortal() { return <SaasFrame><PortalContent /></SaasFrame>; }

function PortalContent() {
  const { text } = useSaasLocale();
  const [expired, setExpired] = useState(false);
  const [loggedOut, setLoggedOut] = useState(false);
  const [credentials, setCredentials] = useState({ email: "", password: "" });
  const [inviteCode, setInviteCode] = useState("");
  const [client] = useState(() => createUserClient(undefined, () => setExpired(true)));
  const boot = useResource(async () => { const config = await client.publicConfig(); return { config, session: config.enabled ? await client.session() : null }; });
  const [path, setPath] = useState(window.location.pathname);
  const [menuOpen, setMenuOpen] = useState(false);
  const action = useAction();
  const mainRef = useRef<HTMLElement>(null);
  useEffect(() => { const update = () => setPath(window.location.pathname); window.addEventListener("popstate", update); return () => window.removeEventListener("popstate", update); }, []);
  function navigate(destination: string) { if (destination !== path) window.history.pushState({}, "", destination); setPath(destination); setMenuOpen(false); mainRef.current?.focus(); }
  function link(event: MouseEvent<HTMLAnchorElement>, destination: string) { if (event.button === 0 && !event.ctrlKey && !event.metaKey && !event.shiftKey && !event.altKey) { event.preventDefault(); navigate(destination); } }
  const config = boot.data?.config;
  const user = !expired && !loggedOut ? boot.data?.session?.user : null;
  useEffect(() => {
    const main = mainRef.current;
    if (!main) return;
    let timeout: number | undefined;
    const handleScroll = () => {
      main.classList.add("saas-scrolling");
      window.clearTimeout(timeout);
      timeout = window.setTimeout(() => main.classList.remove("saas-scrolling"), 600);
    };
    main.addEventListener("scroll", handleScroll);
    return () => { main.removeEventListener("scroll", handleScroll); window.clearTimeout(timeout); };
  }, [user]);
  const navigation = [
    { path: "/", title: text("概览", "Overview"), icon: Home },
    { path: "/usage", title: text("用量", "Usage"), icon: ChartNoAxesCombined },
    { path: "/api-keys", title: text("API 密钥", "API keys"), icon: KeyRound },
    { path: "/recharge", title: text("充值", "Recharge"), icon: Wallet },
    { path: "/redeem", title: text("兑换", "Redeem"), icon: Ticket },
    { path: "/benefits", title: text("订阅与奖励", "Benefits"), icon: Gift },
    { path: "/profile", title: text("个人信息", "Profile"), icon: UserRound },
  ];
  const effectivePath = path === "/login" || path === "/index.html" ? "/" : path;
  useEffect(() => { const previous = document.title; document.title = `${navigation.find(item => item.path === effectivePath)?.title || text("登录", "Sign in")} · ${config?.siteName || "AI Switch"}`; return () => { document.title = previous; }; }, [effectivePath, config?.siteName, text("概览", "Overview")]);
  function logout() { void action.run(() => client.logout(), () => { setLoggedOut(true); navigate("/login"); }); }
  function passwordLogin(event: FormEvent) { event.preventDefault(); if (!config?.passwordLoginEnabled) return; void action.run(() => client.passwordLogin(credentials.email.trim(), credentials.password), session => { if (session.user) { setExpired(false); setLoggedOut(false); boot.reload(); navigate("/"); } }); }
  const oauthError = new URLSearchParams(window.location.search).get("error");
  const errors: Record<string, string> = {
    registration_disabled: text("本站已关闭新用户注册，已有用户仍可登录。", "Registration is closed. Existing users can still sign in."),
    account_too_new: text("GitHub 账号必须已创建满 365 天。", "Your GitHub account must be at least 365 days old."),
    github_account_too_new: text("GitHub 账号必须已创建满 365 天。", "Your GitHub account must be at least 365 days old."),
    user_banned: text("你的账号已被封禁，请联系站点管理员。", "Your account is suspended. Contact the site administrator."),
    invalid_state: text("登录验证已失效，请重新发起 GitHub 登录。", "The login verification has expired. Start GitHub sign-in again."),
    invite_required: text("GitHub 注册需要邀请码。", "An invite code is required for GitHub registration."),
    invite_invalid: text("邀请码无效或已用完。", "The invite code is invalid or exhausted."),
  };
  return <><header className="saas-topbar"><div className="saas-topbar-inner"><a className="saas-brand" href="/" onClick={event => link(event, "/")}><span className="saas-brand-mark"><Boxes size={23} /></span><span>{config?.siteName || "AI Switch"}<small>{text("开发者工作空间", "DEVELOPER WORKSPACE")}</small></span></a>{user && <Button className="saas-mobile-toggle" aria-label={text("切换导航", "Toggle navigation")} aria-expanded={menuOpen} onClick={() => setMenuOpen(!menuOpen)}>{menuOpen ? <X size={20} /> : <Menu size={20} />}</Button>}</div></header>
    {boot.loading ? <main className="saas-auth-wrap"><Loading /></main> : boot.error ? <main className="saas-auth-wrap"><ErrorState error={boot.error} retry={boot.reload} /></main> : !config?.enabled ? <main className="saas-auth-wrap"><Card><Empty title={text("站点暂未开放", "Workspace unavailable")} description={text("SaaS 服务当前已关闭，请联系站点管理员。", "The SaaS service is currently disabled. Contact the site administrator.")} /></Card></main> : !user ? <main className="saas-auth-wrap"><div className="saas-auth-intro"><p className="saas-eyebrow">BUILD SOMETHING GREAT</p><h1>{text("让灵感，\n连接更强大的 AI。", "Your ideas.\nMore possibilities.")}</h1><p>{text("一个工作空间，连接 Codex 与 Claude。\n按实际用量计费，每一笔都有迹可循。", "One workspace for Codex and Claude.\nUsage-based billing, with every dollar accounted for.")}</p><div className="saas-auth-features"><span><KeyRound size={18} />{text("独立 API 密钥", "Dedicated API keys")}</span><span><ShieldCheck size={18} />{text("透明用量与账务", "Transparent usage & billing")}</span></div></div><Card className="saas-login-card"><span className="saas-login-icon">{config.passwordLoginEnabled ? <Mail size={30} /> : <Github size={30} />}</span><h2>{text("欢迎来到你的工作空间", "Welcome to your workspace")}</h2><p className="saas-muted">{config.passwordLoginEnabled ? text("使用管理员创建的邮箱账号登录。", "Sign in with the email account created by your administrator.") : text("使用 GitHub 安全登录，开始构建。", "Sign in securely with GitHub to get started.")}</p>{expired && <Notice tone="warning">{text("会话已失效或账号不可用，请重新登录。", "Your session expired or your account is unavailable. Please sign in again.")}</Notice>}{oauthError && <Notice tone="error">{errors[oauthError] || text("GitHub 登录失败，请重试。错误码：", "GitHub sign-in failed. Please retry. Code: ") + oauthError}</Notice>}{config.passwordLoginEnabled && <form className="saas-form saas-password-login" onSubmit={passwordLogin}><Field label={text("邮箱", "Email")}><input type="email" required autoComplete="username" value={credentials.email} onChange={event => { setCredentials({ ...credentials, email: event.target.value }); action.clear(); }} /></Field><Field label={text("密码", "Password")}><input type="password" required autoComplete="current-password" value={credentials.password} onChange={event => { setCredentials({ ...credentials, password: event.target.value }); action.clear(); }} /></Field><ActionFeedback action={action} /><Button className="saas-login-button" type="submit" tone="primary" busy={action.busy}>{text("登录", "Sign in")}<ArrowRight size={17} /></Button></form>}{!config.passwordLoginEnabled && !config.githubLoginAvailable && <Notice tone="warning">{text("当前未启用任何登录方式，请联系站点管理员。", "No sign-in method is currently enabled. Contact the site administrator.")}</Notice>}{config.githubLoginAvailable && <>{config.passwordLoginEnabled && <div className="saas-login-divider"><span>{text("或者", "OR")}</span></div>}{!config.registrationEnabled && <Notice>{text("GitHub 注册已关闭，仅限已有账号登录。", "GitHub registration is closed. Existing accounts can still sign in.")}</Notice>}{config.inviteRegistrationRequired && <Field label={text("邀请码", "Invite code")}><input value={inviteCode} onChange={event => setInviteCode(event.target.value)} autoComplete="off" /></Field>}<a className="saas-button saas-login-button" href={"/api/saas/auth/github" + (inviteCode.trim() ? "?invite=" + encodeURIComponent(inviteCode.trim()) : "")}><Github size={18} />{text("使用 GitHub 继续", "Continue with GitHub")}</a><p className="saas-caption"><ShieldCheck size={14} />{text("GitHub 账号需已创建满 365 天。我们不会索取你的 GitHub 密码。", "Your GitHub account must be at least 365 days old. We never ask for your GitHub password.")}</p></>}</Card></main> : <div className="saas-workspace"><aside className={`saas-sidebar${menuOpen ? " saas-menu-open" : ""}`}><div className="saas-sidebar-label">{text("工作空间", "WORKSPACE")}</div><nav aria-label={text("工作空间", "Workspace")}>{navigation.map(item => <a key={item.path} href={item.path} aria-current={effectivePath === item.path ? "page" : undefined} onClick={event => link(event, item.path)}><item.icon size={18} />{item.title}</a>)}</nav><div className="saas-sidebar-bottom"><div className="saas-user-chip"><span className="saas-avatar">{(user.displayName || user.login).slice(0, 1).toUpperCase()}</span><span><strong>{user.displayName || user.login}</strong><small>{user.email || `@${user.login}`}</small></span></div><Button tone="quiet" onClick={logout} busy={action.busy}><LogOut size={16} />{text("退出登录", "Sign out")}</Button></div></aside><main ref={mainRef} tabIndex={-1} className="saas-main"><ActionFeedback action={action} />{config.announcement && <Notice>{config.announcement}</Notice>}{effectivePath === "/" ? <HomePage client={client} config={config} user={user} navigate={navigate} /> : effectivePath === "/usage" ? <UsagePage client={client} /> : effectivePath === "/api-keys" ? <KeysPage client={client} /> : effectivePath === "/recharge" ? <RechargePage client={client} config={config} /> : effectivePath === "/redeem" ? <RedeemPage client={client} /> : effectivePath === "/benefits" ? <GrowthPage client={client} config={config} /> : effectivePath === "/profile" ? <ProfilePage client={client} user={user} logout={logout} busy={action.busy} /> : <Empty title={text("页面不存在", "Page not found")} action={<Button onClick={() => navigate("/")}>{text("返回概览", "Back to overview")}</Button>} />}</main></div>}
    <footer className="saas-footer"><a href={REPOSITORY_URL} target="_blank" rel="noopener noreferrer">AISwitch</a><span>{text("让每一次请求都有价值。", "Make every request count.")}</span></footer>
  </>;
}
