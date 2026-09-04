import { Check, Copy, Eye, EyeOff, Plug, X } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import type { ConfigWriteClientStatus } from "../../lib/api/types";
import { copySensitiveText } from "../../lib/routeCredentialTransfer";
import { routeProxyEndpointForPlatform } from "../../lib/routeProxyEndpoint";

const fileStatusLabels: Record<string, string> = {
  missing: "未建立",
  managed: "已接管",
  unmanaged: "未接管",
  invalid: "无法解析",
  error: "无法读取",
};

/** The platform's first-party CLI: what a user who never chose gets today. */
function nativeClientKeys(clients: ConfigWriteClientStatus[]): string[] {
  return clients.filter((client) => client.native).map((client) => client.client_key);
}

type EndpointRowProps = {
  label: string;
  /** `null` while the value is still being read. */
  value: string | null;
  /** Masked behind a reveal toggle: the pool key routes to every pooled account. */
  secret?: boolean;
};

/** One copyable connection parameter for clients this dialog cannot write. */
function EndpointRow({ label, value, secret = false }: EndpointRowProps) {
  const [copied, setCopied] = useState(false);
  const [revealed, setRevealed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const copiedTimerRef = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (copiedTimerRef.current !== null) {
        window.clearTimeout(copiedTimerRef.current);
      }
    },
    [],
  );

  const copy = async () => {
    if (!value) {
      return;
    }
    try {
      await copySensitiveText(value);
      setError(null);
      setCopied(true);
      if (copiedTimerRef.current !== null) {
        window.clearTimeout(copiedTimerRef.current);
      }
      copiedTimerRef.current = window.setTimeout(() => setCopied(false), 1400);
    } catch {
      setError("复制失败：剪贴板不可用。");
    }
  };

  return (
    <div className="grid gap-1">
      <span className="text-[11px] font-semibold text-stone-600">{label}</span>
      <div className="flex items-center gap-1.5">
        <input
          aria-label={label}
          autoComplete="off"
          className="h-8 min-w-0 flex-1 rounded-md border border-stone-200 bg-white px-2.5 font-mono text-[11px] text-stone-800 outline-none placeholder:font-sans placeholder:text-stone-400"
          onKeyDown={(event) => {
            // The field lives inside the write form, where Enter would submit it.
            if (event.key === "Enter") {
              event.preventDefault();
            }
          }}
          placeholder="读取中..."
          readOnly
          type={secret && !revealed ? "password" : "text"}
          value={value ?? ""}
        />
        {secret ? (
          <button
            aria-label={revealed ? `隐藏 ${label}` : `显示 ${label}`}
            className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-stone-200 bg-white text-stone-600 motion-control hover:bg-stone-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500"
            onClick={() => setRevealed((current) => !current)}
            title={revealed ? "隐藏" : "显示"}
            type="button"
          >
            {revealed ? (
              <EyeOff aria-hidden="true" className="h-3.5 w-3.5" />
            ) : (
              <Eye aria-hidden="true" className="h-3.5 w-3.5" />
            )}
          </button>
        ) : null}
        <button
          aria-label={`复制 ${label}`}
          className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-stone-200 bg-white text-stone-600 motion-control hover:bg-stone-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:cursor-not-allowed disabled:opacity-50"
          disabled={!value}
          onClick={() => void copy()}
          title={value ? `复制 ${label}` : "本地路由代理尚未就绪"}
          type="button"
        >
          {copied ? (
            <Check aria-hidden="true" className="h-3.5 w-3.5 text-emerald-600" />
          ) : (
            <Copy aria-hidden="true" className="h-3.5 w-3.5" />
          )}
        </button>
      </div>
      {error ? (
        <p className="text-[11px] text-red-700" role="alert">
          {error}
        </p>
      ) : null}
    </div>
  );
}

/**
 * `builtin` writes files for us; `manual` only hands out the parameters. They are
 * tabs rather than one scrolling column because the endpoint rows used to sit
 * below the client list, where nobody who did not scroll knew they existed.
 */
type ConfigWriteTab = "builtin" | "manual";

