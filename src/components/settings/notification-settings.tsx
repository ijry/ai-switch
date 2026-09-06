import { useState, useCallback, useMemo } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Bell, BellOff, Plus, Trash2, Send, Check, X } from "lucide-react";
import { useI18n } from "../../lib/i18n";
import { getSettings, saveSettings, testNotification, type NotificationChannelKind } from "../../lib/api/client";
import type { AppSettingsView } from "../../lib/api/types";

type NotificationChannelConfig =
  | { enabled: boolean; type: "feishu"; webhook_url: string }
  | { enabled: boolean; type: "bark"; server_url: string; device_key: string }
  | { enabled: boolean; type: "webhook"; url: string };

type NotificationConfig = {
  enabled: boolean;
  channels: NotificationChannelConfig[];
  events: {
    account_error: boolean;
    account_anomaly: boolean;
    health_check: boolean;
  };
};

function parseNotificationConfig(json: string | null | undefined): NotificationConfig {
  if (!json) return { enabled: false, channels: [], events: { account_error: true, account_anomaly: true, health_check: true } };
  try {
    const parsed = JSON.parse(json);
    const channels: NotificationChannelConfig[] = (parsed.channels ?? []).map((ch: Record<string, unknown>) => {
      const enabled = ch.enabled ?? false;
      const type = ch.type as string;
      if (type === "feishu") return { enabled, type: "feishu" as const, webhook_url: String(ch.webhook_url ?? "") };
      if (type === "bark") return { enabled, type: "bark" as const, server_url: String(ch.server_url ?? ""), device_key: String(ch.device_key ?? "") };
      return { enabled, type: "webhook" as const, url: String(ch.url ?? "") };
    });
    return {
      enabled: parsed.enabled ?? false,
      channels,
      events: {
        account_error: parsed.events?.account_error ?? true,
        account_anomaly: parsed.events?.account_anomaly ?? true,
        health_check: parsed.events?.health_check ?? true,
      },
    };
  } catch {
    return { enabled: false, channels: [], events: { account_error: true, account_anomaly: true, health_check: true } };
  }
}

function toNotificationConfigJson(config: NotificationConfig): string {
  return JSON.stringify(config);
}

type NotificationSettingsProps = {
  settings: AppSettingsView;
};

