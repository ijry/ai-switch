import { useCallback, useEffect, useId, useRef, useState, type ButtonHTMLAttributes, type DependencyList, type ReactNode } from "react";
import { AlertCircle, Check, CheckCircle2, ChevronLeft, ChevronRight, Copy, Inbox, LoaderCircle, RefreshCw, X } from "lucide-react";
import { errorMessage } from "../api";
import { useSaasLocale } from "../i18n";
import { formatCount } from "../format";

export function Button({ children, tone = "default", busy = false, className = "", disabled, ...props }: ButtonHTMLAttributes<HTMLButtonElement> & { tone?: "default" | "primary" | "danger" | "quiet"; busy?: boolean }) {
  return <button type="button" {...props} disabled={disabled || busy} className={`saas-button saas-${tone} ${className}`} aria-busy={busy || undefined}>{busy && <LoaderCircle size={16} className="saas-spin" />}{children}</button>;
}

export function Card({ title, action, children, className = "" }: { title?: ReactNode; action?: ReactNode; children: ReactNode; className?: string }) {
  return <section className={`saas-card ${className}`}>{(title || action) && <header className="saas-card-heading"><h2>{title}</h2>{action}</header>}{children}</section>;
}

export function Heading({ eyebrow, title, description, action }: { eyebrow?: string; title: string; description?: string; action?: ReactNode }) {
  return <header className="saas-heading"><div>{eyebrow && <p className="saas-eyebrow">{eyebrow}</p>}<h1>{title}</h1>{description && <p className="saas-muted">{description}</p>}</div>{action}</header>;
}

export function Field({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return <div className="saas-field"><label><span>{label}</span>{children}</label>{hint && <small className="saas-muted">{hint}</small>}</div>;
}

export function CheckField({ label, checked, onChange, disabled, hint }: { label: string; checked: boolean; onChange: (checked: boolean) => void; disabled?: boolean; hint?: string }) {
  return <label className="saas-check"><input type="checkbox" checked={checked} disabled={disabled} onChange={event => onChange(event.target.checked)} /><span>{label}{hint && <small>{hint}</small>}</span></label>;
}

export function Notice({ children, tone = "info" }: { children: ReactNode; tone?: "error" | "success" | "info" | "warning" }) {
  return <div className={`saas-notice saas-notice-${tone}`} role={tone === "error" ? "alert" : tone === "success" ? "status" : undefined}>{tone === "success" ? <CheckCircle2 size={18} /> : <AlertCircle size={18} />}<div>{children}</div></div>;
}

export function Empty({ title, description, action }: { title?: string; description?: string; action?: ReactNode }) {
  const { text } = useSaasLocale();
  return <div className="saas-empty"><Inbox size={29} strokeWidth={1.4} /><strong>{title || text("暂无记录", "No records yet")}</strong><p>{description || text("有真实数据后会显示在这里。", "Records will appear here when available.")}</p>{action}</div>;
}

export function Loading() {
  const { text } = useSaasLocale();
  return <div className="saas-loading" role="status"><LoaderCircle className="saas-spin" size={20} />{text("正在加载…", "Loading…")}</div>;
}

export function ErrorState({ error, retry }: { error: unknown; retry?: () => void }) {
  const { text } = useSaasLocale();
  return <Notice tone="error"><p>{errorMessage(error)}</p>{retry && <Button onClick={retry}><RefreshCw size={15} />{text("重试", "Retry")}</Button>}</Notice>;
}

export function Status({ value }: { value: string }) {
  const { text } = useSaasLocale();
  const labels: Record<string, [string, string]> = { active: ["正常", "Active"], enabled: ["启用", "Enabled"], disabled: ["停用", "Disabled"], banned: ["封禁", "Banned"], pending: ["待审核", "Pending"], pending_review: ["待核对", "Needs reconciliation"], approved: ["已到账", "Approved"], rejected: ["已拒绝", "Rejected"], cancelled: ["已取消", "Cancelled"], used: ["已兑换", "Redeemed"], redeemed: ["已兑换", "Redeemed"], expired: ["已过期", "Expired"], success: ["成功", "Success"], settled: ["已结算", "Settled"], failed: ["失败", "Failed"], released: ["已释放", "Released"], reserved: ["已预占", "Reserved"] };
  return <span className={`saas-badge saas-status-${value.replace(/[^a-z_]/g, "")}`}><span className="saas-status-dot" />{labels[value] ? text(...labels[value]) : value}</span>;
}

export function Table<Item>({ items, columns, rowKey, empty }: { items: Item[]; columns: { label: string; render: (item: Item) => ReactNode }[]; rowKey: (item: Item) => string; empty?: ReactNode }) {
  return items.length ? <div className="saas-table-scroll"><table className="saas-table"><thead><tr>{columns.map(column => <th key={column.label} scope="col">{column.label}</th>)}</tr></thead><tbody>{items.map(item => <tr key={rowKey(item)}>{columns.map(column => <td key={column.label}>{column.render(item)}</td>)}</tr>)}</tbody></table></div> : <>{empty || <Empty />}</>;
}

export function Pagination({ page, total, pageSize = 20, onPage }: { page: number; total: number; pageSize?: number; onPage: (page: number) => void }) {
  const { locale, text } = useSaasLocale();
  return <div className="saas-pagination"><span>{text("共", "Total")} {formatCount(total, locale)} {text("条", "records")}</span><div className="saas-row"><Button aria-label={text("上一页", "Previous page")} disabled={page <= 1} onClick={() => onPage(page - 1)}><ChevronLeft size={16} /></Button><span>{page} / {Math.max(1, Math.ceil(total / pageSize))}</span><Button aria-label={text("下一页", "Next page")} disabled={page * pageSize >= total} onClick={() => onPage(page + 1)}><ChevronRight size={16} /></Button></div></div>;
}

export function Dialog({ title, children, onClose, busy = false, wide = false }: { title: string; children: ReactNode; onClose: () => void; busy?: boolean; wide?: boolean }) {
  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  const busyRef = useRef(busy);
  closeRef.current = onClose;
  busyRef.current = busy;
  const { text } = useSaasLocale();
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialogRef.current?.focus();
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape" && !busyRef.current) { event.preventDefault(); closeRef.current(); }
      if (event.key === "Tab") {
        const focusable = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex="0"]') || []);
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (!first) { event.preventDefault(); return; }
        if (event.shiftKey && (document.activeElement === first || document.activeElement === dialogRef.current)) { event.preventDefault(); last.focus(); }
        else if (!event.shiftKey && (document.activeElement === last || document.activeElement === dialogRef.current)) { event.preventDefault(); first.focus(); }
      }
    }
    document.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("keydown", onKey); previous?.focus(); };
  }, []);
  return <div className="saas-overlay" onMouseDown={event => { if (event.target === event.currentTarget && !busy) onClose(); }}><div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1} className={`saas-dialog${wide ? " saas-dialog-wide" : ""}`}><header><h2 id={titleId}>{title}</h2><Button tone="quiet" aria-label={text("关闭", "Close")} disabled={busy} onClick={onClose}><X size={18} /></Button></header>{children}</div></div>;
}

