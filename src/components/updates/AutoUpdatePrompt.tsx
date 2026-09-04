import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { Download, RefreshCw, RotateCcw, ShieldAlert, X } from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useI18n } from "../../lib/i18n";
import { isDesktop } from "../../lib/transport";
import { ReleaseNotes } from "./ReleaseNotes";

const AUTO_UPDATE_CHECK_INTERVAL_MS = 60 * 60 * 1000;

type DownloadState = {
  downloaded: number;
  total?: number;
};

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
  const checkInProgressRef = useRef(false);

  useEffect(() => {
    if (!isDesktop() || typeof window === "undefined") {
      return;
    }

    let cancelled = false;
    const checkForUpdate = async () => {
      if (checkInProgressRef.current) {
        return;
      }

      checkInProgressRef.current = true;
      try {
        const nextUpdate = await check();
        if (!cancelled && nextUpdate) {
          setUpdate(nextUpdate);
        }
      } catch {
      } finally {
        checkInProgressRef.current = false;
      }
    };

    void checkForUpdate();
    const intervalId = window.setInterval(() => {
      void checkForUpdate();
    }, AUTO_UPDATE_CHECK_INTERVAL_MS);

    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, []);

  const progress = download.total
    ? Math.min(100, Math.round((download.downloaded / download.total) * 100))
    : 0;

  const installUpdate = async () => {
    if (!update || installing || installed) {
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

  return update ? (
        <motion.div
          key="auto-update-prompt"
          className="fixed inset-0 z-[100] grid place-items-center bg-stone-950/35 p-4 backdrop-blur-sm"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
        >
      <motion.div
        aria-label={t("updates.promptTitle")}
        aria-modal="true"
        className="motion-dialog w-full max-w-md rounded-2xl border border-stone-200 bg-white p-5 shadow-2xl"
        role="dialog"
        initial={{ opacity: 0, y: 14, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 10, scale: 0.98 }}
        transition={{ duration: 0.26, ease: [0.22, 1, 0.36, 1] }}
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
              className="grid h-7 w-7 shrink-0 place-items-center rounded-lg text-stone-400 motion-control hover:bg-stone-100 hover:text-stone-700"
              onClick={() => setUpdate(null)}
              title={t("updates.promptLater")}
              type="button"
            >
              <X aria-hidden="true" className="h-4 w-4" />
            </button>
          )}
        </div>

        {update.body && (
          <div className="mt-4 max-h-48 overflow-auto rounded-xl border border-stone-200 bg-stone-50 p-3">
            <ReleaseNotes notes={update.body} />
          </div>
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
              <div className="h-full rounded-full bg-amber-500 motion-control" style={{ width: `${progress}%` }} />
            </div>
          </div>
        )}

        <div className="mt-5 flex justify-end gap-2 border-t border-stone-100 pt-3">
          {!installed && (
            <button
              className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 motion-control hover:bg-stone-50 disabled:opacity-50"
              disabled={installing}
              onClick={() => setUpdate(null)}
              type="button"
            >
              {t("updates.promptLater")}
            </button>
          )}
          <button
            className="inline-flex items-center gap-1.5 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white motion-control hover:bg-stone-800 disabled:cursor-not-allowed disabled:bg-stone-400"
            disabled={installing}
            onClick={installed ? () => void relaunchApp() : () => void installUpdate()}
            type="button"
          >
            {installed ? <RotateCcw aria-hidden="true" className="h-4 w-4" /> : <RefreshCw aria-hidden="true" className={`h-4 w-4 ${installing ? "animate-spin" : ""}`} />}
            {installed ? t("updates.relaunch") : installing ? t("updates.promptInstalling") : t("updates.promptUpdate")}
          </button>
        </div>
      </motion.div>
        </motion.div>
      ) : null;
}
