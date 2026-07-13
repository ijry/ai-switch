import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { CheckCircle2, Download, RefreshCw, RotateCcw, ShieldAlert } from "lucide-react";
import { useState } from "react";

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

function releaseDate(update: Update) {
  const date = update.date ? new Date(update.date) : null;
  return date && !Number.isNaN(date.getTime())
    ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(date)
    : "Unknown date";
}

export function UpdatesScreen() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [download, setDownload] = useState<DownloadState>({ downloaded: 0 });
  const [status, setStatus] = useState("No update check has run in this session.");
  const [error, setError] = useState<string | null>(null);
  const [installed, setInstalled] = useState(false);

  const progress = download.total ? Math.min(100, Math.round((download.downloaded / download.total) * 100)) : 0;

  const handleCheck = async () => {
    setChecking(true);
    setError(null);
    setInstalled(false);
    setStatus("Checking release metadata...");
    try {
      const nextUpdate = await check();
      setUpdate(nextUpdate);
      setStatus(nextUpdate ? `Update ${nextUpdate.version} is available.` : "You are running the latest available version.");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setStatus("Update check failed.");
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
    setStatus("Downloading update...");
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
      setStatus("Update installed. Relaunch the app to finish.");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setStatus("Update install failed.");
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
          <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Updates</p>
          <h1 className="mt-0.5 text-lg font-semibold tracking-tight text-stone-950">App update channel</h1>
          <p className="mt-1 text-[13px] text-stone-600">
            Checks GitHub release metadata configured in Tauri and installs signed desktop updates.
          </p>
        </div>

        <div className="grid gap-3 p-3 lg:grid-cols-[minmax(0,1fr)_320px]">
          <div className="space-y-3 rounded-2xl border border-stone-200 bg-stone-50 p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <p className="text-[13px] font-semibold text-stone-950">Status</p>
                <p className="mt-1 text-[13px] text-stone-600">{status}</p>
              </div>
              <button
                className="inline-flex items-center gap-2 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800 disabled:cursor-not-allowed disabled:bg-stone-400"
                disabled={checking || installing}
                onClick={handleCheck}
                type="button"
              >
                <RefreshCw className={`h-4 w-4 ${checking ? "animate-spin" : ""}`} />
                {checking ? "Checking..." : "Check for updates"}
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
                    <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Available release</p>
                    <h2 className="mt-0.5 text-xl font-semibold tracking-tight text-stone-950">{update.version}</h2>
                    <p className="mt-1 text-[13px] text-stone-500">{releaseDate(update)}</p>
                  </div>
                  <button
                    className="inline-flex items-center gap-2 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800 disabled:cursor-not-allowed disabled:bg-stone-400"
                    disabled={installing || installed}
                    onClick={handleInstall}
                    type="button"
                  >
                    <Download className="h-4 w-4" />
                    {installing ? "Installing..." : installed ? "Installed" : "Download and install"}
                  </button>
                </div>

                {installing && (
                  <div className="mt-4 rounded-xl border border-stone-200 bg-stone-50 p-3">
                    <div className="flex items-center justify-between text-[12px] font-semibold text-stone-600">
                      <span>Download progress</span>
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
                  <p className="mt-2 text-sm font-semibold text-stone-950">No update selected</p>
                  <p className="mt-1 text-[13px] text-stone-500">Run a check to compare this build with the release endpoint.</p>
                </div>
              </div>
            )}
          </div>

          <aside className="space-y-3 rounded-2xl border border-stone-200 bg-stone-50 p-4">
            <div>
              <p className="text-[13px] font-semibold text-stone-950">Release source</p>
              <p className="mt-1 break-all text-[12px] text-stone-500">
                https://github.com/ijry/ai-switch/releases/latest/download/latest.json
              </p>
            </div>
            <div className="rounded-xl border border-amber-200 bg-amber-50 px-3 py-2 text-[12px] leading-5 text-amber-900">
              Updates require a signed bundle and a valid updater manifest. Development builds may report an endpoint or
              signature error until release assets exist.
            </div>
            {installed && (
              <button
                className="inline-flex w-full items-center justify-center gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-800 transition-colors hover:bg-stone-50"
                onClick={handleRelaunch}
                type="button"
              >
                <RotateCcw className="h-4 w-4" />
                Relaunch now
              </button>
            )}
          </aside>
        </div>
      </div>
    </section>
  );
}
