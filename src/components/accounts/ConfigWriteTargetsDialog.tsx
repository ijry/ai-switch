import { Check, Copy, Eye, EyeOff, Plug, X } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import type { ConfigWriteClientStatus } from "../../lib/api/types";
import { copySensitiveText } from "../../lib/routeCredentialTransfer";

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
            className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-stone-200 bg-white text-stone-600 transition-colors hover:bg-stone-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500"
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
          className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-stone-200 bg-white text-stone-600 transition-colors hover:bg-stone-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:cursor-not-allowed disabled:opacity-50"
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
  const dialogRef = useRef<HTMLDivElement>(null);
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
      className="fixed inset-0 z-[80] flex items-center justify-center bg-stone-950/45 p-4 backdrop-blur-sm"
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
        className="flex max-h-[calc(100vh-2rem)] w-full max-w-md flex-col overflow-hidden rounded-lg border border-stone-200 bg-white shadow-2xl"
        role="dialog"
        tabIndex={-1}
      >
        <header className="flex items-start justify-between gap-4 border-b border-stone-200 px-5 py-4">
          <div className="min-w-0">
            <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
              {platform}
            </p>
            <h2
              className="mt-0.5 flex items-center gap-2 text-base font-semibold text-stone-950"
              id="config-write-targets-title"
            >
              <Plug aria-hidden="true" className="h-4 w-4 text-stone-500" />
              选择要写入的客户端
            </h2>
            <p className="mt-1 text-xs leading-5 text-stone-500">
              只写入勾选的客户端，其他客户端的配置文件不会被改动。
            </p>
          </div>
          <button
            aria-label="关闭选择要写入的客户端"
            className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-stone-500 transition-colors hover:bg-stone-100 hover:text-stone-900 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:opacity-50"
            disabled={loading}
            onClick={onClose}
            title="关闭"
            type="button"
          >
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </header>

        <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit}>
          <div className="grid min-h-0 flex-1 gap-3 overflow-y-auto px-5 py-4">
            {capabilityDisabledReason ? (
              <p
                className="rounded-md border border-amber-200 bg-amber-50 px-3 py-2.5 text-xs leading-5 text-amber-950"
                role="alert"
              >
                {capabilityDisabledReason}
              </p>
            ) : null}

            {clients.length === 0 ? (
              <p className="rounded-md border border-stone-200 bg-stone-50 px-3 py-2.5 text-xs leading-5 text-stone-600">
                暂无可写入的客户端。
              </p>
            ) : (
              <ul className="grid gap-2">
                {clients.map((client) => (
                  <li key={client.client_key}>
                    <label className="flex items-start gap-2.5 rounded-md border border-stone-200 px-3 py-2.5 text-xs text-stone-700">
                      <input
                        checked={selected.includes(client.client_key)}
                        className="mt-0.5 h-3.5 w-3.5 shrink-0 accent-stone-900"
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
              <p className="rounded-md border border-stone-200 bg-stone-50 px-3 py-2.5 text-xs leading-5 text-stone-600">
                写入后需重启 {restartNames} 才生效（它不监听配置文件变化）。
              </p>
            ) : null}

            <section className="grid gap-2.5 rounded-md border border-stone-200 bg-stone-50 px-3 py-3">
              <div className="grid gap-0.5">
                <h3 className="text-xs font-semibold text-stone-950">在以上客户端之外使用</h3>
                <p className="text-[11px] leading-5 text-stone-500">
                  如果你需要在以上客户端之外的情况使用当前算力池端点，可以用下面的参数手动配置。
                </p>
              </div>
              <EndpointRow label="Base URL" value={poolBaseUrl} />
              <EndpointRow label="API Key" secret value={poolApiKey} />
              <p className="text-[11px] leading-5 text-amber-800">
                {`注意：每个智能体标签页的算力池端点 API Key 都不一样。这里给出的是 ${platformLabel ?? platform} 标签页的 Key，别的标签页要各自复制。`}
              </p>
            </section>

            {error ? (
              <p
                className="rounded-md border border-red-200 bg-red-50 px-3 py-2.5 text-xs leading-5 text-red-800"
                role="alert"
              >
                {error}
              </p>
            ) : null}
          </div>

          <footer className="flex justify-end gap-2 border-t border-stone-200 bg-stone-50 px-5 py-3">
            <button
              className="h-9 rounded-md border border-stone-300 bg-white px-4 text-sm font-semibold text-stone-700 transition-colors hover:bg-stone-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:opacity-50"
              disabled={loading}
              onClick={onClose}
              type="button"
            >
              取消
            </button>
            <button
              className="h-9 rounded-md bg-stone-900 px-4 text-sm font-semibold text-white transition-colors hover:bg-stone-800 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:cursor-not-allowed disabled:opacity-60"
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