type ConfigWriteTargetsDialogProps = {
  platform: string;
  /** Human-readable name of the agent tab, for prose that names it. */
  platformLabel?: string;
  clients: ConfigWriteClientStatus[];
  /** `null` means the user has never chosen, so only the native client is checked. */
  initialSelection: string[] | null;
  capabilityDisabledReason?: string;
  /** Pool endpoint for clients this dialog cannot write; `null` until it is read. */
  poolBaseUrl?: string | null;
  /**
   * The HTTPS endpoint, on its own port beside the HTTP one. `null` when HTTPS is
   * off or could not start. Never written into a config — only pasted by hand.
   */
  poolHttpsBaseUrl?: string | null;
  /** Why HTTPS is absent even though it was turned on. */
  httpsError?: string | null;
  poolApiKey?: string | null;
  loading: boolean;
  error: string | null;
  onClose: () => void;
  onSubmit: (clientKeys: string[]) => void;
};

export function ConfigWriteTargetsDialog({
  platform,
  platformLabel,
  clients,
  initialSelection,
  capabilityDisabledReason,
  poolBaseUrl = null,
  poolHttpsBaseUrl = null,
  httpsError = null,
  poolApiKey = null,
  loading,
  error,
  onClose,
  onSubmit,
}: ConfigWriteTargetsDialogProps) {
  // `null` until the user touches a checkbox, which keeps the default derived
  // from `clients`. The list arrives from a query after the dialog mounts, so a
  // snapshot taken on the first render would leave nothing checked.
  const [override, setOverride] = useState<string[] | null>(null);
  // `null` until the user picks a tab, for the same reason: `capabilityDisabledReason`
  // may only arrive after the capability query settles, and a platform that cannot
  // be written at all should open on the parameters it can actually use.
  const [tabOverride, setTabOverride] = useState<ConfigWriteTab | null>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const builtinTabRef = useRef<HTMLButtonElement>(null);
  const manualTabRef = useRef<HTMLButtonElement>(null);
  const closeRef = useRef(onClose);
  const loadingRef = useRef(loading);

  closeRef.current = onClose;
  loadingRef.current = loading;

  useEffect(() => {
    const previousFocus =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const firstCheckbox = dialogRef.current?.querySelector<HTMLInputElement>(
      "input[type=checkbox]",
    );
    (firstCheckbox ?? dialogRef.current)?.focus();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        // A write in flight cannot be cancelled, so closing would only hide its
        // outcome from the user.
        if (!loadingRef.current) {
          event.preventDefault();
          closeRef.current();
        }
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) {
        return;
      }

      const focusable = Array.from(
        dialogRef.current.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
        ),
      ).filter(
        // The unselected tab keeps `tabindex="-1"` for roving focus and the browser
        // skips it, so counting it as a stop would break the wrap-around.
        (element) => element.tabIndex >= 0,
      );
      if (!focusable.length) {
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      if (previousFocus?.isConnected) {
        previousFocus.focus();
      }
    };
  }, []);

  const selected = override ?? initialSelection ?? nativeClientKeys(clients);
  const disabled = Boolean(capabilityDisabledReason);
  const activeTab = tabOverride ?? (disabled ? "manual" : "builtin");
  // `null` rather than `""` so `EndpointRow` shows its "reading..." placeholder
  // instead of an empty field before the proxy status arrives.
  const httpEndpoint = routeProxyEndpointForPlatform(poolBaseUrl ?? "", platform) || null;
  const httpsEndpoint = routeProxyEndpointForPlatform(poolHttpsBaseUrl ?? "", platform) || null;
  const restartClients = clients.filter(
    (client) => client.restart_required && selected.includes(client.client_key),
  );
  const restartNames = restartClients.map((client) => client.display_name).join("、");

  const toggle = (clientKey: string) => {
    setOverride(
      selected.includes(clientKey)
        ? selected.filter((key) => key !== clientKey)
        : [...selected, clientKey],
    );
  };

  const handleTabKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>) => {
    const tabs = [builtinTabRef.current, manualTabRef.current];
    const currentIndex = tabs.indexOf(event.currentTarget);
    let nextIndex: number | null = null;

    if (event.key === "ArrowRight") {
      nextIndex = (currentIndex + 1) % tabs.length;
    } else if (event.key === "ArrowLeft") {
      nextIndex = (currentIndex - 1 + tabs.length) % tabs.length;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = tabs.length - 1;
    }

    if (nextIndex === null) {
      return;
    }

    event.preventDefault();
    setTabOverride(nextIndex === 0 ? "builtin" : "manual");
    tabs[nextIndex]?.focus();
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    // An empty write would report success and change nothing.
    if (disabled || loading || selected.length === 0) {
      return;
    }
    // Submit in list order so the result panel lists clients predictably.
    onSubmit(
      clients
        .map((client) => client.client_key)
        .filter((clientKey) => selected.includes(clientKey)),
    );
  };

  return (
    <div
      className="motion-overlay fixed inset-0 z-[80] flex items-center justify-center bg-stone-950/45 p-4 backdrop-blur-sm"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !loading) {
          onClose();
        }
      }}
    >
      <div
        ref={dialogRef}
        aria-labelledby="config-write-targets-title"
        aria-modal="true"
        className="flex max-h-[calc(100vh-2rem)] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-stone-200 bg-white shadow-2xl"
        role="dialog"
        tabIndex={-1}
      >
        <header className="flex items-start justify-between gap-4 border-b border-stone-200 px-6 py-4">
          <div className="min-w-0">
            <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
              {platform}
            </p>
            <h2
              className="mt-0.5 flex items-center gap-2 text-base font-semibold text-stone-950"
              id="config-write-targets-title"
            >
              <Plug aria-hidden="true" className="h-4 w-4 text-stone-500" />
              接入算力池
            </h2>
            <p className="mt-1 text-xs leading-5 text-stone-500">
              内置支持的客户端可以直接写入配置；其他 Agent 手动填 Base URL 与 API Key。
            </p>
          </div>
          <button
            aria-label="关闭接入算力池"
            className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-stone-500 motion-control hover:bg-stone-100 hover:text-stone-900 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:opacity-50"
            disabled={loading}
            onClick={onClose}
            title="关闭"
            type="button"
          >
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </header>

        <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit}>
          <div className="px-6 pt-4">
            <div
              aria-label="接入方式"
              className="flex w-fit rounded-md bg-stone-100 p-1"
              role="tablist"
            >
              <button
                ref={builtinTabRef}
                aria-controls="config-write-builtin-panel"
                aria-selected={activeTab === "builtin"}
                className={`rounded px-3 py-1.5 text-xs font-semibold motion-control focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 ${
                  activeTab === "builtin"
                    ? "bg-stone-900 text-white"
                    : "text-stone-600 hover:text-stone-950"
                }`}
                id="config-write-builtin-tab"
                onClick={() => setTabOverride("builtin")}
                onKeyDown={handleTabKeyDown}
                role="tab"
                tabIndex={activeTab === "builtin" ? 0 : -1}
                type="button"
              >
                内置支持
              </button>
              <button
                ref={manualTabRef}
                aria-controls="config-write-manual-panel"
                aria-selected={activeTab === "manual"}
                className={`rounded px-3 py-1.5 text-xs font-semibold motion-control focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 ${
                  activeTab === "manual"
                    ? "bg-stone-900 text-white"
                    : "text-stone-600 hover:text-stone-950"
                }`}
                id="config-write-manual-tab"
                onClick={() => setTabOverride("manual")}
                onKeyDown={handleTabKeyDown}
                role="tab"
                tabIndex={activeTab === "manual" ? 0 : -1}
                type="button"
              >
                其他 Agent
              </button>
            </div>
          </div>

          <div className="grid min-h-0 flex-1 gap-3 overflow-y-auto px-6 py-4">
            {/* Dialog-level, not per tab: it is why 写入 is dead, and the tab it
                would sit in is the one the user gets sent away from. */}
            {capabilityDisabledReason ? (
              <p
                className="rounded-md border border-amber-200 bg-amber-50 px-3 py-2.5 text-xs leading-5 text-amber-950"
                role="alert"
              >
                {capabilityDisabledReason}
              </p>
            ) : null}

            {activeTab === "builtin" ? (
              <div
                aria-labelledby="config-write-builtin-tab"
                className="grid gap-3"
                id="config-write-builtin-panel"
                role="tabpanel"
              >
                <p className="text-xs leading-5 text-stone-500">
                  只写入勾选的客户端，其他客户端的配置文件不会被改动。
                </p>

                {clients.length === 0 ? (
                  <p className="rounded-md bg-stone-50 px-3.5 py-3 text-xs leading-5 text-stone-600">
                    暂无可写入的客户端。
                  </p>
                ) : (
                  <ul className="grid list-none gap-2">
                    {clients.map((client) => (
                      <li key={client.client_key}>
                        <label
                          className={`flex items-start gap-3 rounded-md bg-stone-50 px-3.5 py-3 text-xs text-stone-700 ${
                            disabled || loading ? "" : "cursor-pointer hover:bg-stone-100"
                          }`}
                        >
                          <input
                            checked={selected.includes(client.client_key)}
                            className="mt-0.5 h-4 w-4 shrink-0 accent-stone-900"
                            disabled={disabled || loading}
                            onChange={() => toggle(client.client_key)}
                            type="checkbox"
                          />
                          <span className="min-w-0 grid gap-0.5">
                            <span className="flex items-center gap-2">
                              <span className="font-semibold text-stone-950">
                                {client.display_name}
                              </span>
                              <span className="text-stone-500">
                                {fileStatusLabels[client.file_status] ?? client.file_status}
                              </span>
                            </span>
                            {client.config_path ? (
                              <span
                                className="truncate font-mono text-[11px] text-stone-500"
                                title={client.config_path}
                              >
                                {client.config_path}
                              </span>
                            ) : null}
                            {client.error_code ? (
                              <span className="font-mono text-[11px] text-red-700">
                                {client.error_code}
                              </span>
                            ) : null}
                          </span>
                        </label>
                      </li>
                    ))}
                  </ul>
                )}

                {restartClients.length > 0 ? (
                  <p className="rounded-md border border-amber-200 bg-amber-50 px-3.5 py-3 text-xs leading-5 text-amber-950">
                    写入后需重启 {restartNames} 才生效（它不监听配置文件变化）。
                  </p>
                ) : null}
              </div>
            ) : (
              <div
                aria-labelledby="config-write-manual-tab"
                className="grid gap-3"
                id="config-write-manual-panel"
                role="tabpanel"
              >
                <p className="text-xs leading-5 text-stone-500">
                  内置支持之外的 Agent 或工具，把下面的参数手动填进它自己的设置里，一样走当前算力池。
                </p>
                <div className="grid gap-2.5 rounded-md bg-stone-50 px-3.5 py-3">
                  <EndpointRow label="Base URL" value={httpEndpoint} />
                  {/* HTTPS has its own port and is never written into a config:
                      clients that ship their own CA bundle (curl, Node-based CLIs)
                      cannot see the local root certificate in the system trust store,
                      so an https:// address would simply break them. Rendered only
                      when it exists, so the row never sits empty. */}
                  {httpsEndpoint ? (
                    <EndpointRow label="HTTPS Base URL" value={httpsEndpoint} />
                  ) : null}
                  <EndpointRow label="API Key" secret value={poolApiKey} />
                </div>
                <p className="text-[11px] leading-5 text-stone-500">
                  {httpsEndpoint
                    ? "正常情况用上面的 Base URL（HTTP）即可。HTTPS 端点只给确实需要 TLS 的特殊场景，且客户端必须信任本地根证书。"
                    : httpsError
                      ? // Saying "go turn HTTPS on" would be wrong here — they did,
                        // and it is the listener that failed.
                        "正常情况用上面的 Base URL（HTTP）即可。HTTPS 端点本次未能启动，原因见设置里的 HTTPS 面板。"
                      : "正常情况用上面的 Base URL（HTTP）即可。需要 TLS 的特殊场景可在设置里开启 HTTPS，它会另占一个端口。"}
                </p>
                <p className="text-[11px] leading-5 text-amber-800">
                  {`注意：每个智能体标签页的算力池端点 API Key 都不一样。这里给出的是 ${platformLabel ?? platform} 标签页的 Key，别的标签页要各自复制。`}
                </p>
              </div>
            )}
          </div>

          {error ? (
            <p
              className="mx-6 mb-3 rounded-md border border-red-200 bg-red-50 px-3 py-2.5 text-xs leading-5 text-red-800"
              role="alert"
            >
              {error}
            </p>
          ) : null}

          <footer className="flex justify-end gap-2 border-t border-stone-200 bg-stone-50 px-6 py-3">
            <button
              className="h-9 rounded-md border border-stone-300 bg-white px-4 text-sm font-semibold text-stone-700 motion-control hover:bg-stone-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:opacity-50"
              disabled={loading}
              onClick={onClose}
              type="button"
            >
              取消
            </button>
            <button
              className="h-9 rounded-md bg-stone-900 px-4 text-sm font-semibold text-white motion-control hover:bg-stone-800 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:cursor-not-allowed disabled:opacity-60"
              disabled={disabled || loading || selected.length === 0}
              type="submit"
            >
              {loading ? "写入中..." : "写入"}
            </button>
          </footer>
        </form>
      </div>
    </div>
  );
}