export function NotificationSettings({ settings }: NotificationSettingsProps) {
  const { t } = useI18n();
  const queryClient = useQueryClient();

  const config = useMemo(
    () => parseNotificationConfig(settings.notification_config_json),
    [settings.notification_config_json],
  );

  const [testResult, setTestResult] = useState<{ index: number; ok: boolean } | null>(null);

  const saveMutation = useMutation({
    mutationFn: (nextConfig: NotificationConfig) =>
      saveSettings({ ...settings, notification_config_json: toNotificationConfigJson(nextConfig) }),
    onSuccess: (saved) => {
      queryClient.setQueryData(["settings"], saved);
    },
  });

  const updateConfig = useCallback(
    (updater: (config: NotificationConfig) => NotificationConfig) => {
      const next = updater(config);
      saveMutation.mutate(next);
    },
    [config, saveMutation],
  );

  const handleToggleEnabled = () => {
    updateConfig((c) => ({ ...c, enabled: !c.enabled }));
  };

  const handleAddChannel = (type: NotificationChannelConfig["type"]) => {
    let newChannel: NotificationChannelConfig;
    if (type === "feishu") {
      newChannel = { enabled: true, type: "feishu", webhook_url: "" };
    } else if (type === "bark") {
      newChannel = { enabled: true, type: "bark", server_url: "https://api.day.app", device_key: "" };
    } else {
      newChannel = { enabled: true, type: "webhook", url: "" };
    }
    updateConfig((c) => ({ ...c, channels: [...c.channels, newChannel] }));
  };

  const handleRemoveChannel = (index: number) => {
    updateConfig((c) => ({
      ...c,
      channels: c.channels.filter((_, i) => i !== index),
    }));
  };

  const handleToggleChannel = (index: number) => {
    updateConfig((c) => ({
      ...c,
      channels: c.channels.map((ch, i) => (i === index ? { ...ch, enabled: !ch.enabled } : ch)),
    }));
  };

  const handleUpdateChannel = (index: number, patch: Record<string, unknown>) => {
    updateConfig((c) => ({
      ...c,
      channels: c.channels.map((ch, i) => (i === index ? ({ ...ch, ...patch } as NotificationChannelConfig) : ch)),
    }));
  };

  const handleToggleEvent = (key: keyof NotificationConfig["events"]) => {
    updateConfig((c) => ({
      ...c,
      events: { ...c.events, [key]: !c.events[key] },
    }));
  };

  const handleTest = async (index: number, kind: NotificationChannelKind) => {
    setTestResult(null);
    try {
      await testNotification(kind);
      setTestResult({ index, ok: true });
    } catch {
      setTestResult({ index, ok: false });
    }
  };

  return (
    <div className="space-y-3">
      {/* Master switch */}
      <div className="rounded-2xl border border-stone-200 bg-white/82 shadow-sm">
        <div className="flex items-center gap-3 px-4 py-3">
          <span className="grid h-8 w-8 place-items-center rounded-lg bg-white text-stone-700 shadow-sm ring-1 ring-stone-200">
            {config.enabled ? <Bell className="h-3.5 w-3.5" /> : <BellOff className="h-3.5 w-3.5" />}
          </span>
          <div className="flex-1 min-w-0">
            <p className="text-[15px] font-semibold text-stone-950">{t("notification.title")}</p>
            <p className="text-[12px] text-stone-500">{t("notification.subtitle")}</p>
          </div>
          <label className="flex items-center gap-2">
            <input
              checked={config.enabled}
              onChange={handleToggleEnabled}
              type="checkbox"
              className="mt-0.5"
            />
          </label>
        </div>
      </div>

      {config.enabled && (
        <>
          {/* Channels */}
          <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-[13px] font-semibold text-stone-950">Channels</h3>
              <div className="flex gap-1">
                {(["feishu", "bark", "webhook"] as const).map((type) => (
                  <button
                    key={type}
                    type="button"
                    onClick={() => handleAddChannel(type)}
                    className="inline-flex items-center gap-1 rounded-lg border border-stone-200 bg-white px-2 py-1 text-[11px] font-semibold text-stone-700 motion-control hover:bg-stone-50"
                  >
                    <Plus className="h-3 w-3" />
                    {type === "feishu" ? t("notification.channel.feishu") : type === "bark" ? t("notification.channel.bark") : t("notification.channel.webhook")}
                  </button>
                ))}
              </div>
            </div>

            {config.channels.length === 0 && (
              <p className="text-[12px] text-stone-500 py-2">
                No notification channels configured. Click + to add one.
              </p>
            )}

            <div className="space-y-3">
              {config.channels.map((channel, index) => (
                <div key={index} className="rounded-xl border border-stone-200 bg-stone-50/70 p-3">
                  <div className="flex items-center gap-2 mb-2">
                    <input
                      checked={channel.enabled}
                      onChange={() => handleToggleChannel(index)}
                      type="checkbox"
                      className="mt-0.5"
                    />
                    <span className="text-[12px] font-semibold text-stone-700">
                      {channel.type === "feishu" ? t("notification.channel.feishu") : channel.type === "bark" ? t("notification.channel.bark") : t("notification.channel.webhook")}
                    </span>
                    <div className="flex-1" />
                    {testResult?.index === index && (
                      <span className={`text-[11px] font-medium ${testResult.ok ? "text-emerald-700" : "text-red-700"}`}>
                        {testResult.ok ? t("notification.testSuccess") : t("notification.testFailed")}
                      </span>
                    )}
                    <button
                      type="button"
                      onClick={() => handleTest(index, channel.type === "feishu" ? { type: "feishu", webhook_url: channel.webhook_url } : channel.type === "bark" ? { type: "bark", server_url: channel.server_url, device_key: channel.device_key } : { type: "webhook", url: channel.url })}
                      disabled={!channel.enabled}
                      className="inline-flex items-center gap-1 rounded-lg border border-stone-200 bg-white px-2 py-1 text-[11px] font-semibold text-stone-700 motion-control hover:bg-stone-50 disabled:opacity-50"
                    >
                      <Send className="h-3 w-3" />
                      {t("notification.testChannel")}
                    </button>
                    <button
                      type="button"
                      onClick={() => handleRemoveChannel(index)}
                      className="inline-flex items-center gap-1 rounded-lg border border-stone-200 bg-white px-2 py-1 text-[11px] font-semibold text-red-600 motion-control hover:bg-red-50"
                    >
                      <Trash2 className="h-3 w-3" />
                      {t("notification.removeChannel")}
                    </button>
                  </div>

                  {channel.type === "feishu" && (
                    <label className="flex flex-col gap-1 text-[12px] font-semibold text-stone-600">
                      <span>{t("notification.feishu.webhookUrl")}</span>
                      <input
                        value={channel.webhook_url}
                        onChange={(e) => handleUpdateChannel(index, { webhook_url: e.target.value })}
                        placeholder={t("notification.feishu.webhookUrlPlaceholder")}
                        className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 shadow-sm outline-none focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
                      />
                    </label>
                  )}

                  {channel.type === "bark" && (
                    <div className="grid grid-cols-2 gap-2">
                      <label className="flex flex-col gap-1 text-[12px] font-semibold text-stone-600">
                        <span>{t("notification.bark.serverUrl")}</span>
                        <input
                          value={channel.server_url}
                          onChange={(e) => handleUpdateChannel(index, { server_url: e.target.value })}
                          placeholder={t("notification.bark.serverUrlPlaceholder")}
                          className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 shadow-sm outline-none focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
                        />
                      </label>
                      <label className="flex flex-col gap-1 text-[12px] font-semibold text-stone-600">
                        <span>{t("notification.bark.deviceKey")}</span>
                        <input
                          value={channel.device_key}
                          onChange={(e) => handleUpdateChannel(index, { device_key: e.target.value })}
                          placeholder={t("notification.bark.deviceKeyPlaceholder")}
                          className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 shadow-sm outline-none focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
                        />
                      </label>
                    </div>
                  )}

                  {channel.type === "webhook" && (
                    <label className="flex flex-col gap-1 text-[12px] font-semibold text-stone-600">
                      <span>{t("notification.webhook.url")}</span>
                      <input
                        value={channel.url}
                        onChange={(e) => handleUpdateChannel(index, { url: e.target.value })}
                        placeholder={t("notification.webhook.urlPlaceholder")}
                        className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 shadow-sm outline-none focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
                      />
                    </label>
                  )}
                </div>
              ))}
            </div>
          </div>

          {/* Event filters */}
          <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
            <h3 className="text-[13px] font-semibold text-stone-950 mb-3">{t("notification.events.title")}</h3>
            <div className="space-y-2">
              {([
                { key: "account_error" as const, label: t("notification.events.accountError"), hint: t("notification.events.accountErrorHint") },
                { key: "account_anomaly" as const, label: t("notification.events.accountAnomaly"), hint: t("notification.events.accountAnomalyHint") },
                { key: "health_check" as const, label: t("notification.events.healthCheck"), hint: t("notification.events.healthCheckHint") },
              ]).map(({ key, label, hint }) => (
                <label key={key} className="flex items-start gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2.5 text-[12px] font-semibold text-stone-700">
                  <input
                    checked={config.events[key]}
                    onChange={() => handleToggleEvent(key)}
                    type="checkbox"
                    className="mt-0.5"
                  />
                  <span className="grid gap-0.5">
                    <span>{label}</span>
                    <span className="text-[11px] font-medium text-stone-500">{hint}</span>
                  </span>
                </label>
              ))}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
