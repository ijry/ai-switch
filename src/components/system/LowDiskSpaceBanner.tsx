import { HardDrive, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { formatByteSize } from "../../lib/byteSize";
import { useI18n } from "../../lib/i18n";
import { useDiskSpaceStatus } from "../../lib/query/diskSpace";

/**
 * Warns while a volume the app writes to is nearly full.
 *
 * A full disk surfaces as unrelated-looking failures — accounts that will not
 * save, snapshots that never land — so the warning has to be global rather than
 * attached to whichever screen happens to fail first.
 */
export function LowDiskSpaceBanner() {
  const { t } = useI18n();
  const { data } = useDiskSpaceStatus();
  const [dismissed, setDismissed] = useState(false);
  const wasLow = useRef(false);

  const low = data?.low ?? false;

  // Dismissal is not persisted: a disk that fills up again is news, and the app
  // would otherwise stay silent about it for the rest of its life. Re-arming on
  // the false -> true edge keeps a single continuous warning from reappearing.
  useEffect(() => {
    if (low && !wasLow.current) {
      setDismissed(false);
    }
    wasLow.current = low;
  }, [low]);

  if (!data || !low || dismissed) {
    return null;
  }

  const lowVolumes = data.volumes.filter((volume) => volume.low);

  return (
    <div
      aria-live="assertive"
      className="fixed left-1/2 top-3 z-[70] w-[min(calc(100vw-1.5rem),34rem)] -translate-x-1/2"
      role="alert"
    >
      <div className="rounded-2xl bg-red-50 px-4 py-3 shadow-lg ring-1 ring-red-200">
        <div className="flex items-start gap-3">
          <span className="grid h-8 w-8 shrink-0 place-items-center rounded-xl bg-red-100 text-red-700">
            <HardDrive aria-hidden="true" className="h-4 w-4" />
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-[13px] font-semibold text-red-900">{t("disk.lowTitle")}</p>
            <p className="mt-0.5 text-[12px] text-red-800">
              {t("disk.lowBody", { threshold: formatByteSize(data.threshold_bytes) })}
            </p>
            <ul className="mt-1.5 list-none space-y-0.5 text-[12px] font-semibold text-red-900">
              {lowVolumes.map((volume) => (
                <li key={volume.path} title={volume.path}>
                  {t("disk.lowVolume", {
                    label: volume.label,
                    available: formatByteSize(volume.available_bytes),
                    total: formatByteSize(volume.total_bytes),
                  })}
                </li>
              ))}
            </ul>
          </div>
          <button
            aria-label={t("disk.lowDismiss")}
            className="grid h-7 w-7 shrink-0 place-items-center rounded-lg text-red-400 transition-colors hover:bg-red-100 hover:text-red-700"
            onClick={() => setDismissed(true)}
            title={t("disk.lowDismiss")}
            type="button"
          >
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </div>
      </div>
    </div>
  );
}