export function CopyButton({ value }: { value: string }) {
  const { text } = useSaasLocale();
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => { setCopied(false); setError(""); }, [value]);
  return <><Button disabled={!value} onClick={async () => {
    try { await navigator.clipboard.writeText(value); setCopied(true); setError(""); }
    catch { setError(text("复制失败，请手动选择并复制。", "Copy failed. Select and copy the text manually.")); }
  }}>{copied ? <Check size={15} /> : <Copy size={15} />}{copied ? text("已复制", "Copied") : text("复制", "Copy")}</Button>{error && <span role="alert" className="saas-error-text">{error}</span>}</>;
}

export function useResource<Result>(load: () => Promise<Result>, dependencies: DependencyList = []) {
  const loader = useRef(load);
  loader.current = load;
  const [version, setVersion] = useState(0);
  const [state, setState] = useState<{ data?: Result; error?: unknown; loading: boolean }>({ loading: true });
  const dependencyKey = JSON.stringify(dependencies);
  useEffect(() => {
    let active = true;
    setState({ loading: true });
    loader.current().then(data => { if (active) setState({ data, loading: false }); }).catch(error => { if (active) setState({ error, loading: false }); });
    return () => { active = false; };
  }, [dependencyKey, version]);
  const reload = useCallback(() => setVersion(current => current + 1), []);
  return { ...state, reload };
}

export function useAction() {
  const lock = useRef(false);
  const active = useRef(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const [success, setSuccess] = useState("");
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  async function run<Result>(action: () => Promise<Result>, done?: (result: Result) => void) {
    if (lock.current) return;
    lock.current = true; setBusy(true); setError(undefined); setSuccess("");
    try { const result = await action(); if (active.current) done?.(result); }
    catch (failure) { if (active.current) setError(failure); }
    finally { lock.current = false; if (active.current) setBusy(false); }
  }
  return { busy, error, success, run, setSuccess, clear: () => { setError(undefined); setSuccess(""); } };
}

export function ActionFeedback({ action }: { action: ReturnType<typeof useAction> }) {
  return <>{action.error != null && <ErrorState error={action.error} />}{action.success && <Notice tone="success">{action.success}</Notice>}</>;
}

export function Metric({ label, value, detail, icon }: { label: string; value: string; detail?: ReactNode; icon?: ReactNode }) {
  return <div className="saas-metric"><div className="saas-row saas-between"><span>{label}</span>{icon}</div><strong>{value}</strong>{detail && <small>{detail}</small>}</div>;
}
