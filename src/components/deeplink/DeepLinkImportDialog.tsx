import { useEffect, useMemo, useState } from "react";
import { createApiRouteCredential } from "../../lib/api/client";
import type { CreateApiRouteCredentialInput, InterfaceFormat } from "../../lib/api/types";
import { getTransport, isDesktop } from "../../lib/transport";
import { Button } from "../ui/Button";

export type DeepLinkProviderImportPayload = {
  scheme: "ccswitch" | "aiswitch" | string;
  version: string;
  resource: string;
  app: string;
  platform: "claude" | "codex" | "gemini" | "grok" | string;
  display_name: string;
  base_url: string;
  api_key_masked: string;
  api_key: string;
  interface_format: string;
  model_mappings_json: string;
  homepage?: string | null;
  notes?: string | null;
  source_url_sanitized: string;
};

type DeepLinkErrorPayload = {
  message: string;
  source: string;
};

type DeepLinkImportDialogProps = {
  onImported: (platform: string) => void;
};

function mappingSummary(modelMappingsJson: string) {
  try {
    const list = JSON.parse(modelMappingsJson) as unknown[];
    if (!Array.isArray(list) || list.length === 0) {
      return "空";
    }
    return `${list.length} 条`;
  } catch {
    return "空";
  }
}

export function DeepLinkImportDialog({ onImported }: DeepLinkImportDialogProps) {
  const [payload, setPayload] = useState<DeepLinkProviderImportPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [bannerError, setBannerError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (!isDesktop()) {
      return;
    }

    const transport = getTransport();
    let active = true;
    let unsubs: Array<() => void> = [];

    void (async () => {
      try {
        const unlistenImport = await transport.subscribe<DeepLinkProviderImportPayload>(
          "deeplink-import",
          (next) => {
            if (!active) {
              return;
            }
            setPayload(next);
            setError(null);
            setBannerError(null);
          },
        );
        const unlistenError = await transport.subscribe<DeepLinkErrorPayload>(
          "deeplink-error",
          (next) => {
            if (!active) {
              return;
            }
            setPayload(null);
            setBannerError(next.message || "深链接解析失败");
          },
        );

        if (!active) {
          unlistenImport();
          unlistenError();
          return;
        }

        unsubs = [unlistenImport, unlistenError];
      } catch {
        // Incomplete Tauri mocks / non-desktop shells may lack event IPC.
        // Deep-link import is optional here; never leave an unhandled rejection.
      }
    })();

    return () => {
      active = false;
      for (const unsub of unsubs) {
        unsub();
      }
    };
  }, []);

  const summary = useMemo(
    () => (payload ? mappingSummary(payload.model_mappings_json) : "空"),
    [payload],
  );

  if (!payload && !bannerError) {
    return null;
  }

  async function handleConfirm() {
    if (!payload) {
      return;
    }

    setSubmitting(true);
    setError(null);
    try {
      const input: CreateApiRouteCredentialInput = {
        platform: payload.platform,
        display_name: payload.display_name,
        api_key: payload.api_key,
        base_url: payload.base_url,
        interface_format: payload.interface_format as InterfaceFormat,
        model_mappings_json: payload.model_mappings_json,
      };
      await createApiRouteCredential(input);
      const platform = payload.platform;
      setPayload(null);
      onImported(platform);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="fixed inset-0 z-[80] flex items-center justify-center bg-stone-950/40 p-4">
      <div
        aria-label="导入 API 账号"
        aria-modal="true"
        className="w-full max-w-lg rounded-2xl border border-stone-200 bg-white p-5 shadow-2xl"
        role="dialog"
      >
        {bannerError && !payload ? (
          <>
            <h2 className="text-base font-semibold text-stone-900">深链接导入失败</h2>
            <p className="mt-3 text-sm text-red-600">{bannerError}</p>
            <div className="mt-5 flex justify-end">
              <Button type="button" variant="secondary" onClick={() => setBannerError(null)}>
                关闭
              </Button>
            </div>
          </>
        ) : payload ? (
          <>
            <h2 className="text-base font-semibold text-stone-900">确认导入 API 账号</h2>
            <dl className="mt-4 space-y-2 text-sm text-stone-700">
              <div className="flex justify-between gap-3">
                <dt className="text-stone-500">类型</dt>
                <dd>API 账号</dd>
              </div>
              <div className="flex justify-between gap-3">
                <dt className="text-stone-500">平台</dt>
                <dd>{payload.platform}</dd>
              </div>
              <div className="flex justify-between gap-3">
                <dt className="text-stone-500">名称</dt>
                <dd className="text-right">{payload.display_name}</dd>
              </div>
              <div className="flex justify-between gap-3">
                <dt className="text-stone-500">Base URL</dt>
                <dd className="break-all text-right">{payload.base_url}</dd>
              </div>
              <div className="flex justify-between gap-3">
                <dt className="text-stone-500">API Key</dt>
                <dd>{payload.api_key_masked}</dd>
              </div>
              <div className="flex justify-between gap-3">
                <dt className="text-stone-500">模型映射</dt>
                <dd>{summary}</dd>
              </div>
              <div className="flex justify-between gap-3">
                <dt className="text-stone-500">来源</dt>
                <dd>{payload.scheme}</dd>
              </div>
            </dl>
            {error ? <p className="mt-3 text-sm text-red-600">{error}</p> : null}
            <div className="mt-5 flex justify-end gap-2">
              <Button
                type="button"
                variant="secondary"
                disabled={submitting}
                onClick={() => {
                  setPayload(null);
                  setError(null);
                }}
              >
                取消
              </Button>
              <Button type="button" disabled={submitting} onClick={() => void handleConfirm()}>
                {submitting ? "导入中..." : "确认导入"}
              </Button>
            </div>
          </>
        ) : null}
      </div>
    </div>
  );
}
