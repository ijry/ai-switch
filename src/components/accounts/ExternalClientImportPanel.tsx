import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, FileSearch, LoaderCircle, RefreshCw } from "lucide-react";
import { previewExternalClientImport } from "../../lib/api/client";
import type {
  ExternalClientAccountPreviewItem,
  ExternalClientImportPreview,
  ExternalImportClient,
} from "../../lib/api/types";

export const EXTERNAL_IMPORT_CLIENT_LABELS: Record<ExternalImportClient, string> = {
  "cc-switch": "CC Switch",
};

type PreviewArgs = {
  client: ExternalImportClient;
  platform: string;
  /** `null` asks the backend to look in the client's default config location. */
  sourcePath: string | null;
  enabled: boolean;
};

/**
 * Reads the external client's providers.
 *
 * `staleTime: 0` on purpose: the user may switch to the other app, edit a
 * provider, and come straight back, so a cached preview would be wrong exactly
 * when it matters. Retries are off because the common failure is "no config
 * here", which retrying cannot fix.
 */
export function useExternalClientImportPreview({
  client,
  platform,
  sourcePath,
  enabled,
}: PreviewArgs) {
  return useQuery({
    queryKey: ["external-client-import", client, platform, sourcePath],
    queryFn: () =>
      previewExternalClientImport({ client, platform, source_path: sourcePath }),
    enabled,
    retry: false,
    staleTime: 0,
    gcTime: 0,
  });
}

const issueLabels: Record<string, string> = {
  "external_import.platform_unsupported": "该客户端类型暂不支持",
  "external_import.official_login_unsupported": "官方登录账号无法导入",
  "external_import.api_key_missing": "缺少 API Key",
  "external_import.base_url_missing": "缺少 Base URL",
  "external_import.base_url_invalid": "Base URL 无效",
  "external_import.conflicting_local_account": "已绑定到其他平台或非 API 账号",
};

function issueLabel(code: string) {
  return issueLabels[code] ?? code;
}

function dispositionLabel(disposition: string) {
  switch (disposition) {
    case "create":
      return "新增";
    case "overwrite":
      return "覆盖已有";
    default:
      return "需修正";
  }
}

function dispositionClass(disposition: string) {
  switch (disposition) {
    case "create":
      return "border-emerald-200 bg-emerald-50 text-emerald-800";
    case "overwrite":
      return "border-amber-200 bg-amber-50 text-amber-900";
    default:
      return "border-red-200 bg-red-50 text-red-800";
  }
}

export function isImportableExternalItem(item: ExternalClientAccountPreviewItem) {
  return item.disposition === "create" || item.disposition === "overwrite";
}

export type ExternalClientImportPanelProps = {
  client: ExternalImportClient;
  /** False in a browser: the native file picker only exists on the desktop. */
  desktop: boolean;
  error: string | null;
  labelClass: string;
  loading: boolean;
  onChooseSourcePath: () => void;
  onRefresh: () => void;
  onResetSourcePath: () => void;
  onToggleAll: (checked: boolean) => void;
  onToggleItem: (sourceId: string) => void;
  preview: ExternalClientImportPreview | null;
  selectedIds: Set<string>;
  sourcePath: string | null;
};

