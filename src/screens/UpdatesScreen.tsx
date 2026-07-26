import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { CheckCircle2, Download, RefreshCw, RotateCcw, ShieldAlert } from "lucide-react";
import { useState } from "react";
import { useI18n, type Language } from "../lib/i18n";

type DownloadState = {
  downloaded: number;
  total?: number;
};

type UpdateStatus =
  | "updates.statusInitial"
  | "updates.statusChecking"
  | "updates.statusAvailable"
  | "updates.statusLatest"
  | "updates.statusDownloading"
  | "updates.statusInstalled"
  | "updates.statusCheckFailed"
  | "updates.statusInstallFailed";

function formatBytes(value: number) {
  if (value < 1024) {
    return `${value} B`;
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KB`;
  }
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function releaseDate(update: Update, language: Language, unknownDate: string) {
  const date = update.date ? new Date(update.date) : null;
  return date && !Number.isNaN(date.getTime())
    ? new Intl.DateTimeFormat(language, { dateStyle: "medium" }).format(date)
    : unknownDate;
}

export function UpdatesScreen() {
  const { language, t } = useI18n();
  const [update, setUpdate] = useState<Update | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [download, setDownload] = useState<DownloadState>({ downloaded: 0 });
  const [status, setStatus] = useState<UpdateStatus>("updates.statusInitial");
  const [error, setError] = useState<string | null>(null);
  const [installed, setInstalled] = useState(false);

  const progress = download.total ? Math.min(100, Math.round((download.downloaded / download.total) * 100)) : 0;
  const statusText = status === "updates.statusAvailable" && update
    ? t(status, { version: update.version })
    : t(status);

  const handleCheck = async () => {
    setChecking(true);
    setError(null);
    setInstalled(false);
    setStatus("updates.statusChecking");
    try {
      const nextUpdate = await check();
      setUpdate(nextUpdate);
      setStatus(nextUpdate ? "updates.statusAvailable" : "updates.statusLatest");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setStatus("updates.statusCheckFailed");
    } finally {
      setChecking(false);
    }
  };

  const handleInstall = async () => {
    if (!update) {
      return;
    }

    setInstalling(true);
    setError(null);
    setInstalled(false);
    setDownload({ downloaded: 0 });
    setStatus("updates.statusDownloading");
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
          setDownload((current) => ({ ...current, downloaded: current.total ?? current.downloaded }));
        }
      });
      setInstalled(true);
      setStatus("updates.statusInstalled");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setStatus("updates.statusInstallFailed");
    } finally {
      setInstalling(false);
    }
  };

  const handleRelaunch = async () => {
    setError(null);
    try {
      await relaunch();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  return (
    <section className="space-y-3">
      <div className="rounded-2xl border border-stone-200 bg-white/82 shadow-sm">
        <div className="border-b border-stone-200 px-4 py-3">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">{t("updates.kicker")}</p>
          <h1 className="mt-0.5 text-lg font-semibold tracking-tight text-stone-950">{t("updates.title")}</h1>
          <p className="mt-1 text-[13px] text-stone-600">
            {t("updates.subtitle")}
          </p>
        </div>

        <div className="grid gap-3 p-3 lg:grid-cols-[minmax(0,1fr)_320px]">
          <div className="space-y-3 rounded-2xl border border-stone-200 bg-stone-50 p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <p className="text-[13px] font-semibold text-stone-950">{t("updates.status")}</p>
                <p className="mt-1 text-[13px] text-stone-600">{statusText}</p>
              </div>
              <button
                className="inline-flex items-center gap-2 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800 disabled:cursor-not-allowed disabled:bg-stone-400"
                disabled={checking || installing}
                onClick={handleCheck}
                type="button"
              >
                <RefreshCw className={`h-4 w-4 ${checking ? "animate-spin" : ""}`} />
                {checking ? t("updates.checking") : t("updates.check")}
              </button>
            </div>

            {error && (
              <div className="flex items-start gap-2 rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-[13px] text-red-700">
                <ShieldAlert className="mt-0.5 h-4 w-4 shrink-0" />
                <span>{error}</span>
              </div>
            )}

            {update ? (
              <div className="rounded-2xl border border-stone-200 bg-white p-4">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
                      {t("updates.availableRelease")}
                    </p>
                    <h2 className="mt-0.5 text-xl font-semibold tracking-tight text-stone-950">{update.version}</h2>
                    <p className="mt-1 text-[13px] text-stone-500">
                      {releaseDate(update, language, t("updates.unknownDate"))}
                    </p>
                  </div>
                  <button
                    className="inline-flex items-center gap-2 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800 disabled:cursor-not-allowed disabled:bg-stone-400"
                    disabled={installing || installed}
                    onClick={handleInstall}
                    type="button"
                  >
                    <Download className="h-4 w-4" />
                    {installing ? t("updates.installing") : installed ? t("updates.installed") : t("updates.downloadInstall")}
                  </button>
                </div>

                {installing && (
                  <div className="mt-4 rounded-xl border border-stone-200 bg-stone-50 p-3">
                    <div className="flex items-center justify-between text-[12px] font-semibold text-stone-600">
                      <span>{t("updates.downloadProgress")}</span>
                      <span>
                        {formatBytes(download.downloaded)}
                        {download.total ? ` / ${formatBytes(download.total)}` : ""}
                      </span>
                    </div>
                    <div className="mt-2 h-2 overflow-hidden rounded-full bg-stone-200">
                      <div className="h-full rounded-full bg-stone-900 transition-all" style={{ width: `${progress}%` }} />
                    </div>
                  </div>
                )}

                {update.body && (
                  <pre className="mt-4 max-h-[280px] overflow-auto whitespace-pre-wrap rounded-xl border border-stone-200 bg-stone-50 p-3 text-[12px] leading-5 text-stone-700">
                    {update.body}
                  </pre>
                )}
              </div>
            ) : (
              <div className="grid min-h-[260px] place-items-center rounded-2xl border border-dashed border-stone-200 bg-white text-center">
                <div>
                  <CheckCircle2 className="mx-auto h-8 w-8 text-stone-300" />
                  <p className="mt-2 text-sm font-semibold text-stone-950">{t("updates.noSelection")}</p>
                  <p className="mt-1 text-[13px] text-stone-500">{t("updates.noSelectionBody")}</p>
                </div>
              </div>
            )}
          </div>

          <aside className="space-y-3 rounded-2xl border border-stone-200 bg-stone-50 p-4">
            <div>
              <p className="text-[13px] font-semibold text-stone-950">{t("updates.releaseSource")}</p>
              <p className="mt-1 break-all text-[12px] text-stone-500">
                https://github.com/ijry/ai-switch/releases/latest/download/latest.json
              </p>
            </div>
            <div className="rounded-xl border border-amber-200 bg-amber-50 px-3 py-2 text-[12px] leading-5 text-amber-900">
              {t("updates.requirements")}
            </div>
            {installed && (
              <button
                className="inline-flex w-full items-center justify-center gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-800 transition-colors hover:bg-stone-50"
                onClick={handleRelaunch}
                type="button"
              >
                <RotateCcw className="h-4 w-4" />
                {t("updates.relaunch")}
              </button>
            )}
          </aside>
        </div>
      </div>
    </section>
  );
}
