import { FormEvent, useEffect, useState } from "react";
import { getSettings } from "../../lib/api/client";
import { useI18n } from "../../lib/i18n";
import {
  clearWebAccessToken,
  getWebAccessToken,
  isUnauthorizedTransportError,
  setWebAccessToken,
} from "../../lib/transport";

type WebAuthGateProps = {
  onAuthenticated: () => void;
};

export function WebAuthGate({ onAuthenticated }: WebAuthGateProps) {
  const { t } = useI18n();
  const [token, setToken] = useState(() => getWebAccessToken());
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [checkingStored, setCheckingStored] = useState(() => Boolean(getWebAccessToken()));

  useEffect(() => {
    let cancelled = false;
    const stored = getWebAccessToken().trim();
    if (!stored) {
      setCheckingStored(false);
      return;
    }

    (async () => {
      try {
        await getSettings();
        if (!cancelled) {
          onAuthenticated();
        }
      } catch (cause) {
        if (cancelled) {
          return;
        }
        clearWebAccessToken();
        setToken("");
        if (isUnauthorizedTransportError(cause)) {
          setError(t("auth.invalidToken"));
        } else {
          setError(cause instanceof Error ? cause.message : t("auth.failed"));
        }
        setCheckingStored(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [onAuthenticated, t]);

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const nextToken = token.trim();
    if (!nextToken) {
      setError(t("auth.tokenRequired"));
      return;
    }

    setPending(true);
    setError(null);
    setWebAccessToken(nextToken);

    try {
      await getSettings();
      onAuthenticated();
    } catch (cause) {
      clearWebAccessToken();
      if (isUnauthorizedTransportError(cause)) {
        setError(t("auth.invalidToken"));
      } else {
        setError(cause instanceof Error ? cause.message : t("auth.failed"));
      }
    } finally {
      setPending(false);
    }
  };

  if (checkingStored) {
    return (
      <div className="grid min-h-screen place-items-center bg-stone-100 px-4 text-[13px] text-stone-500">
        {t("auth.connecting")}
      </div>
    );
  }

  return (
    <div className="grid min-h-screen place-items-center bg-stone-100 px-4">
      <form
        className="w-full max-w-sm space-y-3 rounded-2xl border border-stone-200 bg-white p-5 shadow-sm"
        onSubmit={handleSubmit}
      >
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
            {t("auth.kicker")}
          </p>
          <h1 className="mt-1 text-lg font-semibold text-stone-950">{t("auth.title")}</h1>
          <p className="mt-1 text-[13px] text-stone-500">{t("auth.subtitle")}</p>
        </div>

        <label className="flex flex-col gap-1.5 text-[12px] font-semibold text-stone-600">
          <span>{t("auth.token")}</span>
          <input
            autoComplete="current-password"
            className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-medium text-stone-900 shadow-sm outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
            onChange={(event) => setToken(event.target.value)}
            placeholder={t("auth.tokenPlaceholder")}
            type="password"
            value={token}
          />
        </label>

        {error && <p className="text-[12px] font-medium text-red-700">{error}</p>}

        <button
          className="w-full rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors duration-150 hover:bg-stone-800 disabled:opacity-60"
          disabled={pending}
          type="submit"
        >
          {pending ? t("auth.connecting") : t("auth.connect")}
        </button>
      </form>
    </div>
  );
}
