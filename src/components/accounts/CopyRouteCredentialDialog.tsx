import { AlertTriangle, Copy, X } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import type {
  CopyRouteCredentialInput,
  PlatformId,
  RouteCredential,
} from "../../lib/api/types";

const platformOptions: Array<{ value: PlatformId; label: string }> = [
  { value: "codex", label: "Codex" },
  { value: "claude", label: "Claude" },
  { value: "gemini", label: "Gemini" },
  { value: "grok", label: "Grok" },
  { value: "opencode", label: "OpenCode" },
  { value: "openclaw", label: "OpenClaw" },
  { value: "hermes", label: "Hermes" },
];

type CopyRouteCredentialDialogProps = {
  credential: RouteCredential;
  sourcePlatform: PlatformId;
  loading: boolean;
  error: string | null;
  onClose: () => void;
  onSubmit: (input: CopyRouteCredentialInput) => void;
};

export function CopyRouteCredentialDialog({
  credential,
  sourcePlatform,
  loading,
  error,
  onClose,
  onSubmit,
}: CopyRouteCredentialDialogProps) {
  const official = credential.kind === "official";
  const [targetPlatform, setTargetPlatform] = useState<PlatformId>(sourcePlatform);
  const [apiKey, setApiKey] = useState("");
  const dialogRef = useRef<HTMLDivElement>(null);
  const targetSelectRef = useRef<HTMLSelectElement>(null);
  const closeRef = useRef(onClose);
  const loadingRef = useRef(loading);
  const crossPlatform = targetPlatform !== sourcePlatform;
  const targets = official
    ? platformOptions.filter((option) => option.value === sourcePlatform)
    : platformOptions;

  closeRef.current = onClose;
  loadingRef.current = loading;

  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    targetSelectRef.current?.focus();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
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
          'button:not([disabled]), input:not([disabled]), select:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
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

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmedApiKey = apiKey.trim();
    onSubmit({
      target_platform: targetPlatform,
      ...(trimmedApiKey ? { api_key: trimmedApiKey } : {}),
    });
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
        aria-labelledby="copy-route-credential-title"
        aria-modal="true"
        className="w-full max-w-md overflow-hidden rounded-lg border border-stone-200 bg-white shadow-2xl"
        role="dialog"
        tabIndex={-1}
      >
        <header className="flex items-start justify-between gap-4 border-b border-stone-200 px-5 py-4">
          <div className="min-w-0">
            <h2
              className="flex items-center gap-2 text-base font-semibold text-stone-950"
              id="copy-route-credential-title"
            >
              <Copy aria-hidden="true" className="h-4 w-4 text-stone-500" />
              复制账号
            </h2>
            <p className="mt-1 truncate text-xs text-stone-500" title={credential.display_name}>
              {credential.display_name}
            </p>
          </div>
          <button
            aria-label="关闭复制账号"
            className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-stone-500 transition-colors hover:bg-stone-100 hover:text-stone-900 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:opacity-50"
            disabled={loading}
            onClick={onClose}
            title="关闭"
            type="button"
          >
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </header>

        <form onSubmit={submit}>
          <div className="grid gap-4 px-5 py-4">
            <label className="grid gap-1.5 text-xs font-semibold text-stone-700">
              复制目标
              <select
                ref={targetSelectRef}
                aria-label="复制目标"
                className="h-10 w-full rounded-md border border-stone-300 bg-white px-3 text-sm font-normal text-stone-900 outline-none transition-colors focus:border-blue-500 focus:ring-2 focus:ring-blue-100 disabled:bg-stone-100 disabled:text-stone-600"
                disabled={official || loading}
                onChange={(event) => setTargetPlatform(event.target.value as PlatformId)}
                value={targetPlatform}
              >
                {targets.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>

            {official ? (
              <p className="rounded-md border border-stone-200 bg-stone-50 px-3 py-2.5 text-xs leading-5 text-stone-600">
                官方账号仅支持复制到当前智能体。
              </p>
            ) : (
              <label className="grid gap-1.5 text-xs font-semibold text-stone-700">
                新 API Key（可选）
                <input
                  aria-label="新 API Key（可选）"
                  autoComplete="off"
                  className="h-10 w-full rounded-md border border-stone-300 bg-white px-3 font-mono text-sm font-normal text-stone-900 outline-none transition-colors placeholder:font-sans placeholder:text-stone-400 focus:border-blue-500 focus:ring-2 focus:ring-blue-100"
                  disabled={loading}
                  onChange={(event) => setApiKey(event.target.value)}
                  placeholder="不填则复制原 API Key"
                  type="password"
                  value={apiKey}
                />
              </label>
            )}

            {crossPlatform ? (
              <div
                className="flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 px-3 py-2.5 text-xs leading-5 text-amber-950"
                role="alert"
              >
                <AlertTriangle
                  aria-hidden="true"
                  className="mt-0.5 h-4 w-4 shrink-0 text-amber-700"
                />
                <p>
                  复制到其他智能体不会保留模型映射、已获取模型等不兼容配置；Base URL
                  和接口格式会按目标智能体自动调整。
                </p>
              </div>
            ) : null}

            {error ? (
              <p className="rounded-md border border-red-200 bg-red-50 px-3 py-2.5 text-xs text-red-800" role="alert">
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
              className="h-9 rounded-md bg-stone-900 px-4 text-sm font-semibold text-white transition-colors hover:bg-stone-800 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:cursor-wait disabled:opacity-60"
              disabled={loading}
              type="submit"
            >
              {loading ? "复制中..." : "确认复制"}
            </button>
          </footer>
        </form>
      </div>
    </div>
  );
}