export function ExternalClientImportPanel({
  client,
  desktop,
  error,
  labelClass,
  loading,
  onChooseSourcePath,
  onRefresh,
  onResetSourcePath,
  onToggleAll,
  onToggleItem,
  preview,
  selectedIds,
  sourcePath,
}: ExternalClientImportPanelProps) {
  const clientLabel = EXTERNAL_IMPORT_CLIENT_LABELS[client];
  const importable = preview?.items.filter(isImportableExternalItem) ?? [];
  const allSelected = importable.length > 0 && importable.every((item) => selectedIds.has(item.source_id));
  const otherPlatforms = Object.entries(preview?.counts.other_platform_counts ?? {});

  return (
    <div className="mt-4 grid gap-3">
      <p className="text-[13px] leading-5 text-stone-600">
        从本机的 {clientLabel} 配置中读取 API 账号，勾选后导入。重复导入同一条记录会覆盖上次导入的账号，不会新增重复项。
      </p>

      <div className="grid gap-2 rounded-xl border border-stone-200 bg-white p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className={labelClass}>配置文件</span>
          <div className="flex items-center gap-1.5">
            <button
              aria-label="选择客户端配置文件"
              className="inline-flex items-center gap-1.5 rounded-xl border border-blue-200 bg-blue-50 px-3 py-1.5 text-[12px] font-semibold text-blue-900 transition-colors hover:bg-blue-100 disabled:cursor-not-allowed disabled:opacity-50"
              disabled={!desktop}
              onClick={onChooseSourcePath}
              title={desktop ? undefined : "此功能仅桌面端可用。"}
              type="button"
            >
              <FileSearch className="h-3.5 w-3.5" />
              选择文件
            </button>
            {sourcePath ? (
              <button
                aria-label="改回默认配置位置"
                className="rounded-xl border border-stone-200 bg-white px-3 py-1.5 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50"
                onClick={onResetSourcePath}
                type="button"
              >
                用默认位置
              </button>
            ) : null}
            <button
              aria-label="重新读取客户端账号"
              className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-1.5 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50 disabled:opacity-50"
              disabled={loading}
              onClick={onRefresh}
              type="button"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
              重新读取
            </button>
          </div>
        </div>
        <p className="break-all font-mono text-[11px] text-stone-500">
          {preview?.source_path ?? sourcePath ?? `自动查找 ${clientLabel} 默认配置位置`}
        </p>
      </div>

      {error ? (
        <p
          aria-live="assertive"
          className="rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-[12px] font-semibold text-red-800"
          role="alert"
        >
          {error}
        </p>
      ) : null}

      {loading && !preview ? (
        <p
          aria-live="polite"
          className="flex items-center gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[12px] text-stone-600"
        >
          <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
          正在读取 {clientLabel} 账号...
        </p>
      ) : null}

      {preview ? (
        <div className="grid gap-2">
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[12px] text-stone-700">
            <span className="font-semibold">
              可导入 {preview.counts.importable} 项 · 新增 {preview.counts.create} · 覆盖 {preview.counts.overwrite}
              {preview.counts.errors > 0 ? ` · 需修正 ${preview.counts.errors}` : ""}
            </span>
            <span className="font-mono text-[11px] text-stone-500">已勾选 {selectedIds.size}</span>
          </div>

          {otherPlatforms.length > 0 ? (
            <p className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[11px] text-stone-500">
              另有 {preview.counts.other_platform} 个账号属于其他平台（
              {otherPlatforms.map(([platform, count]) => `${platform} ${count}`).join(" · ")}
              ），请切换到对应平台再导入。
            </p>
          ) : null}

          {preview.items.length === 0 ? (
            <p className="rounded-xl border border-dashed border-stone-200 bg-white px-3 py-3 text-[12px] text-stone-500">
              该配置里没有当前平台的账号。
            </p>
          ) : (
            <div className="overflow-hidden rounded-xl border border-stone-200 bg-white">
              <label className="flex items-center gap-2 border-b border-stone-100 bg-stone-50 px-3 py-2 text-[12px] font-semibold text-stone-700">
                <input
                  aria-label="全选可导入账号"
                  checked={allSelected}
                  className="h-4 w-4 rounded border-stone-300 text-amber-500 focus:ring-blue-400"
                  disabled={importable.length === 0}
                  onChange={(event) => onToggleAll(event.target.checked)}
                  type="checkbox"
                />
                全选可导入账号
              </label>
              <ul className="max-h-72 overflow-y-auto">
                {preview.items.map((item) => {
                  const selectable = isImportableExternalItem(item);
                  return (
                    <li className="border-b border-stone-100 px-3 py-2 last:border-b-0" key={item.source_id}>
                      <label className="flex min-w-0 items-start gap-2">
                        <input
                          aria-label={`导入 ${item.display_name}`}
                          checked={selectedIds.has(item.source_id)}
                          className="mt-1 h-4 w-4 rounded border-stone-300 text-amber-500 focus:ring-blue-400 disabled:opacity-40"
                          disabled={!selectable}
                          onChange={() => onToggleItem(item.source_id)}
                          type="checkbox"
                        />
                        <span className="min-w-0 flex-1 grid gap-0.5">
                          <span className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                            <span className="min-w-0 truncate text-[13px] font-semibold text-stone-900">
                              {item.display_name}
                            </span>
                            <span
                              className={`rounded-full border px-1.5 py-0.5 text-[10px] font-semibold ${dispositionClass(item.disposition)}`}
                            >
                              {dispositionLabel(item.disposition)}
                            </span>
                          </span>
                          {item.base_url ? (
                            <span className="truncate font-mono text-[11px] text-stone-500">
                              {item.base_url}
                              {item.api_key_masked ? ` · ${item.api_key_masked}` : ""}
                            </span>
                          ) : null}
                          <span className="text-[11px] text-stone-500">
                            {item.interface_format ? `${item.interface_format} · ` : ""}
                            映射 {item.model_mapping_count} 条
                            {/* Only an actual overwrite promises a replacement; a
                                refused entry names the row it collides with in
                                the issue line instead. */}
                            {item.disposition === "overwrite" && item.existing_display_name
                              ? ` · 将覆盖「${item.existing_display_name}」`
                              : ""}
                          </span>
                          {item.issue_codes.length > 0 ? (
                            <span className="flex items-center gap-1 text-[11px] font-semibold text-red-700">
                              <AlertTriangle className="h-3 w-3" />
                              {item.issue_codes.map(issueLabel).join("、")}
                              {item.existing_display_name ? `（${item.existing_display_name}）` : ""}
                            </span>
                          ) : null}
                        </span>
                      </label>
                    </li>
                  );
                })}
              </ul>
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}
