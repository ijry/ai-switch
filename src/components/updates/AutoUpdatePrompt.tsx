import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { Download, RefreshCw, RotateCcw, ShieldAlert, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useI18n } from "../../lib/i18n";
import { isDesktop } from "../../lib/transport";

const LAST_UPDATE_CHECK_STORAGE_KEY = "ai-switch.last-update-check";

type DownloadState = {
  downloaded: number;
  total?: number;
};

function localDateKey() {
  const now = new Date();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

function formatBytes(value: number) {
  if (value < 1024) {
    return `${value} B`;
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KB`;
  }
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

export function AutoUpdatePrompt() {
  const { t } = useI18n();
  const [update, setUpdate] = useState<Update | null>(null);
  const [installing, setInstalling] = useState(false);
  const [installed, setInstalled] = useState(false);
  const [download, setDownload] = useState<DownloadState>({ downloaded: 0 });
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isDesktop() || typeof window === "undefined") {
      return;
    }

    const today = localDateKey();
    try {
      if (window.localStorage.getItem(LAST_UPDATE_CHECK_STORAGE_KEY) === today) {
        return;
      }
      window.localStorage.setItem(LAST_UPDATE_CHECK_STORAGE_KEY, today);
    } catch {
      // Restricted webviews may not provide storage; still try the check once.
    }

    let cancelled = false;
    void check()
      .then((nextUpdate) => {
        if (!cancelled && nextUpdate) {
          setUpdate(nextUpdate);
        }
      })
      .catch(() => {
        // Automatic checks are silent when the endpoint is unavailable.
      });

    return () => {
      cancelled = true;
    };
  }, []);

  if (!update) {
    return null;
  }

  const progress = download.total
    ? Math.min(100, Math.round((download.downloaded / download.total) * 100))
    : 0;

  const installUpdate = async () => {
    if (installing || installed) {
      return;
    }

    setInstalling(true);
    setError(null);
    setDownload({ downloaded: 0 });
    try {
      let downloaded = 0;
      let total: number | undefined;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength;
          downloaded = 0;
          setDownload({ downloaded, total });
          return;
        }
        if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setDownload({ downloaded, total });
          return;
        }
        if (event.event === "Finished") {
          setDownload((current) => ({
            ...current,
            downloaded: current.total ?? current.downloaded,
          }));
        }
      });
      setInstalled(true);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setInstalling(false);
    }
  };

  const relaunchApp = async () => {
    setError(null);
    try {
      await relaunch();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  return (
    <div className="fixed inset-0 z-[100] grid place-items-center bg-stone-950/35 p-4 backdrop-blur-sm">
      <div
        aria-label={t("updates.promptTitle")}
        aria-modal="true"
        className="w-full max-w-md rounded-2xl border border-stone-200 bg-white p-5 shadow-2xl"
        role="dialog"
      >
        <div className="flex items-start justify-between gap-4">
          <div className="flex min-w-0 items-start gap-3">
            <span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-amber-50 text-amber-700">
              <Download aria-hidden="true" className="h-4 w-4" />
            </span>
            <div className="min-w-0">
              <h2 className="text-base font-semibold text-stone-950">{t("updates.promptTitle")}</h2>
              <p className="mt-1 text-[13px] text-stone-600">
                {t("updates.promptBody", { version: update.version })}
              </p>
            </div>
          </div>
          {!installing && !installed && (
            <button
              aria-label={t("updates.promptLater")}
              className="grid h-7 w-7 shrink-0 place-items-center rounded-lg text-stone-400 transition-colors hover:bg-stone-100 hover:text-stone-700"
              onClick={() => setUpdate(null)}
              title={t("updates.promptLater")}
              type="button"
            >
              <X aria-hidden="true" className="h-4 w-4" />
            </button>
          )}
        </div>

        {update.body && (
          <pre className="mt-4 max-h-36 overflow-auto whitespace-pre-wrap rounded-xl border border-stone-200 bg-stone-50 p-3 text-[12px] leading-5 text-stone-600">
            {update.body}
          </pre>
        )}

        {error && (
          <div className="mt-3 flex items-start gap-2 rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-[12px] text-red-700">
            <ShieldAlert className="mt-0.5 h-4 w-4 shrink-0" />
            <span>{error}</span>
          </div>
        )}

        {installing && (
          <div className="mt-4 rounded-xl border border-stone-200 bg-stone-50 p-3">
            <div className="flex items-center justify-between text-[12px] font-semibold text-stone-600">
              <span>{t("updates.promptInstalling")}</span>
              <span>
                {formatBytes(download.downloaded)}
                {download.total ? ` / ${formatBytes(download.total)}` : ""}
              </span>
            </div>
            <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-stone-200">
              <div className="h-full rounded-full bg-amber-500 transition-all" style={{ width: `${progress}%` }} />
            </div>
          </div>
        )}

        <div className="mt-5 flex justify-end gap-2 border-t border-stone-100 pt-3">
          {!installed && (
            <button
              className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 transition-colors hover:bg-stone-50 disabled:opacity-50"
              disabled={installing}
              onClick={() => setUpdate(null)}
              type="button"
            >
              {t("updates.promptLater")}
            </button>
          )}
          <button
            className="inline-flex items-center gap-1.5 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800 disabled:cursor-not-allowed disabled:bg-stone-400"
            disabled={installing}
            onClick={installed ? () => void relaunchApp() : () => void installUpdate()}
            type="button"
          >
            {installed ? <RotateCcw aria-hidden="true" className="h-4 w-4" /> : <RefreshCw aria-hidden="true" className={`h-4 w-4 ${installing ? "animate-spin" : ""}`} />}
            {installed ? t("updates.relaunch") : installing ? t("updates.promptInstalling") : t("updates.promptUpdate")}
          </button>
        </div>
      </div>
    </div>
  );
}
