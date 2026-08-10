import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  ArrowRight,
  Archive,
  ArchiveRestore,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Copy,
  Download,
  Edit3,
  ExternalLink,
  FileCode2,
  GripVertical,
  KeyRound,
  List,
  MessageSquareText,
  Play,
  Plus,
  RefreshCw,
  ScanText,
  ScrollText,
  Send,
  Square,
  Trash2,
  Wand2,
  X,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type ReactNode,
} from "react";
import { PlatformSupportBadge } from "../components/platform/PlatformSupportBadge";
import { expandDisplayModelMappings, ModelMappingSummary } from "../components/accounts/ModelMappingSummary";
import { RouteCredentialExportDialog } from "../components/accounts/RouteCredentialExportDialog";
import { neighborsForDrop } from "../lib/accountReorder";
import {
  createBatch,
  copyRouteCredential,
  createApiRouteCredential,
  archiveRouteCredentials,
  deleteRouteCredential,
  fetchRouteModels,
  getRoutePool,
  getRouteProxyKey,
  getRouteProxyStatus,
  importOfficialRouteCredentialsFromFiles,
  importOfficialRouteCredentialsFromText,
  listRouteCredentials,
  listRouteCredentialPage,
  reorderRouteCredentials,
  refreshRouteCredentialQuota,
  refreshRouteCredentialsQuota,
  restoreRouteCredentials,
  routePoolTestModel,
  setRouteCredentialStatuses,
  setRoutePoolMembers,
  startRouteProxy,
  stopRouteProxy,
  subscribeRouteProxyLiveLog,
  unsubscribeRouteProxyLiveLog,
  updateRouteCredential,
  writeRouteProxyConfigs,
} from "../lib/api/client";
import type {
  AccountStatus,
  AnthropicApiKeyField,
  ConfigWriteOutcome,
  FetchedRouteModel,
  InterfaceFormat,
  ModelMapping,
  PlatformId,
  QuotaRefreshOutcome,
  RouteCredential,
  RouteCredentialActivityEvent,
  RouteCredentialPage,
  RouteCredentialPoolScope,
  RouteCredentialSelectionContext,
  RouteModelsFetchRequest,
  RoutePoolModelTestOutcome,
  RoutePoolModelTestRequest,
  RoutePoolUsageLog,
  RouteProxyLiveLogEntry,
} from "../lib/api/types";
import {
  capabilityReason,
  credentialKindAllowed,
  findPlatformCapability,
  operationEnabled,
} from "../lib/platformCapabilities";
import { usePlatformCapabilities } from "../lib/query/platformCapabilities";
import {
  matchUserAgentPreset,
  readUserAgentFromConfig,
  USER_AGENT_PRESETS,
  writeUserAgentToConfig,
} from "../lib/accountUserAgent";
import { getTransport, isTauriRuntime } from "../lib/transport";
import { fetchRouteProxyModels } from "../lib/routeProxyModels";
import { copySensitiveText } from "../lib/routeCredentialTransfer";
import {
  codexModelTestInterfaceFormat,
  loadCodexModelTestEndpoint,
  saveCodexModelTestEndpoint,
  type CodexModelTestEndpoint,
} from "../lib/codexModelTestEndpoint";
import {
  ClipboardImageReadError,
  readClipboardImageBlob,
  recognizeApiKeysFromImageBlob,
} from "../lib/ocr/apiKeyOcr";

type PlatformKey = PlatformId;
type CreateMode = "api" | "official";
type AccountView = "in_pool" | "out_of_pool" | "archived" | "stats";
type RoutePoolAction = "add" | "remove" | "sync";
type RoutePoolFeedback = {
  type: "success" | "error";
  message: string;
} | null;
type RoutePoolMutationInput = {
  platform: string;
  account_ids: string[];
  action: RoutePoolAction;
  affectedCount: number;
};

function formatApiError(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (error && typeof error === "object") {
    const record = error as {
      message?: unknown;
      details?: unknown;
      code?: unknown;
    };
    const message =
      typeof record.message === "string" ? record.message.trim() : "";
    const details =
      typeof record.details === "string" ? record.details.trim() : "";
    if (message && details) {
      return `${message} (${details})`;
    }
    if (message) {
      return message;
    }
    if (details) {
      return details;
    }
    if (typeof record.code === "string" && record.code.trim()) {
      return record.code;
    }
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  return fallback;
}

function accountStatusLabel(status: string): string {
  switch (status) {
    case "ok":
      return "正常";
    case "warning":
      return "警告";
    case "error":
      return "异常";
    case "revoked":
      return "revoked";
    case "paused":
      return "暂停";
    default:
      return status || "未知";
  }
}

function accountStatusClass(status: string): string {
  switch (status) {
    case "ok":
      return "bg-emerald-50 text-emerald-800";
    case "warning":
      return "bg-amber-50 text-amber-800";
    case "error":
      return "bg-red-50 text-red-800";
    case "revoked":
      return "bg-rose-100 text-rose-900 ring-1 ring-rose-200";
    case "paused":
      return "bg-slate-100 text-slate-700 ring-1 ring-slate-200";
    default:
      return "bg-stone-100 text-stone-600";
  }
}

const routeStatsPeriods = [
  { key: "today", label: "当日" },
  { key: "week", label: "本周" },
  { key: "month", label: "本月" },
  { key: "all", label: "累计" },
] as const;

const accountViewOptions: Array<{ key: AccountView; label: string }> = [
  { key: "in_pool", label: "算力池" },
  { key: "out_of_pool", label: "未入池" },
  { key: "archived", label: "已归档" },
  { key: "stats", label: "统计" },
];

const routeStatsPageSize = 20;
const routeStatsRefreshMs = 5000;

type RouteStatsPeriod = (typeof routeStatsPeriods)[number]["key"];

function routeStatsSince(period: RouteStatsPeriod, now = new Date()) {
  if (period === "all") {
    return null;
  }

  const start = new Date(now);
  start.setHours(0, 0, 0, 0);

  if (period === "week") {
    const day = start.getDay();
    const daysSinceMonday = day === 0 ? 6 : day - 1;
    start.setDate(start.getDate() - daysSinceMonday);
  }

  if (period === "month") {
    start.setDate(1);
  }

  return start.toISOString();
}

function formatUsageTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

function liveLogStagesIdentical(entry: RouteProxyLiveLogEntry): boolean {
  return (entry.upstream_response ?? null) === (entry.final_response ?? null);
}

function LiveLogStage({ title, body }: { title: string; body: string | null | undefined }) {
  return (
    <div>
      <p className="text-[11px] font-medium text-stone-500">{title}</p>
      <pre className="mt-1 max-h-48 overflow-auto rounded-lg border border-stone-200 bg-white p-2 font-mono text-[11px] leading-relaxed text-stone-700">
        {body && body.trim() ? prettyJsonOrText(body) : "（空）"}
      </pre>
    </div>
  );
}

type ParsedUsageMetadata = {
  path: string;
  status: string;
  model: string;
  responseBody: string | null;
  formattedJson: string;
  raw: string;
  valid: boolean;
};

function metadataField(record: Record<string, unknown>, key: string) {
  const value = record[key];
  if (typeof value === "string" && value.trim()) {
    return value;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return "-";
}

function optionalMetadataField(record: Record<string, unknown>, key: string) {
  const value = record[key];
  if (typeof value === "string" && value.trim()) {
    return value.trim();
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return null;
}

function parseUsageMetadata(metadataJson: string): ParsedUsageMetadata {
  try {
    const value = JSON.parse(metadataJson) as unknown;
    const formattedJson = JSON.stringify(value, null, 2) ?? metadataJson;
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      return {
        path: "-",
        status: "-",
        model: "-",
        responseBody: null,
        formattedJson,
        raw: metadataJson,
        valid: true,
      };
    }
    const record = value as Record<string, unknown>;
    const requestedModel = optionalMetadataField(record, "requested_model");
    const upstreamModel = optionalMetadataField(record, "upstream_model");
    const model =
      requestedModel && upstreamModel && requestedModel !== upstreamModel
        ? `${requestedModel}->${upstreamModel}`
        : requestedModel ?? upstreamModel ?? "-";
    return {
      path: metadataField(record, "path"),
      status: metadataField(record, "status"),
      model,
      responseBody: optionalMetadataField(record, "response_body"),
      formattedJson,
      raw: metadataJson,
      valid: true,
    };
  } catch {
    return {
      path: "-",
      status: "-",
      model: "-",
      responseBody: null,
      formattedJson: metadataJson,
      raw: metadataJson,
      valid: false,
    };
  }
}

function formatUsageCount(value: number | null | undefined) {
  return value == null ? "-" : value.toLocaleString();
}

function formatUsageTotalTokens(request: RoutePoolUsageLog) {
  if (request.input_tokens == null && request.output_tokens == null) {
    return "-";
  }
  return ((request.input_tokens ?? 0) + (request.output_tokens ?? 0)).toLocaleString();
}

function usageTokenTooltip(request: RoutePoolUsageLog) {
  return `输入 Token：${formatUsageCount(request.input_tokens)}；输出 Token：${formatUsageCount(request.output_tokens)}；缓存 Token：${formatUsageCount(request.cache_tokens)}`;
}

function formatUsagePrice(request: RoutePoolUsageLog) {
  if (request.price_currency === "cny" && request.price_cny_micros != null) {
    return `¥${(request.price_cny_micros / 1_000_000).toFixed(6)}`;
  }
  if (request.price_currency === "usd" && request.price_usd_micros != null) {
    return `$${(request.price_usd_micros / 1_000_000).toFixed(6)}`;
  }
  return "-";
}

function RouteRequestDetail({
  metadata,
  request,
}: {
  metadata: ParsedUsageMetadata;
  request: RoutePoolUsageLog;
}) {
  return (
    <div
      aria-label={`请求 ${request.id} 详情`}
      className="border-t border-stone-100 bg-stone-50 px-3 py-3"
      id={`route-request-detail-${request.id}`}
    >
      <div className="flex items-center justify-between gap-2">
        <p className="text-[12px] font-semibold text-stone-800">请求详情</p>
        <p className="font-mono text-[11px] text-stone-500">{request.id}</p>
      </div>
      <div className="mt-3 grid gap-2 text-[12px] sm:grid-cols-2 lg:grid-cols-3">
        <div>
          <p className="text-[11px] font-medium text-stone-500">账号</p>
          <p className="mt-0.5 text-stone-800">{request.account_name ?? "-"}</p>
        </div>
        <div>
          <p className="text-[11px] font-medium text-stone-500">账号 ID</p>
          <p className="mt-0.5 break-all font-mono text-[11px] text-stone-700">{request.account_id ?? "-"}</p>
        </div>
        <div>
          <p className="text-[11px] font-medium text-stone-500">来源</p>
          <p className="mt-0.5 text-stone-800">{request.source_label}</p>
        </div>
        <div>
          <p className="text-[11px] font-medium text-stone-500">指标</p>
          <p className="mt-0.5 text-stone-800">
            {request.amount} {request.unit}
          </p>
        </div>
        <div>
          <p className="text-[11px] font-medium text-stone-500">输入 Token</p>
          <p className="mt-0.5 text-stone-800">{formatUsageCount(request.input_tokens)}</p>
        </div>
        <div>
          <p className="text-[11px] font-medium text-stone-500">输出 Token</p>
          <p className="mt-0.5 text-stone-800">{formatUsageCount(request.output_tokens)}</p>
        </div>
        <div>
          <p className="text-[11px] font-medium text-stone-500">缓存 Token</p>
          <p className="mt-0.5 text-stone-800">{formatUsageCount(request.cache_tokens)}</p>
        </div>
        <div>
          <p className="text-[11px] font-medium text-stone-500">价格</p>
          <p className="mt-0.5 text-stone-800">{formatUsagePrice(request)}</p>
        </div>
        <div>
          <p className="text-[11px] font-medium text-stone-500">时间</p>
          <p className="mt-0.5 text-stone-800">{formatUsageTime(request.created_at)}</p>
        </div>
      </div>
      {metadata.responseBody ? (
        <div className="mt-3">
          <p className="text-[11px] font-medium text-stone-500">上游原始响应</p>
          <pre className="mt-1 max-h-56 overflow-auto rounded-lg border border-stone-200 bg-white p-2 font-mono text-[11px] leading-relaxed text-stone-700">
            {prettyJsonOrText(metadata.responseBody)}
          </pre>
        </div>
      ) : null}
      <div className="mt-3">
        <p className="text-[11px] font-medium text-stone-500">
          {metadata.valid ? "metadata_json" : "metadata_json 无法解析，显示原始内容。"}
        </p>
        <pre className="mt-1 max-h-56 overflow-auto rounded-lg border border-stone-200 bg-white p-2 font-mono text-[11px] leading-relaxed text-stone-700">
          {metadata.valid ? metadata.formattedJson : metadata.raw}
        </pre>
      </div>
    </div>
  );
}

type AccountsScreenProps = {
  platform?: PlatformKey;
  onOpenSessions?: (platform: PlatformKey) => void;
  sidebarCollapsed?: boolean;
};

const platformLabels: Record<PlatformKey, string> = {
  codex: "Codex",
  claude: "Claude",
  grok: "Grok",
  gemini: "Gemini",
  opencode: "OpenCode",
  openclaw: "OpenClaw",
  hermes: "Hermes",
};

const routeInterfaceFormats: InterfaceFormat[] = ["openai", "openai-responses", "anthropic", "gemini"];

const interfaceFormatLabels: Record<InterfaceFormat, string> = {
  openai: "OpenAI Chat Completions",
  "openai-responses": "OpenAI Responses",
  anthropic: "Claude Messages",
  gemini: "Gemini",
};

function interfaceFormatLabel(value: InterfaceFormat | string | null | undefined) {
  if (!value) {
    return "";
  }
  if (value in interfaceFormatLabels) {
    return interfaceFormatLabels[value as InterfaceFormat];
  }
  return value;
}

const anthropicApiKeyFields: Array<{ value: AnthropicApiKeyField; label: string; description: string }> = [
  {
    value: "ANTHROPIC_AUTH_TOKEN",
    label: "ANTHROPIC_AUTH_TOKEN",
    description: "Authorization: Bearer，兼容 cc-switch / Sub2API 常见配置",
  },
  {
    value: "ANTHROPIC_API_KEY",
    label: "ANTHROPIC_API_KEY",
    description: "x-api-key，Anthropic 官方 API Key 默认方式",
  },
];

const claudeModelTemplates = [
  { value: "claude-sonnet-5", label: "Sonnet", keywords: ["sonnet"] },
  { value: "claude-opus-4-8", label: "Opus", keywords: ["opus"] },
  { value: "claude-fable-5", label: "Fable", keywords: ["fable"] },
  { value: "claude-haiku-4-5", label: "Haiku", keywords: ["haiku", "flash", "mini", "lite"] },
] as const;

const claudeModelSources = [
  ...claudeModelTemplates.map((template) => ({
    value: template.value,
    label: `${template.label}（默认角色）`,
  })),
  { value: "claude-opus", label: "Claude Opus（旧版）" },
  { value: "claude-sonnet", label: "Claude Sonnet（旧版）" },
  { value: "claude-haiku", label: "Claude Haiku（旧版）" },
  { value: "claude-opus-4-20250514", label: "Claude Opus 4" },
  { value: "claude-sonnet-4-20250514", label: "Claude Sonnet 4" },
  { value: "claude-3-5-haiku-20241022", label: "Claude 3.5 Haiku" },
];

function defaultOfficialJson(platform: PlatformKey) {
  return `{
  "type": "${platform}",
  "email": "name@example.com",
  "access_token": "access-token",
  "refresh_token": "refresh-token"
}`;
}

function shortId(id: string) {
  return id.length > 8 ? id.slice(0, 8) : id;
}

const SINGLE_ACCOUNT_FILTER = "__single__";

function credentialBatchFilterKey(credential: RouteCredential): string {
  return credential.batch_id || SINGLE_ACCOUNT_FILTER;
}

function credentialBatchFilterLabel(key: string): string {
  return key === SINGLE_ACCOUNT_FILTER ? "单账号" : key;
}


function kindLabel(kind: RouteCredential["kind"]) {
  return kind === "api" ? "API" : "官方";
}

function parseJsonPreview(value: string, fallback: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return fallback;
  }
}

function decodeBase64Text(value: string) {
  const normalized = value.trim().replace(/-/g, "+").replace(/_/g, "/");
  if (!normalized) {
    throw new Error("empty");
  }

  const padded = normalized.padEnd(normalized.length + ((4 - (normalized.length % 4)) % 4), "=");
  const binary = atob(padded);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}

function apiKeyLines(value: string) {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function defaultInterfaceFormat(platform: PlatformKey): InterfaceFormat {
  if (platform === "claude") {
    return "anthropic";
  }
  if (platform === "gemini") {
    return "gemini";
  }
  // CLIProxyAPI xAI Grok uses OpenAI-compatible endpoints under api.x.ai/v1.
  if (platform === "grok") {
    return "openai";
  }
  return "openai";
}

function interfaceFormatsForPlatform(platform: PlatformKey): InterfaceFormat[] {
  if (platform === "gemini") {
    return ["gemini"];
  }
  if (platform === "codex" || platform === "claude") {
    return routeInterfaceFormats;
  }
  return [defaultInterfaceFormat(platform)];
}

function shouldShowInterfaceFormatSelect(platform: PlatformKey) {
  return interfaceFormatsForPlatform(platform).length > 1;
}

function isAnthropicInterfaceFormat(value: InterfaceFormat | string) {
  return value === "anthropic";
}

function shouldShowResponsesCustomToolCompat(platform: PlatformKey) {
  return platform === "codex";
}

function shouldShowResponsesCustomToolCompatForFormat(
  platform: PlatformKey,
  interfaceFormat: InterfaceFormat,
) {
  return shouldShowResponsesCustomToolCompat(platform) && interfaceFormat === "openai-responses";
}

function defaultAnthropicApiKeyFieldForCreate(platform: PlatformKey): AnthropicApiKeyField {
  return platform === "claude" ? "ANTHROPIC_AUTH_TOKEN" : "ANTHROPIC_API_KEY";
}

function anthropicApiKeyFieldFromConfig(
  config: Record<string, unknown>,
  fallback: AnthropicApiKeyField,
): AnthropicApiKeyField {
  const value = stringFromRecord(config, "api_key_field");
  return value === "ANTHROPIC_AUTH_TOKEN" || value === "ANTHROPIC_API_KEY" ? value : fallback;
}

function apiKeyFieldForPayload(
  interfaceFormat: InterfaceFormat,
  apiKeyField: AnthropicApiKeyField,
) {
  return isAnthropicInterfaceFormat(interfaceFormat) ? apiKeyField : undefined;
}

function anthropicApiKeyFieldDescription(value: AnthropicApiKeyField) {
  return anthropicApiKeyFields.find((field) => field.value === value)?.description ?? "";
}

function defaultModelMappings(_platform: PlatformKey): ModelMapping[] {
  return [];
}

function defaultRequestedModel(platform: PlatformKey, interfaceFormat?: InterfaceFormat | string) {
  if (platform === "claude" || interfaceFormat === "anthropic") {
    return "claude-sonnet-4-20250514";
  }
  if (platform === "gemini" || interfaceFormat === "gemini") {
    return "gemini-2.5-flash";
  }
  if (platform === "grok") {
    return "grok-4.5";
  }
  return "gpt-5.5";
}

function isClaudeTemplateSource(value: string) {
  return claudeModelTemplates.some((template) => template.value === value.trim());
}

function modelIdList(models: FetchedRouteModel[]) {
  return models.map((model) => model.id).filter(Boolean);
}

function pickModelByKeywords(models: FetchedRouteModel[], keywords: readonly string[]) {
  const ids = modelIdList(models);
  for (const keyword of keywords) {
    const model = ids.find((id) => id.toLowerCase().includes(keyword));
    if (model) {
      return model;
    }
  }
  return null;
}

const oneMModelPattern = /(?:^|[^a-z0-9])(?:1m|1-m|1_m|1million|one[-_\s]?million)(?:[^a-z0-9]|$)/i;

function fetchedModelSupportsOneM(model: FetchedRouteModel) {
  return (
    model.supports_1m === true ||
    oneMModelPattern.test(model.id) ||
    (model.owned_by ? oneMModelPattern.test(model.owned_by) : false)
  );
}

function modelIdSupportsOneM(models: FetchedRouteModel[], id: string) {
  const matched = models.find((model) => model.id === id);
  return matched ? fetchedModelSupportsOneM(matched) : oneMModelPattern.test(id);
}

function pickGeneralModel(platform: PlatformKey, models: FetchedRouteModel[]) {
  const ids = modelIdList(models);
  if (ids.length === 0) {
    return null;
  }
  if (platform === "gemini") {
    return pickModelByKeywords(models, ["gemini", "flash", "pro"]) ?? ids[0];
  }
  if (platform === "grok") {
    return (
      pickModelByKeywords(models, ["grok-4.5", "grok-4", "grok-3", "grok"]) ??
      ids.find((id) => !id.toLowerCase().includes("embedding")) ??
      ids[0]
    );
  }
  return (
    pickModelByKeywords(models, ["gpt-5.5", "gpt-5", "gpt-4o", "gpt", "claude", "sonnet"]) ??
    ids.find((id) => !id.toLowerCase().includes("embedding")) ??
    ids[0]
  );
}

function buildOneClickMappings(
  platform: PlatformKey,
  models: FetchedRouteModel[],
  interfaceFormat?: InterfaceFormat | string,
) {
  if (platform === "claude") {
    const fallback = pickGeneralModel(platform, models);
    return claudeModelTemplates
      .map((template) => {
        const target = pickModelByKeywords(models, template.keywords) ?? fallback ?? "";
        return {
          from: template.value,
          to: target,
          label: template.label,
          ...(target && modelIdSupportsOneM(models, target) ? { supports_1m: true } : {}),
        };
      })
      .filter((mapping) => mapping.to.trim());
  }

  const model = pickGeneralModel(platform, models);
  return model
    ? [
        {
          from: defaultRequestedModel(platform, interfaceFormat),
          to: model,
        },
      ]
    : [];
}

function parseModelMappingsFromConfig(configJson: string): ModelMapping[] {
  try {
    const parsed = JSON.parse(configJson) as { model_mappings?: unknown };
    if (!Array.isArray(parsed.model_mappings)) {
      return [];
    }
    return parsed.model_mappings
      .filter((item): item is ModelMapping => {
        if (!item || typeof item !== "object") {
          return false;
        }
        const candidate = item as Partial<ModelMapping>;
        return typeof candidate.from === "string" && typeof candidate.to === "string";
      })
      .map((item) => ({
        from: item.from,
        to: item.to,
        label: item.label ?? null,
        supports_1m:
          item.supports_1m === true || (item as { supports1m?: unknown }).supports1m === true
            ? true
            : null,
      }));
  } catch {
    return [];
  }
}

function normalizeModelMappings(mappings: ModelMapping[], platform: PlatformKey) {
  const normalized: ModelMapping[] = [];
  for (const mapping of mappings) {
    const from = mapping.from.trim();
    const to = mapping.to.trim();
    const label = mapping.label?.trim() ?? "";
    if (!from && !to) {
      continue;
    }
    if (platform === "claude" && isClaudeTemplateSource(from) && !to) {
      continue;
    }
    if (!from || !to) {
      return {
        error: "模型映射需要同时填写请求模型和上游模型。",
        mappings: [],
      };
    }
    if (from === "upstream-model" || to === "upstream-model") {
      return {
        error: "upstream-model 只是示例占位，请填写真实上游模型名或删除该映射。",
        mappings: [],
      };
    }
    const normalizedMapping: ModelMapping = label ? { from, to, label } : { from, to };
    if (platform === "claude" && mapping.supports_1m === true) {
      normalizedMapping.supports_1m = true;
    }
    normalized.push(normalizedMapping);
  }

  return { error: null, mappings: normalized };
}


function numberFromRecord(record: Record<string, unknown>, key: string): number | null {
  const value = record[key];
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function officialSubscriptionType(credential: RouteCredential): string | null {
  if (credential.kind !== "official") {
    return null;
  }
  const direct =
    typeof credential.subscription_type === "string"
      ? credential.subscription_type.trim()
      : "";
  if (direct) {
    return direct;
  }
  const config = parseJsonObject(credential.config_json);
  const value = stringFromRecord(config, "subscription_type");
  return value || null;
}

function officialPrimaryRemain(credential: RouteCredential): number | null {
  if (credential.kind !== "official") {
    return null;
  }
  if (typeof credential.primary_remain === "number" && Number.isFinite(credential.primary_remain)) {
    return credential.primary_remain;
  }
  if (typeof credential.quota_remaining === "number" && Number.isFinite(credential.quota_remaining)) {
    return credential.quota_remaining;
  }
  const config = parseJsonObject(credential.config_json);
  return numberFromRecord(config, "primary_remain") ?? numberFromRecord(config, "quota_remaining");
}

function officialWeeklyRemain(credential: RouteCredential): number | null {
  if (credential.kind !== "official") {
    return null;
  }
  if (typeof credential.weekly_remain === "number" && Number.isFinite(credential.weekly_remain)) {
    return credential.weekly_remain;
  }
  const config = parseJsonObject(credential.config_json);
  return numberFromRecord(config, "weekly_remain");
}

function officialLatestResetLabel(credential: RouteCredential): string | null {
  if (credential.kind !== "official") {
    return null;
  }
  const config = parseJsonObject(credential.config_json);
  const candidates = [
    typeof credential.reset_primary === "string" ? credential.reset_primary : null,
    typeof credential.reset_weekly === "string" ? credential.reset_weekly : null,
    typeof credential.quota_updated_at === "string" ? credential.quota_updated_at : null,
    stringFromRecord(config, "reset_primary") || null,
    stringFromRecord(config, "reset_weekly") || null,
    stringFromRecord(config, "quota_updated_at") || null,
  ]
    .map((value) => (value ? value.trim() : ""))
    .filter(Boolean);
  if (candidates.length === 0) {
    return null;
  }
  // RFC3339 strings compare lexicographically for latest time.
  const latest = candidates.reduce((best, current) => (current > best ? current : best));
  return latest;
}

function parseJsonObject(value: string) {
  try {
    const parsed = JSON.parse(value) as unknown;
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
}

function stringFromRecord(record: Record<string, unknown>, key: string) {
  const value = record[key];
  return typeof value === "string" ? value.trim() : "";
}

/// Extract the domain (origin) from a credential's configured base_url so the
/// account row can offer a clickable link. Returns null when there is no valid
/// http(s) base_url (e.g. most official credentials).
function credentialBaseUrlLink(credential: RouteCredential): { href: string; host: string } | null {
  const baseUrl = stringFromRecord(parseJsonObject(credential.config_json), "base_url");
  if (!baseUrl) {
    return null;
  }
  try {
    const url = new URL(baseUrl);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      return null;
    }
    return { href: url.origin, host: url.host };
  } catch {
    return null;
  }
}

function interfaceFormatFromConfig(config: Record<string, unknown>): InterfaceFormat {
  const value = stringFromRecord(config, "interface_format");
  return routeInterfaceFormats.includes(value as InterfaceFormat) ? (value as InterfaceFormat) : "openai";
}

function apiSecretJsonWithKey(secretJson: string, apiKey: string) {
  const secret = parseJsonObject(secretJson);
  secret.api_key = apiKey.trim();
  return JSON.stringify(secret, null, 2);
}

function responsesCustomToolCompatFromConfig(config: Record<string, unknown>): boolean {
  return config.responses_custom_tool_compat === true;
}

function inlineRemoteImagesFromConfig(config: Record<string, unknown>): boolean {
  return config.inline_remote_images === true;
}

function apiConfigJsonWithFields(
  configJson: string,
  baseUrl: string,
  interfaceFormat: InterfaceFormat,
  mappings: ModelMapping[],
  apiKeyField: AnthropicApiKeyField,
  responsesCustomToolCompat = false,
  userAgent = "",
  inlineRemoteImages = false,
) {
  const config = parseJsonObject(configJson);
  config.base_url = baseUrl.trim();
  config.interface_format = interfaceFormat;
  config.model_mappings = mappings;
  config.responses_custom_tool_compat = responsesCustomToolCompat;
  config.inline_remote_images = inlineRemoteImages;
  if (isAnthropicInterfaceFormat(interfaceFormat)) {
    config.api_key_field = apiKeyField;
  } else {
    delete config.api_key_field;
  }
  return JSON.stringify(writeUserAgentToConfig(config, userAgent), null, 2);
}

function credentialRetryLabel(credential: RouteCredential): string | null {
  const raw = credential.cooldown_until || credential.next_retry_at;
  if (!raw) {
    return null;
  }
  const date = new Date(raw);
  if (!Number.isFinite(date.getTime()) || date.getTime() <= Date.now()) {
    return null;
  }
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function credentialRequestStats(credential: RouteCredential) {
  const requestCount = credential.request_count ?? 0;
  if (requestCount <= 0) {
    return {
      requestCount: 0,
      successCount: 0,
      failureCount: 0,
      rateLabel: "-",
    };
  }
  const successCount = credential.success_count ?? 0;
  const failureCount = credential.failure_count ?? Math.max(0, requestCount - successCount);
  const successRate = credential.success_rate ?? (successCount / requestCount) * 100;
  const rateLabel = Number.isFinite(successRate) ? `${successRate.toFixed(1).replace(/\.0$/, "")}%` : "-";
  return { requestCount, successCount, failureCount, rateLabel };
}

function apiPreviewJsonFromPayloads(platform: PlatformKey, secretJson: string, configJson: string) {
  const secret = parseJsonObject(secretJson);
  const config = parseJsonObject(configJson);
  const baseUrl = stringFromRecord(config, "base_url") || null;
  const interfaceFormat = stringFromRecord(config, "interface_format") || null;

  if (platform === "codex") {
    const configToml = `model_provider = "ai-switch"\n\n[model_providers.ai-switch]\nbase_url = "${baseUrl ?? "http://127.0.0.1:43111/v1"}"\n`;
    return JSON.stringify(
      {
        auth_json: {
          api_key: stringFromRecord(secret, "api_key") || "<api-key>",
        },
        config_toml: configToml,
      },
      null,
      2,
    );
  }

  if (platform === "claude" || platform === "gemini" || platform === "grok") {
    const apiKeyField = stringFromRecord(config, "api_key_field") || null;
    return JSON.stringify(
      {
        settings_json: JSON.stringify({
          aiSwitch: {
            kind: "api",
            baseUrl,
            interfaceFormat,
            apiKeyField,
          },
        }),
      },
      null,
      2,
    );
  }

  return "{}";
}

function apiPreviewJsonWithFields(
  platform: PlatformKey,
  secretJson: string,
  apiKey: string,
  configJson: string,
  baseUrl: string,
  interfaceFormat: InterfaceFormat,
  mappings: ModelMapping[],
  apiKeyField: AnthropicApiKeyField,
  responsesCustomToolCompat = false,
  userAgent = "",
) {
  return apiPreviewJsonFromPayloads(
    platform,
    apiSecretJsonWithKey(secretJson, apiKey),
    apiConfigJsonWithFields(
      configJson,
      baseUrl,
      interfaceFormat,
      mappings,
      apiKeyField,
      responsesCustomToolCompat,
      userAgent,
    ),
  );
}

type ModelMappingsEditorProps = {
  error?: string | null;
  fetchError?: string | null;
  fetchedModels?: FetchedRouteModel[];
  interfaceFormat?: InterfaceFormat | string;
  isFetchingModels?: boolean;
  label: string;
  onChange: (mappings: ModelMapping[]) => void;
  onFetchModels?: () => void;
  platform: PlatformKey;
  value: ModelMapping[];
};

function ModelMappingsEditor({
  error,
  fetchError,
  fetchedModels = [],
  interfaceFormat,
  isFetchingModels = false,
  label,
  onChange,
  onFetchModels,
  platform,
  value,
}: ModelMappingsEditorProps) {
  const isClaude = platform === "claude";
  const templateValues = new Set<string>(claudeModelTemplates.map((template) => template.value));
  const rows = isClaude
    ? [
        ...claudeModelTemplates.map((template) => {
          const existing = value.find((mapping) => mapping.from.trim() === template.value);
          return {
            from: template.value,
            to: existing?.to ?? "",
            label: existing?.label ?? template.label,
            supports_1m: existing?.supports_1m ?? false,
          };
        }),
        ...value.filter((mapping) => !templateValues.has(mapping.from.trim())),
      ]
    : value;
  const modelListId = `${platform}-${label}-fetched-models`.replace(/[^a-zA-Z0-9_-]/g, "-");
  const sourceOptions =
    isClaude
      ? [
          ...claudeModelSources,
          ...value
            .filter(
              (mapping) =>
                mapping.from.trim() &&
                !claudeModelSources.some((option) => option.value === mapping.from.trim()),
            )
            .map((mapping) => ({
              value: mapping.from.trim(),
              label: `${mapping.from.trim()}（已有）`,
            })),
        ]
      : [];

  const updateRow = (index: number, patch: Partial<ModelMapping>) => {
    const next = rows.map((mapping, rowIndex) =>
      rowIndex === index ? { ...mapping, ...patch } : mapping,
    );
    onChange(next);
  };

  const removeRow = (index: number) => {
    const next = rows.filter((_, rowIndex) => rowIndex !== index);
    onChange(next);
  };

  const addRow = () => {
    onChange([...rows, { from: "", to: "", label: null }]);
  };

  const oneClickSetup = () => {
    onChange(buildOneClickMappings(platform, fetchedModels, interfaceFormat));
  };

  return (
    <div className="grid gap-2">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <p className="text-[12px] font-semibold text-stone-600">{label}</p>
        <div className="flex flex-wrap items-center gap-2">
          {onFetchModels ? (
            <button
              className="inline-flex items-center gap-1.5 rounded-lg border border-blue-200 bg-blue-50 px-2.5 py-1.5 text-[12px] font-semibold text-blue-900 transition-colors hover:bg-blue-100 disabled:opacity-50"
              disabled={isFetchingModels}
              onClick={onFetchModels}
              type="button"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${isFetchingModels ? "animate-spin" : ""}`} />
              {isFetchingModels ? "获取中..." : "获取模型列表"}
            </button>
          ) : null}
          <button
            className="inline-flex items-center gap-1.5 rounded-lg border border-emerald-200 bg-emerald-50 px-2.5 py-1.5 text-[12px] font-semibold text-emerald-900 transition-colors hover:bg-emerald-100 disabled:opacity-50"
            disabled={fetchedModels.length === 0}
            onClick={oneClickSetup}
            type="button"
          >
            <Wand2 className="h-3.5 w-3.5" />
            一键设置
          </button>
          <button
            className="inline-flex items-center gap-1.5 rounded-lg border border-stone-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50"
            onClick={addRow}
            type="button"
          >
            <Plus className="h-3.5 w-3.5" />
            新增映射
          </button>
        </div>
      </div>
      <p className="text-[11px] font-medium leading-5 text-stone-500">
        留空表示不改写模型；获取列表只用于辅助选择，只有保存账号时才写入映射。
        {isClaude ? " 勾选 1M 会声明该 Claude 角色支持 1M 上下文。" : ""}
        {fetchedModels.length > 0 ? ` 已获取 ${fetchedModels.length} 个模型。` : ""}
      </p>
      {value.length === 0 ? (
        <p className="rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-[11px] font-semibold leading-5 text-amber-900">
          如果上游只支持有限模型，建议先获取模型列表并配置模型映射；配置后算力池只会把该账号匹配到映射别名。
        </p>
      ) : (
        <p className="rounded-lg border border-blue-100 bg-blue-50 px-3 py-2 text-[11px] font-medium leading-5 text-blue-900">
          当前账号仅按已配置的本地模型别名参与匹配，共 {value.length} 条。
        </p>
      )}
      {fetchError ? <p className="text-[12px] font-semibold text-red-700">{fetchError}</p> : null}
      {fetchedModels.length > 0 ? (
        <datalist id={modelListId}>
          {fetchedModels.map((model) => (
            <option key={model.id} value={model.id}>
              {model.owned_by ?? model.id}
            </option>
          ))}
        </datalist>
      ) : null}

      <div className="space-y-2 rounded-xl border border-stone-200 bg-stone-50/70 p-2">
        {rows.length === 0 ? (
          <div className="rounded-lg border border-dashed border-stone-200 bg-white px-3 py-3 text-[12px] font-medium text-stone-500">
            暂无模型映射。需要改写上游模型时或者该上游模型有限时再新增。
          </div>
        ) : (
          rows.map((mapping, index) => {
            const isTemplateRow = isClaude && isClaudeTemplateSource(mapping.from);
            return (
              <div
                className={`grid gap-2 sm:items-center ${
                  isClaude
                    ? "sm:grid-cols-[0.7fr_minmax(0,1fr)_auto_minmax(0,1fr)_auto_auto]"
                    : "sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)_auto]"
                }`}
                key={isTemplateRow ? `claude-template-${mapping.from}` : `model-mapping-${index}`}
              >
                {isClaude ? (
                  <>
                    <input
                      aria-label={`显示名称 ${index + 1}`}
                      className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
                      onChange={(event) => updateRow(index, { label: event.target.value })}
                      placeholder="Sonnet"
                      value={mapping.label ?? ""}
                    />
                    <select
                      aria-label={`请求模型 ${index + 1}`}
                      className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
                      disabled={isTemplateRow}
                      onChange={(event) => updateRow(index, { from: event.target.value })}
                      value={mapping.from}
                    >
                      <option value="">选择请求模型</option>
                      {sourceOptions.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                  </>
                ) : (
                  <input
                    aria-label={`请求模型 ${index + 1}`}
                    className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
                    list={modelListId}
                    onChange={(event) => updateRow(index, { from: event.target.value })}
                    placeholder="gpt-5.5"
                    value={mapping.from}
                  />
                )}
                <ArrowRight className="hidden h-4 w-4 text-stone-400 sm:block" />
                <input
                  aria-label={`上游模型 ${index + 1}`}
                  className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
                  list={modelListId}
                  onChange={(event) => updateRow(index, { to: event.target.value })}
                  placeholder="例如：gpt-4o"
                  value={mapping.to}
                />
                {isClaude ? (
                  <label className="inline-flex h-9 items-center justify-center gap-1.5 rounded-xl border border-stone-200 bg-white px-2.5 text-[12px] font-semibold text-stone-600">
                    <input
                      aria-label={`声明支持 1M ${index + 1}`}
                      checked={mapping.supports_1m === true}
                      className="h-3.5 w-3.5 accent-blue-600"
                      onChange={(event) => updateRow(index, { supports_1m: event.target.checked })}
                      type="checkbox"
                    />
                    1M
                  </label>
                ) : null}
                <button
                  aria-label={`删除模型映射 ${index + 1}`}
                  className="grid h-9 w-9 place-items-center rounded-xl border border-stone-200 bg-white text-stone-500 transition-colors hover:bg-red-50 hover:text-red-700"
                  onClick={() => removeRow(index)}
                  type="button"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </div>
            );
          })
        )}
      </div>

      {error && <p className="text-[12px] font-semibold text-red-700">{error}</p>}
    </div>
  );
}

function modelTestStatusLine(outcome: RoutePoolModelTestOutcome) {
  const status = outcome.response_status ? `HTTP ${outcome.response_status}` : "无 HTTP 状态";
  return `${status} · ${outcome.duration_ms} ms`;
}

function modelTestTargetText(outcome: RoutePoolModelTestOutcome) {
  if (outcome.target_url) {
    return outcome.target_url;
  }
  if (outcome.base_url) {
    return `${outcome.base_url.replace(/\/$/, "")}${outcome.request_path}`;
  }
  return outcome.request_path;
}

const modelTestPrompt = "Reply with exactly: ai-switch-ok";

function modelTestProxyPath(platform: PlatformKey, interfaceFormat: string, model: string) {
  if (interfaceFormat === "openai-responses") {
    return "/responses";
  }
  if (interfaceFormat === "openai") {
    return "/chat/completions";
  }
  if (interfaceFormat === "anthropic") {
    return "/v1/messages";
  }
  if (interfaceFormat === "gemini") {
    return "/v1beta/models/" + encodeURIComponent(model) + ":generateContent";
  }
  return platform === "gemini" ? "/v1beta/models/" + encodeURIComponent(model) + ":generateContent" : "/chat/completions";
}

function modelTestRequestBody(interfaceFormat: string, model: string) {
  if (interfaceFormat === "openai-responses") {
    return {
      model,
      input: modelTestPrompt,
      temperature: 0,
      max_output_tokens: 16,
    };
  }
  if (interfaceFormat === "anthropic") {
    return {
      model,
      messages: [{ role: "user", content: modelTestPrompt }],
      max_tokens: 16,
    };
  }
  if (interfaceFormat === "gemini") {
    return {
      contents: [{ role: "user", parts: [{ text: modelTestPrompt }] }],
      generationConfig: { temperature: 0, maxOutputTokens: 16 },
    };
  }
  return {
    model,
    messages: [{ role: "user", content: modelTestPrompt }],
    temperature: 0,
    max_tokens: 16,
  };
}

function windowsDoubleQuote(value: string) {
  return '"' + value.replace(/"/g, '""') + '"';
}

function shellSingleQuote(value: string) {
  return "'" + value.replace(/'/g, "'\"'\"'") + "'";
}

function compactJsonForCurl(value: string) {
  try {
    const compact = JSON.stringify(JSON.parse(value));
    return typeof compact === "string" ? compact : value;
  } catch {
    return value.replace(/\r?\n/g, " ").trim();
  }
}

function joinUrl(baseUrl: string, path: string) {
  return baseUrl.replace(/\/+$/, "") + "/" + path.replace(/^\/+/, "");
}

function modelTestCurlCommand({
  activePlatform,
  codexEndpoint,
  outcome,
  proxyKey,
  proxyBaseUrl,
  requestedModel,
  shell = "posix",
}: {
  activePlatform: PlatformKey;
  codexEndpoint: CodexModelTestEndpoint;
  outcome: RoutePoolModelTestOutcome | null;
  proxyBaseUrl: string;
  proxyKey: string;
  requestedModel: string;
  shell?: "posix" | "windows";
}) {
  const interfaceFormat =
    outcome?.interface_format ||
    (activePlatform === "codex" ? codexModelTestInterfaceFormat(codexEndpoint) : defaultInterfaceFormat(activePlatform));
  const model = requestedModel.trim() || defaultRequestedModel(activePlatform, interfaceFormat);
  const requestPath =
    outcome?.route_proxy_entry_path || modelTestProxyPath(activePlatform, interfaceFormat, model);
  const requestBody =
    outcome?.request_body_json?.trim() || JSON.stringify(modelTestRequestBody(interfaceFormat, model), null, 2);
  const url = joinUrl(proxyBaseUrl.trim(), requestPath);
  const tlsOptions = url.toLowerCase().startsWith("https://") ? ["--ssl-no-revoke"] : [];
  const quote = shell === "windows" ? windowsDoubleQuote : shellSingleQuote;

  return [
    "curl.exe " + quote(url),
    ...tlsOptions,
    "-X POST",
    "-H " + quote("Content-Type: application/json"),
    "-H " + quote("Authorization: Bearer " + proxyKey),
    "-H " + quote("x-ai-switch-platform: " + activePlatform),
    "--data-raw " + quote(compactJsonForCurl(requestBody)),
  ].join(" ");
}

function modelTestRouteChainItems(outcome: RoutePoolModelTestOutcome) {
  const entry =
    outcome.route_proxy_entry_url ?? outcome.route_proxy_entry_path ?? outcome.request_path;
  return [
    {
      label: "算力池入口",
      value: entry,
    },
    {
      label: "命中账号",
      value: `${outcome.selected_account_name} · ${outcome.selected_account_id}`,
    },
    {
      label: "上游接口",
      value: modelTestTargetText(outcome),
    },
  ].filter((item) => item.value.trim().length > 0);
}

function prettyJsonOrText(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}

function CredentialFailureTooltip({
  credential,
  children,
}: {
  credential: RouteCredential;
  children: ReactNode;
}) {
  const tooltipId = useId();
  const response = credential.last_failure_response_json?.trim();
  if (!response) {
    return <>{children}</>;
  }

  return (
    <span
      aria-describedby={tooltipId}
      className="group relative inline-flex max-w-full outline-none focus:ring-2 focus:ring-red-300"
      tabIndex={0}
    >
      {children}
      <span
        className="pointer-events-none absolute left-0 top-full z-50 mt-1 hidden max-h-80 w-[min(36rem,calc(100vw-2rem))] max-w-[calc(100vw-2rem)] overflow-auto whitespace-normal break-words rounded-lg border border-stone-700 bg-stone-900 px-3 py-2 text-left text-[11px] font-medium leading-5 text-white shadow-xl group-hover:block group-focus-within:block"
        id={tooltipId}
        role="tooltip"
      >
        <span className="block text-stone-300">
          失败类型：{credential.last_failure_kind?.trim() || "未知"}
        </span>
        {credential.last_failure_message?.trim() ? (
          <span className="mt-1 block break-words">
            失败消息：{credential.last_failure_message.trim()}
          </span>
        ) : null}
        <pre className="mt-2 whitespace-pre-wrap break-words font-mono text-[10px] leading-4 text-stone-100">
          {prettyJsonOrText(response)}
        </pre>
      </span>
    </span>
  );
}

function UserAgentFields({
  fieldClass,
  idPrefix,
  labelClass,
  onChange,
  value,
}: {
  fieldClass: string;
  idPrefix: string;
  labelClass: string;
  onChange: (next: string) => void;
  value: string;
}) {
  const preset = matchUserAgentPreset(value);
  return (
    <div className="grid gap-2">
      <label className={labelClass}>
        User-Agent 预设
        <select
          aria-label={`${idPrefix} User-Agent 预设`}
          className={fieldClass}
          onChange={(event) => {
            const selected = USER_AGENT_PRESETS.find((item) => item.id === event.target.value);
            if (!selected) {
              return;
            }
            if (selected.id === "custom") {
              onChange(value);
              return;
            }
            onChange(selected.value);
          }}
          value={preset}
        >
          {USER_AGENT_PRESETS.map((item) => (
            <option key={item.id} value={item.id}>
              {item.label}
            </option>
          ))}
        </select>
      </label>
      <label className={labelClass}>
        User-Agent
        <input
          aria-label={`${idPrefix} User-Agent`}
          className={fieldClass}
          onChange={(event) => onChange(event.target.value)}
          placeholder="留空则使用默认/内置 UA"
          value={value}
        />
      </label>
    </div>
  );
}

export function AccountsScreen({
  onOpenSessions,
  platform = "codex",
  sidebarCollapsed = false,
}: AccountsScreenProps) {
  const queryClient = useQueryClient();
  const activePlatform = platform;
  const capabilitiesQuery = usePlatformCapabilities();
  const activeCapability = findPlatformCapability(capabilitiesQuery.data, activePlatform);
  const capabilityReady = capabilitiesQuery.isSuccess && Boolean(activeCapability);
  const configWriteRule = activeCapability?.operations.config_write;
  const officialImportRule = activeCapability?.operations.official_import;
  const officialQuotaRule = activeCapability?.operations.official_quota;
  const modelTestRule = activeCapability?.operations.model_test;
  const configWriteEnabled = capabilityReady && operationEnabled(configWriteRule);
  const officialImportEnabled = capabilityReady && operationEnabled(officialImportRule);
  const officialQuotaEnabled = capabilityReady && operationEnabled(officialQuotaRule);
  const modelTestEnabled = capabilityReady && operationEnabled(modelTestRule);
  const configWriteReason = capabilityReason(configWriteRule);
  const officialImportReason = capabilityReason(officialImportRule);
  const officialQuotaReason = capabilityReason(officialQuotaRule);
  const modelTestReason = capabilityReason(modelTestRule);
  const [draftPoolIds, setDraftPoolIds] = useState<Set<string>>(() => new Set());
  const [selectedAccountIds, setSelectedAccountIds] = useState<Set<string>>(() => new Set());
  const [batchStatus, setBatchStatus] = useState<AccountStatus | "">("");
  const [accountFilters, setAccountFilters] = useState<string[]>([]);
  const [accountPage, setAccountPage] = useState(1);
  const [accountPageSize, setAccountPageSize] = useState(20);
  const [draggedAccountId, setDraggedAccountId] = useState<string | null>(null);
  const [dragTargetIndex, setDragTargetIndex] = useState<number | null>(null);
  const accountEdgeTimerRef = useRef<number | null>(null);
  const [accountFilterMenuOpen, setAccountFilterMenuOpen] = useState(false);
  const [refreshMenuOpen, setRefreshMenuOpen] = useState(false);
  const [modelTestMenuOpen, setModelTestMenuOpen] = useState(false);
  const [modelTestMenuCopied, setModelTestMenuCopied] = useState<
    "curl" | "curl-cmd" | "base-url" | "sk" | null
  >(null);
  const [copiedCredentialId, setCopiedCredentialId] = useState<string | null>(null);
  const accountFilterMenuRef = useRef<HTMLDivElement | null>(null);
  const refreshMenuRef = useRef<HTMLDivElement | null>(null);
  const modelTestMenuRef = useRef<HTMLDivElement | null>(null);
  const [accountView, setAccountView] = useState<AccountView>("in_pool");
  const [toolbarAutoHidden, setToolbarAutoHidden] = useState(false);
  const toolbarHideTimerRef = useRef<number | null>(null);
  const toolbarHoveredRef = useRef(false);
  const toolbarAutoHideEligibleRef = useRef(false);
  const [statsPeriod, setStatsPeriod] = useState<RouteStatsPeriod>("today");
  const [requestPage, setRequestPage] = useState(1);
  const [expandedRequestId, setExpandedRequestId] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [createMode, setCreateMode] = useState<CreateMode>("api");
  const [joinPoolOnCreate, setJoinPoolOnCreate] = useState(true);
  const [officialText, setOfficialText] = useState(() => defaultOfficialJson(activePlatform));
  const [officialBatchName, setOfficialBatchName] = useState("");
  const [officialFilePaths, setOfficialFilePaths] = useState<string[]>([]);
  const [apiName, setApiName] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [apiKeyDecodeError, setApiKeyDecodeError] = useState<string | null>(null);
  const [apiKeyOcrError, setApiKeyOcrError] = useState<string | null>(null);
  const [apiKeyOcrRecognizing, setApiKeyOcrRecognizing] = useState(false);
  const apiKeyOcrFileInputRef = useRef<HTMLInputElement | null>(null);
  const [apiBaseUrl, setApiBaseUrl] = useState(() =>
    activePlatform === "grok" ? "https://api.x.ai/v1" : "https://api.example.com/v1",
  );
  const [apiInterfaceFormat, setApiInterfaceFormat] = useState<InterfaceFormat>(() =>
    defaultInterfaceFormat(activePlatform),
  );
  const [apiResponsesCustomToolCompat, setApiResponsesCustomToolCompat] = useState(false);
  const [apiUserAgent, setApiUserAgent] = useState("");
  const [apiKeyField, setApiKeyField] = useState<AnthropicApiKeyField>(() =>
    defaultAnthropicApiKeyFieldForCreate(activePlatform),
  );
  const [apiMappings, setApiMappings] = useState<ModelMapping[]>(() => defaultModelMappings(activePlatform));
  const [apiMappingsError, setApiMappingsError] = useState<string | null>(null);
  const [apiFetchedModels, setApiFetchedModels] = useState<FetchedRouteModel[]>([]);
  const [apiFetchModelsError, setApiFetchModelsError] = useState<string | null>(null);
  const [apiPreviewJson, setApiPreviewJson] = useState("");
  const [editingCredential, setEditingCredential] = useState<RouteCredential | null>(null);
  const [editName, setEditName] = useState("");
  const [editEmail, setEditEmail] = useState("");
  const [editStatus, setEditStatus] = useState<AccountStatus>("ok");
  const [editPriority, setEditPriority] = useState(3);
  const [editMaxConcurrency, setEditMaxConcurrency] = useState("1");
  const [editApiKey, setEditApiKey] = useState("");
  const [editApiKeyDecodeError, setEditApiKeyDecodeError] = useState<string | null>(null);
  const [editApiKeyOcrError, setEditApiKeyOcrError] = useState<string | null>(null);
  const [editApiKeyOcrRecognizing, setEditApiKeyOcrRecognizing] = useState(false);
  const editApiKeyOcrFileInputRef = useRef<HTMLInputElement | null>(null);
  const [editApiBaseUrl, setEditApiBaseUrl] = useState("");
  const [editApiInterfaceFormat, setEditApiInterfaceFormat] = useState<InterfaceFormat>("openai");
  const [editResponsesCustomToolCompat, setEditResponsesCustomToolCompat] = useState(false);
  const [editInlineRemoteImages, setEditInlineRemoteImages] = useState(false);
  const [editUserAgent, setEditUserAgent] = useState("");
  const [editApiKeyField, setEditApiKeyField] = useState<AnthropicApiKeyField>("ANTHROPIC_API_KEY");
  const [editSecretJson, setEditSecretJson] = useState("{}");
  const [editConfigJson, setEditConfigJson] = useState("{}");
  const [editModelMappings, setEditModelMappings] = useState<ModelMapping[]>([]);
  const [editModelMappingsError, setEditModelMappingsError] = useState<string | null>(null);
  const [editFetchedModels, setEditFetchedModels] = useState<FetchedRouteModel[]>([]);
  const [editFetchModelsError, setEditFetchModelsError] = useState<string | null>(null);
  const [editPreviewJson, setEditPreviewJson] = useState("{}");
  const [lastRouteAccount, setLastRouteAccount] = useState<string | null>(null);
  const [routeTestModelsByPlatform, setRouteTestModelsByPlatform] = useState<Partial<Record<PlatformKey, string>>>({});
  const [codexModelTestEndpoint, setCodexModelTestEndpoint] =
    useState<CodexModelTestEndpoint>(() => loadCodexModelTestEndpoint());
  const [modelTestDialogOpen, setModelTestDialogOpen] = useState(false);
  const [routePoolModelsDialogOpen, setRoutePoolModelsDialogOpen] = useState(false);
  const [liveLogOpen, setLiveLogOpen] = useState(false);
  const [liveLogEntries, setLiveLogEntries] = useState<RouteProxyLiveLogEntry[]>([]);
  const [expandedLiveLogId, setExpandedLiveLogId] = useState<string | null>(null);
  const [modelTestAccount, setModelTestAccount] = useState<RouteCredential | null>(null);
  const [exportRequest, setExportRequest] = useState<{
    selection_context: RouteCredentialSelectionContext;
    credential_ids: string[];
  } | null>(null);
  const [testingAccountId, setTestingAccountId] = useState<string | null>(null);
  const [refreshingQuotaId, setRefreshingQuotaId] = useState<string | null>(null);
  const [quotaRefreshMessage, setQuotaRefreshMessage] = useState<string | null>(null);
  const autoQuotaRefreshedPlatform = useRef<string | null>(null);
  const [modelTestOutcome, setModelTestOutcome] = useState<RoutePoolModelTestOutcome | null>(null);
  const [configWriteOutcomes, setConfigWriteOutcomes] = useState<ConfigWriteOutcome[]>([]);
  const [configWriteError, setConfigWriteError] = useState<string | null>(null);
  const [routePoolFeedback, setRoutePoolFeedback] = useState<RoutePoolFeedback>(null);
  const routeTestModel = routeTestModelsByPlatform[activePlatform] ?? "";
  const statsOpen = accountView === "stats";
  const accountScope: RouteCredentialPoolScope =
    accountView === "archived"
      ? "archived"
      : accountView === "out_of_pool"
        ? "out_of_pool"
        : "in_pool";
  const poolMemberKey = useMemo(
    () => Array.from(draftPoolIds).sort().join(","),
    [draftPoolIds],
  );
  const statsSince = useMemo(() => routeStatsSince(statsPeriod), [statsPeriod]);

  const clearToolbarHideTimer = useCallback(() => {
    if (toolbarHideTimerRef.current != null) {
      window.clearTimeout(toolbarHideTimerRef.current);
      toolbarHideTimerRef.current = null;
    }
  }, []);
  const scheduleToolbarHide = useCallback(() => {
    clearToolbarHideTimer();
    if (!toolbarAutoHideEligibleRef.current) {
      return;
    }
    toolbarHideTimerRef.current = window.setTimeout(() => {
      toolbarHideTimerRef.current = null;
      if (!toolbarHoveredRef.current) {
        setToolbarAutoHidden(true);
      }
    }, 2600);
  }, [clearToolbarHideTimer]);
  const revealToolbar = useCallback(() => {
    clearToolbarHideTimer();
    setToolbarAutoHidden(false);
    if (!toolbarHoveredRef.current && toolbarAutoHideEligibleRef.current) {
      scheduleToolbarHide();
    }
  }, [clearToolbarHideTimer, scheduleToolbarHide]);

  useEffect(() => {
    if (!isTauriRuntime()) {
      toolbarAutoHideEligibleRef.current = false;
      return clearToolbarHideTimer;
    }

    let disposed = false;
    const updateEligibility = async () => {
      try {
        const tauriInternals = (window as typeof window & {
          __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } };
        }).__TAURI_INTERNALS__;
        const label = tauriInternals?.metadata?.currentWindow?.label ?? "main";
        const [position, monitor] = await Promise.all([
          tauriInvoke<{ y: number }>("plugin:window|outer_position", { label }),
          tauriInvoke<{
            position?: { y?: number };
            workArea?: { position?: { y?: number } };
          } | null>("plugin:window|current_monitor"),
        ]);
        if (disposed) {
          return;
        }
        const monitorTop = monitor?.workArea?.position?.y ?? monitor?.position?.y ?? 0;
        const eligible = position.y <= monitorTop + 12;
        toolbarAutoHideEligibleRef.current = eligible;
        if (!eligible) {
          clearToolbarHideTimer();
          setToolbarAutoHidden(false);
        } else if (!toolbarHoveredRef.current) {
          scheduleToolbarHide();
        }
      } catch {
        if (disposed) {
          return;
        }
        toolbarAutoHideEligibleRef.current = false;
        clearToolbarHideTimer();
        setToolbarAutoHidden(false);
      }
    };

    void updateEligibility();
    const positionPoll = window.setInterval(() => {
      void updateEligibility();
    }, 500);

    return () => {
      disposed = true;
      window.clearInterval(positionPoll);
      clearToolbarHideTimer();
    };
  }, [clearToolbarHideTimer, scheduleToolbarHide]);

  useEffect(() => {
    setAccountFilters([]);
    setAccountPage(1);
    setAccountView("in_pool");
    setAccountFilterMenuOpen(false);
    setRefreshMenuOpen(false);
    setModelTestMenuOpen(false);
    setModelTestMenuCopied(null);
    setCopiedCredentialId(null);
    setConfigWriteError(null);
    setBatchStatus("");
  }, [activePlatform]);

  useEffect(() => () => {
    if (accountEdgeTimerRef.current != null) window.clearTimeout(accountEdgeTimerRef.current);
  }, []);

  useEffect(() => {
    if (capabilitiesQuery.isSuccess && !officialImportEnabled && createMode === "official") {
      setCreateMode("api");
    }
  }, [capabilitiesQuery.isSuccess, createMode, officialImportEnabled]);

  useEffect(() => {
    if (!accountFilterMenuOpen) {
      return;
    }
    const onPointerDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (target && accountFilterMenuRef.current && !accountFilterMenuRef.current.contains(target)) {
        setAccountFilterMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", onPointerDown);
    return () => document.removeEventListener("mousedown", onPointerDown);
  }, [accountFilterMenuOpen]);

  useEffect(() => {
    if (!refreshMenuOpen) {
      return;
    }
    const onPointerDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (target && refreshMenuRef.current && !refreshMenuRef.current.contains(target)) {
        setRefreshMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", onPointerDown);
    return () => document.removeEventListener("mousedown", onPointerDown);
  }, [refreshMenuOpen]);

  useEffect(() => {
    if (!modelTestMenuOpen) {
      return;
    }
    const onPointerDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (target && modelTestMenuRef.current && !modelTestMenuRef.current.contains(target)) {
        setModelTestMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", onPointerDown);
    return () => document.removeEventListener("mousedown", onPointerDown);
  }, [modelTestMenuOpen]);

  useEffect(() => {
    setAccountPage(1);
    setSelectedAccountIds(new Set());
    setDraggedAccountId(null);
    setDragTargetIndex(null);
    setAccountFilterMenuOpen(false);
    setRefreshMenuOpen(false);
    setModelTestMenuOpen(false);
    setModelTestMenuCopied(null);
    setBatchStatus("");
  }, [accountView]);


  const credentialsQuery = useQuery<RouteCredentialPage>({
    queryKey: [
      "route-credential-page",
      activePlatform,
      accountScope,
      accountPage,
      accountPageSize,
      accountFilters,
      poolMemberKey,
    ],
    queryFn: async () => {
      if (typeof listRouteCredentialPage === "function") {
        const page = await listRouteCredentialPage({
          platform: activePlatform,
          page: accountPage,
          page_size: accountPageSize,
          filters: accountFilters,
          pool_scope: accountScope,
        });
        if (page && Array.isArray(page.items)) {
          return page;
        }
      }
      const legacy = await listRouteCredentials(activePlatform);
      const scoped = legacy.filter((item) => {
        if (accountScope === "archived") {
          return Boolean(item.archived_at);
        }
        return accountScope === "in_pool"
          ? !item.archived_at && draftPoolIds.has(item.id)
          : !item.archived_at && !draftPoolIds.has(item.id);
      });
      const filtered = accountFilters.length
        ? scoped.filter((item) => accountFilters.includes(credentialBatchFilterKey(item)))
        : scoped;
      const start = (accountPage - 1) * accountPageSize;
      return {
        items: filtered.slice(start, start + accountPageSize),
        total: filtered.length,
        page: accountPage,
        page_count: Math.max(1, Math.ceil(filtered.length / accountPageSize)),
        page_size: accountPageSize,
        previous_page_account_id: start > 0 ? filtered[start - 1]?.id ?? null : null,
        next_page_account_id: start + accountPageSize < filtered.length ? filtered[start + accountPageSize]?.id ?? null : null,
        filter_options: Array.from(
          new Map(
            legacy.map((item) => [
              credentialBatchFilterKey(item),
              item.batch_name?.trim() || credentialBatchFilterLabel(credentialBatchFilterKey(item)),
            ]),
          ),
        ).map(([key, label]) => ({ key, label })),
        official_account_count: legacy.filter((item) => item.kind === "official").length,
      };
    },
    placeholderData: keepPreviousData,
    staleTime: 0,
    enabled: !statsOpen,
    refetchOnMount: "always",
    refetchOnWindowFocus: true,
  });

  const allCredentialsQuery = useQuery<RouteCredential[]>({
    queryKey: ["route-credentials-all", activePlatform],
    queryFn: () => listRouteCredentials(activePlatform),
    staleTime: 0,
    refetchOnMount: "always",
    refetchOnWindowFocus: true,
  });

  const routePoolQuery = useQuery({
    queryKey: ["route-pool", activePlatform, statsSince, requestPage, routeStatsPageSize],
    queryFn: () => getRoutePool(activePlatform, statsSince, requestPage, routeStatsPageSize),
    placeholderData: keepPreviousData,
    refetchInterval: statsOpen ? routeStatsRefreshMs : false,
  });
  const routeProxyQuery = useQuery({
    queryKey: ["route-proxy-status"],
    queryFn: getRouteProxyStatus,
    refetchInterval: (query) => (query.state.data?.running ? false : 1000),
  });

  useEffect(() => {
    let disposed = false;
    let activityUnsubscribe: (() => void) | undefined;
    let statusUnsubscribe: (() => void) | undefined;
    const transport = getTransport();

    void Promise.all([
      transport.subscribe<RouteCredentialActivityEvent>(
        "route-credential-activity",
        (event) => {
          if (event.platform !== activePlatform) {
            return;
          }
          queryClient.setQueriesData<RouteCredentialPage>(
            { queryKey: ["route-credential-page", activePlatform] },
            (current) => {
              if (!current) {
                return current;
              }
              return {
                ...current,
                items: current.items.map((credential) =>
                  credential.id === event.credential_id
                    ? {
                        ...credential,
                        active_request_count: event.active_request_count,
                        max_concurrency: event.max_concurrency,
                      }
                    : credential,
                ),
              };
            },
          );
          queryClient.setQueryData<RouteCredential[]>(
            ["route-credentials-all", activePlatform],
            (current) =>
              current?.map((credential) =>
                credential.id === event.credential_id
                  ? {
                      ...credential,
                      active_request_count: event.active_request_count,
                      max_concurrency: event.max_concurrency,
                    }
                  : credential,
              ),
          );
        },
      ),
      transport.subscribe<{ platform: string; credential_id: string }>(
        "route-credential-status",
        (event) => {
          if (event.platform !== activePlatform) {
            return;
          }
          void Promise.all([
            queryClient.invalidateQueries({
              queryKey: ["route-credential-page", activePlatform],
            }),
            queryClient.invalidateQueries({
              queryKey: ["route-credentials-all", activePlatform],
            }),
            queryClient.invalidateQueries({
              queryKey: ["route-pool", activePlatform],
            }),
          ]);
        },
      ),
    ])
      .then(([nextActivityUnsubscribe, nextStatusUnsubscribe]) => {
        if (disposed) {
          nextActivityUnsubscribe();
          nextStatusUnsubscribe();
        } else {
          activityUnsubscribe = nextActivityUnsubscribe;
          statusUnsubscribe = nextStatusUnsubscribe;
        }
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      activityUnsubscribe?.();
      statusUnsubscribe?.();
    };
  }, [activePlatform, queryClient]);

  // Live request log: seed recent history + tail live events while the modal is
  // open for the active platform. Subscribing bumps a backend viewer count so
  // the proxy only pushes events while someone is watching.
  useEffect(() => {
    if (!liveLogOpen) {
      return;
    }
    let disposed = false;
    let liveUnsubscribe: (() => void) | undefined;
    setLiveLogEntries([]);
    setExpandedLiveLogId(null);
    const transport = getTransport();

    void subscribeRouteProxyLiveLog(activePlatform)
      .then((history) => {
        if (!disposed) {
          setLiveLogEntries(history.slice(-200));
        }
      })
      .catch(() => undefined);

    void transport
      .subscribe<RouteProxyLiveLogEntry>("route-proxy-live-log", (event) => {
        if (event.platform !== activePlatform) {
          return;
        }
        setLiveLogEntries((current) => {
          if (current.some((entry) => entry.id === event.id)) {
            return current;
          }
          const next = [...current, event];
          return next.length > 200 ? next.slice(next.length - 200) : next;
        });
      })
      .then((unsubscribe) => {
        if (disposed) {
          unsubscribe();
        } else {
          liveUnsubscribe = unsubscribe;
        }
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      liveUnsubscribe?.();
      void unsubscribeRouteProxyLiveLog().catch(() => undefined);
    };
  }, [liveLogOpen, activePlatform]);

  useEffect(() => {
    setRequestPage(1);
    setSelectedAccountIds(new Set());
    setRoutePoolFeedback(null);
  }, [activePlatform]);

  useEffect(() => {
    setExpandedRequestId(null);
  }, [activePlatform, statsPeriod, requestPage]);

  useEffect(() => {
    if (routePoolQuery.data) {
      setDraftPoolIds(new Set(routePoolQuery.data.account_ids));
    }
  }, [routePoolQuery.data]);

  useEffect(() => {
    const nextInterfaceFormat = defaultInterfaceFormat(activePlatform);
    setOfficialText(defaultOfficialJson(activePlatform));
    setOfficialFilePaths([]);
    setApiInterfaceFormat(nextInterfaceFormat);
    setApiResponsesCustomToolCompat(false);
    setApiUserAgent("");
    setApiBaseUrl(activePlatform === "grok" ? "https://api.x.ai/v1" : "https://api.example.com/v1");
    setApiKeyField(defaultAnthropicApiKeyFieldForCreate(activePlatform));
    setApiMappings(defaultModelMappings(activePlatform));
    setApiMappingsError(null);
    setApiFetchedModels([]);
    setApiFetchModelsError(null);
    setModelTestOutcome(null);
  }, [activePlatform]);

  useEffect(() => {
    if (!editingCredential) {
      return;
    }
    setEditName(editingCredential.display_name);
    setEditEmail(editingCredential.email ?? "");
    setEditStatus(editingCredential.status);
    setEditPriority(editingCredential.route_priority ?? 3);
    setEditMaxConcurrency(String(editingCredential.max_concurrency ?? 1));
    const secret = parseJsonObject(editingCredential.secret_payload_json);
    const config = parseJsonObject(editingCredential.config_json);
    setEditSecretJson(parseJsonPreview(editingCredential.secret_payload_json, editingCredential.secret_payload_json));
    setEditConfigJson(parseJsonPreview(editingCredential.config_json, editingCredential.config_json));
    setEditUserAgent(readUserAgentFromConfig(config));
    if (editingCredential.kind === "api") {
      const interfaceFormat = interfaceFormatFromConfig(config);
      setEditApiKey(stringFromRecord(secret, "api_key"));
      setEditApiBaseUrl(stringFromRecord(config, "base_url"));
      setEditApiInterfaceFormat(interfaceFormat);
      setEditApiKeyField(anthropicApiKeyFieldFromConfig(config, "ANTHROPIC_API_KEY"));
      setEditResponsesCustomToolCompat(responsesCustomToolCompatFromConfig(config));
      setEditInlineRemoteImages(inlineRemoteImagesFromConfig(config));
      setEditApiKeyDecodeError(null);
      setEditApiKeyOcrError(null);
    } else {
      setEditApiKey("");
      setEditApiBaseUrl("");
      setEditApiInterfaceFormat("openai");
      setEditApiKeyField("ANTHROPIC_API_KEY");
      setEditResponsesCustomToolCompat(false);
      setEditInlineRemoteImages(false);
      setEditApiKeyDecodeError(null);
      setEditApiKeyOcrError(null);
    }
    setEditModelMappings(parseModelMappingsFromConfig(editingCredential.config_json));
    setEditModelMappingsError(null);
    setEditFetchedModels([]);
    setEditFetchModelsError(null);
    setEditPreviewJson(parseJsonPreview(editingCredential.preview_json, editingCredential.preview_json));
  }, [editingCredential]);

  useEffect(() => {
    if (configWriteOutcomes.length === 0) {
      return;
    }

    const timeout = window.setTimeout(() => {
      setConfigWriteOutcomes([]);
    }, 3000);

    return () => window.clearTimeout(timeout);
  }, [configWriteOutcomes]);

  useEffect(() => {
    const stats = routePoolQuery.data?.stats;
    if (!stats) {
      return;
    }
    const nextPageCount = Math.max(
      1,
      Math.ceil(stats.request_row_count / Math.max(1, stats.request_page_size)),
    );
    if (requestPage > nextPageCount) {
      setRequestPage(nextPageCount);
    }
  }, [requestPage, routePoolQuery.data?.stats]);

  const accountPageData = credentialsQuery.data;
  const credentials = accountPageData?.items ?? [];
  const modelTestCredentials = allCredentialsQuery.data ?? credentials;
  const poolModelMappingTargets = useMemo(() => {
    const targetsByAlias = new Map<string, Set<string>>();
    for (const credential of modelTestCredentials) {
      if (credential.archived_at || !draftPoolIds.has(credential.id)) {
        continue;
      }
      const mappings = expandDisplayModelMappings(
        activePlatform,
        parseModelMappingsFromConfig(credential.config_json),
      );
      for (const mapping of mappings) {
        const alias = mapping.alias.trim().toLowerCase();
        const target = mapping.target.trim();
        if (!alias || !target) {
          continue;
        }
        const targets = targetsByAlias.get(alias) ?? new Set<string>();
        targets.add(target);
        targetsByAlias.set(alias, targets);
      }
    }
    return new Map(
      Array.from(targetsByAlias.entries()).map(([alias, targets]) => [alias, Array.from(targets)]),
    );
  }, [activePlatform, draftPoolIds, modelTestCredentials]);
  const hasEligiblePoolModelTestCredential = modelTestCredentials.some(
    (credential) =>
      !credential.archived_at &&
      draftPoolIds.has(credential.id) &&
      credentialKindAllowed(modelTestRule, credential.kind),
  );
  const accountFilterOptions = useMemo(
    () => (accountPageData?.filter_options ?? []).map((option) => option.key),
    [accountPageData?.filter_options],
  );
  const accountFilterLabels = useMemo(
    () => new Map((accountPageData?.filter_options ?? []).map((option) => [option.key, option.label])),
    [accountPageData?.filter_options],
  );

  const routeStats = routePoolQuery.data?.stats;
  const costTotal = (routeStats?.cost_micros ?? 0) / 1_000_000;
  const requestRowCount = routeStats?.request_row_count ?? (routeStats?.requests ?? []).length;
  const resolvedRequestPage = routeStats?.request_page ?? requestPage;
  const resolvedRequestPageSize = routeStats?.request_page_size ?? routeStatsPageSize;
  const requestPageCount = Math.max(
    1,
    Math.ceil(requestRowCount / Math.max(1, resolvedRequestPageSize)),
  );
  const generatedEditApiPreviewJson = useMemo(() => {
    if (editingCredential?.kind !== "api") {
      return editPreviewJson;
    }
    return apiPreviewJsonWithFields(
      activePlatform,
      editSecretJson.trim() || "{}",
      editApiKey,
      editConfigJson.trim() || "{}",
      editApiBaseUrl,
      editApiInterfaceFormat,
      editModelMappings,
      editApiKeyField,
      editResponsesCustomToolCompat,
      editUserAgent,
    );
  }, [
    activePlatform,
    editApiBaseUrl,
    editApiInterfaceFormat,
    editApiKeyField,
    editApiKey,
    editConfigJson,
    editModelMappings,
    editPreviewJson,
    editResponsesCustomToolCompat,
    editSecretJson,
    editUserAgent,
    editingCredential?.kind,
  ]);

  const invalidateAccountData = async () => {
    const accountPageQueryPrefix = ["route-credential-page", activePlatform] as const;
    const allCredentialsQueryKey = ["route-credentials-all", activePlatform] as const;
    await Promise.all([
      queryClient.invalidateQueries({
        queryKey: accountPageQueryPrefix,
        refetchType: "active",
      }),
      queryClient.invalidateQueries({
        queryKey: allCredentialsQueryKey,
        refetchType: "active",
      }),
      queryClient.invalidateQueries({
        queryKey: ["route-pool", activePlatform],
        refetchType: "active",
      }),
    ]);
    // Force network refetch so status/import changes show immediately.
    await Promise.all([
      queryClient.refetchQueries({ queryKey: accountPageQueryPrefix, type: "active" }),
      queryClient.refetchQueries({ queryKey: allCredentialsQueryKey, type: "active" }),
      queryClient.refetchQueries({ queryKey: ["route-pool", activePlatform], type: "active" }),
    ]);
  };

  const mergeCredentialsIntoCache = (imported: RouteCredential[]) => {
    if (!imported.length) {
      return;
    }
    queryClient.setQueryData<RouteCredential[]>(
      ["route-credentials-all", activePlatform],
      (current) => {
        if (!current) return current;
        const updates = new Map(imported.map((item) => [item.id, item]));
        return current.map((item) => updates.get(item.id) ?? item);
      },
    );
    queryClient.setQueryData<RouteCredentialPage>(
      [
        "route-credential-page",
        activePlatform,
        accountScope,
        accountPage,
        accountPageSize,
        accountFilters,
        poolMemberKey,
      ],
      (current) => {
        if (!current) return current;
        const updates = new Map(imported.map((item) => [item.id, item]));
        return { ...current, items: current.items.map((item) => updates.get(item.id) ?? item) };
      },
    );
  };

  const openExport = () => {
    if (accountView === "stats" || selectedAccountIds.size === 0) {
      return;
    }
    setExportRequest({
      selection_context: { platform: activePlatform, pool_scope: accountScope },
      credential_ids: Array.from(selectedAccountIds),
    });
  };

  useEffect(() => {
    if (capabilitiesQuery.isLoading) {
      return;
    }
    if (!officialQuotaEnabled) {
      autoQuotaRefreshedPlatform.current = activePlatform;
      return;
    }
    if (credentialsQuery.isLoading || credentialsQuery.isFetching) {
      return;
    }
    if (autoQuotaRefreshedPlatform.current === activePlatform) {
      return;
    }
    if (!(accountPageData?.official_account_count ?? 0)) {
      autoQuotaRefreshedPlatform.current = activePlatform;
      return;
    }
    autoQuotaRefreshedPlatform.current = activePlatform;
    void refreshRouteCredentialsQuota(activePlatform)
      .then(async (outcomes: QuotaRefreshOutcome[]) => {
        const next = outcomes.map((item) => item.credential).filter((item) => item.id);
        if (next.length) {
          mergeCredentialsIntoCache(next);
          await invalidateAccountData();
        }
      })
      .catch(() => {
        // Keep page usable when vendor usage endpoints are unavailable.
      });
  }, [
    activePlatform,
    capabilitiesQuery.isLoading,
    accountPageData?.official_account_count,
    credentialsQuery.isFetching,
    credentialsQuery.isLoading,
    officialQuotaEnabled,
  ]);

  const createModelsFetchRequest = (): RouteModelsFetchRequest => {
    const firstKey = apiKeyLines(apiKey)[0] ?? "";
    if (!firstKey) {
      throw new Error("请先填写 API Key，再获取模型列表。");
    }
    if (!apiBaseUrl.trim()) {
      throw new Error("请先填写 Base URL，再获取模型列表。");
    }
    const apiKeyFieldPayload = apiKeyFieldForPayload(apiInterfaceFormat, apiKeyField);
    return {
      base_url: apiBaseUrl.trim(),
      api_key: firstKey,
      interface_format: apiInterfaceFormat,
      ...(apiKeyFieldPayload ? { api_key_field: apiKeyFieldPayload } : {}),
    };
  };

  const editModelsFetchRequest = (): RouteModelsFetchRequest => {
    const apiKeyValue = editApiKey.trim();
    const baseUrl = editApiBaseUrl.trim();
    if (!apiKeyValue) {
      throw new Error("请先填写 API Key，再获取模型列表。");
    }
    if (!baseUrl) {
      throw new Error("请先填写 Base URL，再获取模型列表。");
    }
    const apiKeyFieldPayload = apiKeyFieldForPayload(editApiInterfaceFormat, editApiKeyField);
    return {
      base_url: baseUrl,
      api_key: apiKeyValue,
      interface_format: editApiInterfaceFormat,
      ...(apiKeyFieldPayload ? { api_key_field: apiKeyFieldPayload } : {}),
    };
  };

  const apiFetchModelsMutation = useMutation({
    mutationFn: (request: RouteModelsFetchRequest) => fetchRouteModels(request),
    onMutate: () => {
      setApiFetchModelsError(null);
    },
    onSuccess: (models) => {
      setApiFetchedModels(models);
      setApiFetchModelsError(null);
    },
    onError: (error) => {
      setApiFetchModelsError(formatApiError(error, "获取模型列表失败。"));
    },
  });

  const editFetchModelsMutation = useMutation({
    mutationFn: (request: RouteModelsFetchRequest) => fetchRouteModels(request),
    onMutate: () => {
      setEditFetchModelsError(null);
    },
    onSuccess: (models) => {
      setEditFetchedModels(models);
      setEditFetchModelsError(null);
    },
    onError: (error) => {
      setEditFetchModelsError(formatApiError(error, "获取模型列表失败。"));
    },
  });

  const createMutation = useMutation({
    mutationFn: async () => {
      if (createMode === "official") {
        if (!officialImportEnabled) {
          throw new Error(officialImportReason);
        }
        const batchName = officialBatchName.trim();
        if (!batchName) {
          throw new Error("批量名称不能为空");
        }
        if (officialFilePaths.length > 0) {
          return importOfficialRouteCredentialsFromFiles({
            platform: activePlatform,
            file_paths: officialFilePaths,
            batch_name: batchName,
          });
        }
        if (!officialText.trim()) {
          throw new Error("请粘贴账号 JSON，或选择 JSON 文件导入。");
        }
        return importOfficialRouteCredentialsFromText({
          platform: activePlatform,
          text: officialText,
          batch_name: batchName,
        });
      }

      if (!apiName.trim()) {
        throw new Error("API 账号名称不能为空");
      }
      const apiKeys = apiKeyLines(apiKey);
      if (apiKeys.length === 0) {
        throw new Error("至少需要一个 API Key");
      }
      const normalizedMappings = normalizeModelMappings(apiMappings, activePlatform);
      if (normalizedMappings.error) {
        setApiMappingsError(normalizedMappings.error);
        throw new Error(normalizedMappings.error);
      }
      setApiMappingsError(null);
      const batch =
        apiKeys.length > 1
          ? await createBatch({
              name: `${apiName.trim()} 批量`,
              source: "api_route_credentials",
              notes: null,
            })
          : null;
      const imported = [];
      const selectedApiKeyField = apiKeyFieldForPayload(apiInterfaceFormat, apiKeyField);
      for (const [index, key] of apiKeys.entries()) {
        const input = {
          platform: activePlatform,
          display_name: apiKeys.length > 1 ? `${apiName.trim()} ${index + 1}` : apiName.trim(),
          api_key: key,
          base_url: apiBaseUrl,
          interface_format: apiInterfaceFormat,
          model_mappings_json: JSON.stringify(normalizedMappings.mappings),
          preview_json: apiPreviewJson.trim() || null,
          batch_id: batch?.id ?? null,
          responses_custom_tool_compat: apiResponsesCustomToolCompat,
          user_agent: apiUserAgent.trim() || null,
        };
        imported.push(
          await createApiRouteCredential(
            selectedApiKeyField ? { ...input, api_key_field: selectedApiKeyField } : input,
          ),
        );
      }
      return { imported, failed: [] };
    },
    onSuccess: async (result) => {
      setCreateOpen(false);
      const imported =
        result && typeof result === "object" && "imported" in result
          ? ((result as { imported?: RouteCredential[] }).imported ?? [])
          : [];
      if (result && typeof result === "object" && "imported" in result) {
        mergeCredentialsIntoCache(imported);
      }
      if (imported.length > 0) {
        const nextPoolIds = new Set(draftPoolIds);
        for (const credential of imported) {
          if (joinPoolOnCreate) {
            nextPoolIds.add(credential.id);
          } else {
            nextPoolIds.delete(credential.id);
          }
        }
        setDraftPoolIds(nextPoolIds);
        try {
          const state = await setRoutePoolMembers({
            platform: activePlatform,
            account_ids: Array.from(nextPoolIds),
          });
          setDraftPoolIds(new Set(state.account_ids));
          if (joinPoolOnCreate) {
            setRoutePoolFeedback({
              type: "success",
              message: `已新增 ${imported.length} 个账号并加入算力池。`,
            });
          }
        } catch (error) {
          setRoutePoolFeedback({
            type: "error",
            message: `算力池同步失败：${formatApiError(error, "请求未成功。")}`,
          });
        }
        setAccountView(joinPoolOnCreate ? "in_pool" : "out_of_pool");
      }
      await invalidateAccountData();
    },
  });

  const routePoolMutation = useMutation({
    mutationFn: ({ platform, account_ids }: RoutePoolMutationInput) =>
      setRoutePoolMembers({ platform, account_ids }),
    onMutate: () => {
      setRoutePoolFeedback(null);
    },
    onSuccess: (state, variables) => {
      setDraftPoolIds(new Set(state.account_ids));
      const message =
        variables.action === "add"
          ? `已加入 ${variables.affectedCount} 个账号。`
          : variables.action === "remove"
            ? `已移出 ${variables.affectedCount} 个账号。`
            : "算力池已同步。";
      setRoutePoolFeedback({ type: "success", message });
      void invalidateAccountData();
    },
    onError: (error) => {
      if (routePoolQuery.data) {
        setDraftPoolIds(new Set(routePoolQuery.data.account_ids));
      }
      setRoutePoolFeedback({
        type: "error",
        message: `算力池更新失败：${formatApiError(error, "请求未成功。")}`,
      });
      void invalidateAccountData();
    },
  });
  const modelTestMutation = useMutation({
    mutationFn: (request: RoutePoolModelTestRequest) => {
      const credential = request.account_id
        ? credentials.find((item) => item.id === request.account_id)
        : null;
      if (
        !modelTestEnabled ||
        (credential && !credentialKindAllowed(modelTestRule, credential.kind))
      ) {
        throw new Error(modelTestReason);
      }
      return routePoolTestModel(request);
    },
    onSuccess: (outcome) => {
      setModelTestOutcome(outcome);
      setLastRouteAccount(outcome.selected_account_name);
      queryClient.setQueryData(
        ["route-pool", activePlatform, statsSince, requestPage, routeStatsPageSize],
        {
          platform: outcome.platform,
          account_ids: routePoolQuery.data?.account_ids ?? Array.from(draftPoolIds),
          stats: outcome.stats,
        },
      );
    },
    onSettled: () => {
      setTestingAccountId(null);
      // Refresh even on OAuth refresh failures so revoked/error badges update.
      void invalidateAccountData();
    },
  });

  const quotaRefreshMutation = useMutation({
    mutationFn: (id: string) => {
      if (!officialQuotaEnabled) {
        throw new Error(officialQuotaReason);
      }
      return refreshRouteCredentialQuota(id);
    },
    onMutate: (id) => {
      setRefreshingQuotaId(id);
      setQuotaRefreshMessage(null);
    },
    onSuccess: async (outcome) => {
      mergeCredentialsIntoCache([outcome.credential]);
      await invalidateAccountData();
      if (outcome.message) {
        setQuotaRefreshMessage(outcome.message);
      } else if (outcome.updated) {
        setQuotaRefreshMessage(`已更新额度（${outcome.source}）`);
      } else {
        setQuotaRefreshMessage(outcome.source === 'none' ? '暂无可用额度数据' : '额度未变化');
      }
    },
    onError: (error) => {
      setQuotaRefreshMessage(formatApiError(error, '刷新额度失败'));
    },
    onSettled: () => {
      setRefreshingQuotaId(null);
    },
  });

  const quotaRefreshPlatformMutation = useMutation({
    mutationFn: () => {
      if (!officialQuotaEnabled) {
        throw new Error(officialQuotaReason);
      }
      return refreshRouteCredentialsQuota(activePlatform);
    },
    onMutate: () => {
      setRefreshingQuotaId('__platform__');
      setQuotaRefreshMessage(null);
    },
    onSuccess: async (outcomes) => {
      const credentials = outcomes.map((item) => item.credential).filter((item) => item.id);
      if (credentials.length) {
        mergeCredentialsIntoCache(credentials);
      }
      await invalidateAccountData();
      const updated = outcomes.filter((item) => item.updated).length;
      const failed = outcomes.filter((item) => item.source === 'error').length;
      const parts = [`官方账号 ${outcomes.length} 个`];
      if (updated) parts.push(`更新 ${updated}`);
      if (failed) parts.push(`失败 ${failed}`);
      setQuotaRefreshMessage(parts.join(' · '));
    },
    onError: (error) => {
      setQuotaRefreshMessage(formatApiError(error, '批量刷新额度失败'));
    },
    onSettled: () => {
      setRefreshingQuotaId(null);
    },
  });

  const startProxyMutation = useMutation({
    mutationFn: startRouteProxy,
    onSuccess: (status) => queryClient.setQueryData(["route-proxy-status"], status),
  });
  const stopProxyMutation = useMutation({
    mutationFn: stopRouteProxy,
    onSuccess: (status) => {
      queryClient.setQueryData(["route-proxy-status"], status);
      setConfigWriteOutcomes([]);
    },
  });
  const writeConfigsMutation = useMutation({
    mutationFn: () => {
      if (!configWriteEnabled) {
        throw new Error(configWriteReason);
      }
      return writeRouteProxyConfigs(routeProxyQuery.data?.base_url ?? null, activePlatform);
    },
    onMutate: () => setConfigWriteError(null),
    onSuccess: setConfigWriteOutcomes,
    onError: (error) => setConfigWriteError(formatApiError(error, "配置写入失败。")),
  });
  const updateMutation = useMutation({
    mutationFn: async () => {
      if (!editingCredential) {
        throw new Error("缺少账号");
      }
      const normalizedMappings = normalizeModelMappings(editModelMappings, activePlatform);
      if (editingCredential.kind === "api" && normalizedMappings.error) {
        setEditModelMappingsError(normalizedMappings.error);
        throw new Error(normalizedMappings.error);
      }
      if (editingCredential.kind === "api") {
        if (!editApiKey.trim()) {
          throw new Error("API Key 不能为空");
        }
        if (!editApiBaseUrl.trim()) {
          throw new Error("Base URL 不能为空");
        }
      }
      if (!Number.isInteger(editPriority) || editPriority < 1 || editPriority > 5) {
        throw new Error("路由优先级必须是 1-5 的整数");
      }
      const maxConcurrency = Number(editMaxConcurrency);
      if (!Number.isInteger(maxConcurrency) || maxConcurrency < 1) {
        throw new Error("最大并发数必须是大于等于 1 的整数");
      }
      setEditModelMappingsError(null);
      const nextSecretJson =
        editingCredential.kind === "api"
          ? apiSecretJsonWithKey(editSecretJson, editApiKey)
          : editSecretJson.trim() || "{}";
      const nextConfigJson =
        editingCredential.kind === "api"
          ? apiConfigJsonWithFields(
              editConfigJson.trim() || "{}",
              editApiBaseUrl,
              editApiInterfaceFormat,
              normalizedMappings.mappings,
              editApiKeyField,
              editResponsesCustomToolCompat,
              editUserAgent,
              editInlineRemoteImages,
            )
          : JSON.stringify(
              writeUserAgentToConfig(parseJsonObject(editConfigJson.trim() || "{}"), editUserAgent),
              null,
              2,
            );
      const nextPreviewJson =
        editingCredential.kind === "api"
          ? apiPreviewJsonFromPayloads(activePlatform, nextSecretJson, nextConfigJson)
          : editPreviewJson.trim() || "{}";
      return updateRouteCredential(editingCredential.id, {
        display_name: editName.trim(),
        email: editingCredential.kind === "api" ? null : editEmail.trim() || null,
        status: editStatus,
        route_priority: editPriority,
        max_concurrency: maxConcurrency,
        secret_payload_json: nextSecretJson,
        config_json: nextConfigJson,
        preview_json: nextPreviewJson,
      });
    },
    onSuccess: async () => {
      setEditingCredential(null);
      await invalidateAccountData();
    },
  });
  const deleteMutation = useMutation({
    mutationFn: deleteRouteCredential,
    onSuccess: async () => {
      setEditingCredential(null);
      await invalidateAccountData();
    },
  });
  const copyCredentialMutation = useMutation({
    mutationFn: (id: string) => copyRouteCredential(id),
    onSuccess: async (credential, sourceId) => {
      mergeCredentialsIntoCache([credential]);
      setCopiedCredentialId(sourceId);
      window.setTimeout(() => {
        setCopiedCredentialId((current) => (current === sourceId ? null : current));
      }, 1400);
      await invalidateAccountData();
    },
  });

  const reorderMutation = useMutation({
    mutationFn: async (input: Parameters<typeof reorderRouteCredentials>[0]) => {
      if (typeof reorderRouteCredentials !== "function") {
        throw new Error("账号排序功能不可用");
      }
      return reorderRouteCredentials(input);
    },
    onSuccess: async (page) => {
      queryClient.setQueryData(
        [
          "route-credential-page",
          activePlatform,
          accountScope,
          page.page,
          page.page_size,
          accountFilters,
          poolMemberKey,
        ],
        page,
      );
      setAccountPage(page.page);
      await invalidateAccountData();
    },
    onError: () => {
      void invalidateAccountData();
    },
    onSettled: () => {
      setDraggedAccountId(null);
      setDragTargetIndex(null);
    },
  });
  const routePoolModelsMutation = useMutation({
    mutationFn: async () => {
      const proxyStatus = routeProxyQuery.data;
      if (!proxyStatus?.running || !proxyStatus.base_url?.trim()) {
        throw new Error("请先启动本地路由代理，再查看算力池模型列表。");
      }
      const proxyKey = await getRouteProxyKey(activePlatform);
      return fetchRouteProxyModels(proxyStatus.base_url, proxyKey, activePlatform);
    },
  });

  const batchDeleteMutation = useMutation({
    mutationFn: async (ids: string[]) => {
      for (const id of ids) {
        await deleteRouteCredential(id);
      }
      return ids;
    },
    onSuccess: async (ids) => {
      if (editingCredential && ids.includes(editingCredential.id)) {
        setEditingCredential(null);
      }
      await invalidateAccountData();
    },
  });

  const archiveMutation = useMutation({
    mutationFn: (ids: string[]) => archiveRouteCredentials(ids),
    onSuccess: async () => {
      setSelectedAccountIds(new Set());
      await invalidateAccountData();
    },
  });
  const restoreMutation = useMutation({
    mutationFn: (ids: string[]) => restoreRouteCredentials(ids),
    onSuccess: async () => {
      setSelectedAccountIds(new Set());
      await invalidateAccountData();
    },
  });
  const batchStatusMutation = useMutation({
    mutationFn: ({ ids, status }: { ids: string[]; status: AccountStatus }) =>
      setRouteCredentialStatuses(ids, status),
    onSuccess: async () => {
      setSelectedAccountIds(new Set());
      setBatchStatus("");
      await invalidateAccountData();
    },
  });

  const toggleAccountSelection = (credentialId: string) => {
    setSelectedAccountIds((current) => {
      const next = new Set(current);
      if (next.has(credentialId)) {
        next.delete(credentialId);
      } else {
        next.add(credentialId);
      }
      return next;
    });
  };

  const clearAccountSelection = () => {
    setSelectedAccountIds(new Set());
  };

  const archiveSelectedAccounts = () => {
    if (
      selectedAccountIds.size === 0 ||
      archiveMutation.isPending ||
      restoreMutation.isPending
    ) {
      return;
    }
    archiveMutation.mutate(Array.from(selectedAccountIds));
  };

  const restoreSelectedAccounts = () => {
    if (
      selectedAccountIds.size === 0 ||
      archiveMutation.isPending ||
      restoreMutation.isPending
    ) {
      return;
    }
    restoreMutation.mutate(Array.from(selectedAccountIds));
  };

  const setSelectedAccountsStatus = () => {
    if (
      selectedAccountIds.size === 0 ||
      !batchStatus ||
      batchStatusMutation.isPending ||
      accountView === "stats"
    ) {
      return;
    }
    batchStatusMutation.mutate({
      ids: Array.from(selectedAccountIds),
      status: batchStatus,
    });
  };

  const toggleAccountFilter = (key: string) => {
    setAccountPage(1);
    setAccountFilters((current) =>
      current.includes(key) ? current.filter((item) => item !== key) : [...current, key],
    );
  };

  const removeAccountFilter = (key: string) => {
    setAccountPage(1);
    setAccountFilters((current) => current.filter((item) => item !== key));
  };

  const copyCredential = (credential: RouteCredential) => {
    if (copyCredentialMutation.isPending) {
      return;
    }
    copyCredentialMutation.mutate(credential.id);
  };

  const applyPoolMembership = (
    accountIds: string[],
    action: RoutePoolAction,
    affectedCount: number,
  ) => {
    const next = new Set(accountIds);
    setDraftPoolIds(next);
    routePoolMutation.mutate({
      platform: activePlatform,
      account_ids: Array.from(next),
      action,
      affectedCount,
    });
  };

  const commitAccountReorder = (movedId: string, targetIndex: number) => {
    if (!accountPageData || reorderMutation.isPending) return;
    const neighbors = neighborsForDrop({
      items: credentials,
      movedId,
      targetIndex,
      previousPageAccountId: accountPageData.previous_page_account_id,
      nextPageAccountId: accountPageData.next_page_account_id,
    });
    reorderMutation.mutate({
      platform: activePlatform,
      moved_account_id: movedId,
      previous_account_id: neighbors.previousAccountId,
      next_account_id: neighbors.nextAccountId,
      filters: accountFilters,
      pool_scope: accountScope,
      page_size: accountPageSize,
    });
  };

  const scheduleAccountEdgePage = (direction: -1 | 1) => {
    if (accountEdgeTimerRef.current != null || !accountPageData || !draggedAccountId) return;
    const nextPage = accountPageData.page + direction;
    if (nextPage < 1 || nextPage > accountPageData.page_count) return;
    accountEdgeTimerRef.current = window.setTimeout(() => {
      accountEdgeTimerRef.current = null;
      setAccountPage(nextPage);
    }, 600);
  };

  const addSelectedToPool = () => {
    if (selectedAccountIds.size === 0 || routePoolMutation.isPending) {
      return;
    }
    const next = new Set(draftPoolIds);
    for (const id of selectedAccountIds) {
      next.add(id);
    }
    applyPoolMembership(Array.from(next), "add", selectedAccountIds.size);
    clearAccountSelection();
  };

  const removeSelectedFromPool = () => {
    if (selectedAccountIds.size === 0 || routePoolMutation.isPending) {
      return;
    }
    const next = new Set(draftPoolIds);
    for (const id of selectedAccountIds) {
      next.delete(id);
    }
    applyPoolMembership(Array.from(next), "remove", selectedAccountIds.size);
    clearAccountSelection();
  };

  const deleteSelectedAccounts = () => {
    if (selectedAccountIds.size === 0 || batchDeleteMutation.isPending) {
      return;
    }
    const ids = Array.from(selectedAccountIds);
    const remainingPool = Array.from(draftPoolIds).filter((id) => !selectedAccountIds.has(id));
    clearAccountSelection();
    batchDeleteMutation.mutate(ids, {
      onSuccess: () => {
        if (remainingPool.length !== draftPoolIds.size) {
          applyPoolMembership(remainingPool, "sync", ids.length);
        }
      },
    });
  };

  const openRouteTestDialog = () => {
    if (!modelTestEnabled || !hasEligiblePoolModelTestCredential) {
      return;
    }
    setTestingAccountId(null);
    setModelTestAccount(null);
    setModelTestDialogOpen(true);
  };

  const copyModelTestCurl = async (shell: "posix" | "windows" = "posix") => {
    const proxyBaseUrl = routeProxyQuery.data?.base_url?.trim();
    if (!routeProxyQuery.data?.running || !proxyBaseUrl) {
      setRoutePoolFeedback({
        type: "error",
        message: "复制 curl 失败：本地路由代理尚未启动。",
      });
      return;
    }
    try {
      const proxyKey = await getRouteProxyKey(activePlatform);
      const command = modelTestCurlCommand({
        activePlatform,
        codexEndpoint: codexModelTestEndpoint,
        outcome: modelTestOutcome,
        proxyBaseUrl,
        proxyKey,
        requestedModel: routeTestModel,
        shell,
      });
      await copySensitiveText(command);
      setModelTestMenuCopied(shell === "windows" ? "curl-cmd" : "curl");
      setModelTestMenuOpen(false);
      window.setTimeout(() => setModelTestMenuCopied(null), 1400);
    } catch (error) {
      setRoutePoolFeedback({
        type: "error",
        message: "复制 curl 失败：" + formatApiError(error, "剪贴板不可用。"),
      });
    }
  };

  const copyModelTestBaseUrl = async () => {
    const baseUrl = routeProxyQuery.data?.base_url?.trim();
    if (!routeProxyQuery.data?.running || !baseUrl) {
      setRoutePoolFeedback({
        type: "error",
        message: "复制 Base URL 失败：本地路由代理尚未启动。",
      });
      return;
    }

    try {
      await copySensitiveText(baseUrl);
      setModelTestMenuCopied("base-url");
      setModelTestMenuOpen(false);
      window.setTimeout(() => setModelTestMenuCopied(null), 1400);
    } catch (error) {
      setRoutePoolFeedback({
        type: "error",
        message: "复制 Base URL 失败：" + formatApiError(error, "剪贴板不可用。"),
      });
    }
  };

  const copyModelTestSk = async () => {
    try {
      const proxyKey = await getRouteProxyKey(activePlatform);
      await copySensitiveText(proxyKey);
      setModelTestMenuCopied("sk");
      setModelTestMenuOpen(false);
      window.setTimeout(() => setModelTestMenuCopied(null), 1400);
    } catch (error) {
      setRoutePoolFeedback({
        type: "error",
        message: "复制 sk 失败：" + formatApiError(error, "无法读取本地路由密钥。"),
      });
    }
  };

  const openRoutePoolModelsDialog = () => {
    setModelTestMenuOpen(false);
    routePoolModelsMutation.reset();
    setRoutePoolModelsDialogOpen(true);
    routePoolModelsMutation.mutate();
  };

  const closeRoutePoolModelsDialog = () => {
    setRoutePoolModelsDialogOpen(false);
    routePoolModelsMutation.reset();
  };

  const openAccountTestDialog = (credential: RouteCredential) => {
    if (credential.archived_at || !credentialKindAllowed(modelTestRule, credential.kind)) {
      return;
    }
    setModelTestAccount(credential);
    setModelTestDialogOpen(true);
  };

  const selectCodexModelTestEndpoint = (endpoint: CodexModelTestEndpoint) => {
    setCodexModelTestEndpoint(endpoint);
    saveCodexModelTestEndpoint(endpoint);
  };

  const submitModelTest = () => {
    if (
      !modelTestEnabled ||
      (modelTestAccount && !credentialKindAllowed(modelTestRule, modelTestAccount.kind))
    ) {
      return;
    }
    const accountId = modelTestAccount?.id ?? null;
    setTestingAccountId(accountId);
    setModelTestOutcome(null);
    modelTestMutation.reset();
    modelTestMutation.mutate({
      platform: activePlatform,
      ...(accountId ? { account_id: accountId } : {}),
      model: routeTestModel.trim() || null,
      ...(activePlatform === "codex"
        ? { interface_format: codexModelTestInterfaceFormat(codexModelTestEndpoint) }
        : {}),
    });
    setModelTestDialogOpen(false);
  };

  const fetchApiModels = () => {
    try {
      apiFetchModelsMutation.mutate(createModelsFetchRequest());
    } catch (error) {
      setApiFetchModelsError(formatApiError(error, "获取模型列表失败。"));
    }
  };

  const fetchEditModels = () => {
    try {
      editFetchModelsMutation.mutate(editModelsFetchRequest());
    } catch (error) {
      setEditFetchModelsError(formatApiError(error, "获取模型列表失败。"));
    }
  };

  const closeModelTestOutcome = () => {
    setModelTestOutcome(null);
    modelTestMutation.reset();
  };

  const selectStatsPeriod = (period: RouteStatsPeriod) => {
    setStatsPeriod(period);
    setRequestPage(1);
  };

  const selectAccountView = (view: AccountView) => {
    if (view === accountView) {
      return;
    }
    setAccountView(view);
    if (view === "stats") {
      void routePoolQuery.refetch();
    }
  };

  const decodeApiKey = () => {
    try {
      setApiKey(
        apiKey
          .split(/\r?\n/)
          .map((line) => {
            const trimmed = line.trim();
            return trimmed ? decodeBase64Text(trimmed) : "";
          })
          .join("\n"),
      );
      setApiKeyDecodeError(null);
      setApiKeyOcrError(null);
      setApiFetchedModels([]);
      setApiFetchModelsError(null);
    } catch {
      setApiKeyDecodeError("API Key 不是有效的 Base64 字符串。");
    }
  };

  const recognizeApiKeyImage = async (blob: Blob) => {
    setApiKeyOcrRecognizing(true);
    setApiKeyDecodeError(null);
    setApiKeyOcrError(null);
    try {
      const recognized = await recognizeApiKeysFromImageBlob(blob);
      if (!recognized) {
        setApiKeyOcrError("未识别到 API Key。");
        return;
      }
      setApiKey(recognized);
      setApiFetchedModels([]);
      setApiFetchModelsError(null);
    } catch {
      setApiKeyOcrError("OCR 识别失败，请换一张更清晰的图片。");
    } finally {
      setApiKeyOcrRecognizing(false);
    }
  };

  const chooseApiKeyOcrFile = () => {
    apiKeyOcrFileInputRef.current?.click();
  };

  const runApiKeyOcr = async () => {
    setApiKeyDecodeError(null);
    setApiKeyOcrError(null);
    try {
      await recognizeApiKeyImage(await readClipboardImageBlob());
    } catch (error) {
      setApiKeyOcrError(
        error instanceof ClipboardImageReadError && error.code === "no-image"
          ? "剪切板中没有图片，请选择图片文件。"
          : "无法读取剪切板图片，请选择图片文件。",
      );
      chooseApiKeyOcrFile();
    }
  };

  const handleApiKeyOcrFileChange = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0] ?? null;
    event.target.value = "";
    if (!file) {
      return;
    }
    if (!file.type.startsWith("image/")) {
      setApiKeyOcrError("请选择图片文件。");
      return;
    }
    await recognizeApiKeyImage(file);
  };

  const decodeEditApiKey = () => {
    try {
      setEditApiKey(decodeBase64Text(editApiKey));
      setEditApiKeyDecodeError(null);
      setEditApiKeyOcrError(null);
      setEditFetchedModels([]);
      setEditFetchModelsError(null);
    } catch {
      setEditApiKeyDecodeError("API Key 不是有效的 Base64 字符串。");
    }
  };

  const recognizeEditApiKeyImage = async (blob: Blob) => {
    setEditApiKeyOcrRecognizing(true);
    setEditApiKeyDecodeError(null);
    setEditApiKeyOcrError(null);
    try {
      const recognized = await recognizeApiKeysFromImageBlob(blob);
      if (!recognized) {
        setEditApiKeyOcrError("未识别到 API Key。");
        return;
      }
      setEditApiKey(recognized);
      setEditFetchedModels([]);
      setEditFetchModelsError(null);
    } catch {
      setEditApiKeyOcrError("OCR 识别失败，请换一张更清晰的图片。");
    } finally {
      setEditApiKeyOcrRecognizing(false);
    }
  };

  const chooseEditApiKeyOcrFile = () => {
    editApiKeyOcrFileInputRef.current?.click();
  };

  const runEditApiKeyOcr = async () => {
    setEditApiKeyDecodeError(null);
    setEditApiKeyOcrError(null);
    try {
      await recognizeEditApiKeyImage(await readClipboardImageBlob());
    } catch (error) {
      setEditApiKeyOcrError(
        error instanceof ClipboardImageReadError && error.code === "no-image"
          ? "剪切板中没有图片，请选择图片文件。"
          : "无法读取剪切板图片，请选择图片文件。",
      );
      chooseEditApiKeyOcrFile();
    }
  };

  const handleEditApiKeyOcrFileChange = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0] ?? null;
    event.target.value = "";
    if (!file) {
      return;
    }
    if (!file.type.startsWith("image/")) {
      setEditApiKeyOcrError("请选择图片文件。");
      return;
    }
    await recognizeEditApiKeyImage(file);
  };

  const chooseOfficialFiles = async () => {
    const selected = await open({
      multiple: true,
      title: "选择账号 JSON 文件",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });

    if (Array.isArray(selected)) {
      setOfficialFilePaths(selected);
      return;
    }
    if (typeof selected === "string") {
      setOfficialFilePaths([selected]);
    }
  };

  const fieldClass =
    "rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100";
  const monoFieldClass = `${fieldClass} font-mono`;
  const labelClass = "grid gap-1.5 text-[12px] font-semibold text-stone-600";
  const secondaryButtonClass =
    "rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 transition-colors hover:bg-stone-50";
  const primaryButtonClass =
    "rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white shadow-sm transition-colors hover:bg-stone-800 disabled:opacity-50";
  const handleEditUserAgentChange = (next: string) => {
    setEditUserAgent(next);
    if (editingCredential?.kind === "official") {
      setEditConfigJson(
        JSON.stringify(
          writeUserAgentToConfig(parseJsonObject(editConfigJson.trim() || "{}"), next),
          null,
          2,
        ),
      );
    }
  };

  return (
    <section className="accounts-screen flex h-full min-h-0 flex-col overflow-hidden">
      <div
        className="account-workspace grid h-full min-h-0 overflow-hidden rounded-lg bg-transparent transition-[grid-template-rows] duration-300 ease-out"
        data-testid="account-workspace"
        onMouseMove={(event) => {
          if (toolbarAutoHidden && event.clientY <= 10) {
            revealToolbar();
          }
        }}
        onPointerMove={(event) => {
          if (toolbarAutoHidden && event.clientY <= 10) {
            revealToolbar();
          }
        }}
        style={{
          gridTemplateRows: toolbarAutoHidden
            ? "44px minmax(0, 1fr) 32px"
            : "60px minmax(0, 1fr) 32px",
        }}
      >
        <div
          className="relative z-30 flex h-full min-h-0 items-center justify-between gap-3 border-b border-[#d1d1d6] bg-[#f2f2f7] px-3"
          data-testid="account-workspace-toolbar"
          onFocus={revealToolbar}
          onPointerEnter={() => {
            toolbarHoveredRef.current = true;
            clearToolbarHideTimer();
            revealToolbar();
          }}
          onPointerLeave={() => {
            toolbarHoveredRef.current = false;
            if (toolbarAutoHideEligibleRef.current) {
              scheduleToolbarHide();
            }
          }}
        >
          <div
            className={`min-w-0 flex-1 transition-all duration-300 max-[599px]:hidden ${sidebarCollapsed ? "hidden" : ""} ${toolbarAutoHidden ? "pointer-events-none -translate-y-3 opacity-0" : "translate-y-0 opacity-100"}`}
            data-testid="workspace-toolbar-leading"
          >
            <div className="flex items-center gap-2">
              <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
                {platformLabels[activePlatform]}
              </p>
              {activeCapability?.support_level === "partial" ? (
                <PlatformSupportBadge
                  displayName={activeCapability.display_name}
                  supportLevel={activeCapability.support_level}
                />
              ) : null}
            </div>
            <h1 className="mt-0.5 text-lg font-semibold leading-tight tracking-tight text-stone-950">
              {sidebarCollapsed ? (
                "AI Switch"
              ) : (
                <>
                  <span className="max-[599px]:hidden">算力中心</span>
                  <span className="hidden max-[599px]:inline">AI Switch</span>
                </>
              )}
            </h1>
          </div>
          <div
            className="mx-2 flex min-w-0 max-w-[560px] flex-[1.5] items-center justify-between gap-3 rounded-lg border border-stone-300/80 bg-stone-100/75 px-2.5 py-1.5 shadow-[inset_0_1px_2px_rgba(28,25,23,0.08),inset_0_-1px_0_rgba(255,255,255,0.82)] max-[599px]:mx-0 max-[599px]:max-w-none max-[599px]:flex-1"
            data-testid="pool-status-strip"
          >
            <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
              <KeyRound aria-hidden="true" className="h-3.5 w-3.5 shrink-0 text-emerald-700" />
              <div className="min-w-0 flex-1">
                <div className="flex min-w-0 items-center gap-1.5">
                  <span className="shrink-0 text-[11px] font-semibold text-stone-800">算力池</span>
                  <span className="shrink-0 font-mono text-[10px] text-stone-500">{draftPoolIds.size} 个账号</span>
                </div>
                <div className="hidden min-w-0 items-center gap-2 truncate text-[10px] text-stone-500 sm:flex">
                  <span className="truncate" title={routeProxyQuery.data?.base_url ?? undefined}>
                    {routeProxyQuery.data?.running ? routeProxyQuery.data.base_url ?? "代理运行中" : "代理未启动"}
                  </span>
                  {lastRouteAccount ? <span className="truncate">最近：{lastRouteAccount}</span> : null}
                </div>
                <span className="sr-only">已加入 {draftPoolIds.size} 个账号</span>
                <span className="sr-only">
                  本地代理：{routeProxyQuery.data?.running ? routeProxyQuery.data.base_url ?? "运行中" : "未启动"}
                </span>
                {lastRouteAccount ? <span className="sr-only">最近路由到：{lastRouteAccount}</span> : null}
              </div>
            </div>

            <div className="flex shrink-0 items-center gap-2">
              {routeProxyQuery.data?.running ? (
                <button
                  aria-label="停止本地路由代理"
                  className="grid h-6 w-6 place-items-center border border-red-700 bg-red-600 text-white transition-colors hover:bg-red-700 disabled:opacity-50"
                  disabled={startProxyMutation.isPending || stopProxyMutation.isPending}
                  onClick={() => stopProxyMutation.mutate()}
                  title="停止本地路由代理"
                  type="button"
                >
                  <Square aria-hidden="true" className="h-3.5 w-3.5 fill-current" />
                </button>
              ) : (
                <button
                  aria-label="启动本地路由代理"
                  className="grid h-6 w-6 place-items-center border border-emerald-700 bg-emerald-600 text-white transition-colors hover:bg-emerald-700 disabled:opacity-50"
                  disabled={startProxyMutation.isPending || stopProxyMutation.isPending}
                  onClick={() => startProxyMutation.mutate()}
                  title="启动本地路由代理"
                  type="button"
                >
                  <Play aria-hidden="true" className="h-3.5 w-3.5 fill-current" />
                </button>
              )}
              <button
                aria-label="写入路由配置文件"
                className="grid h-6 w-6 place-items-center border border-stone-300 bg-white text-stone-700 transition-colors hover:bg-stone-200 disabled:opacity-50"
                disabled={!routeProxyQuery.data?.running || !configWriteEnabled || writeConfigsMutation.isPending}
                onClick={() => writeConfigsMutation.mutate()}
                title={!configWriteEnabled ? configWriteReason : undefined}
                type="button"
              >
                <FileCode2 aria-hidden="true" className="h-3.5 w-3.5" />
              </button>
              <div className="relative flex shrink-0" ref={modelTestMenuRef}>
                <button
                  aria-label="真实生成测试算力池路由"
                  className="grid h-6 w-6 place-items-center rounded-l-md border border-stone-300 border-r-0 bg-transparent text-sky-700 transition-colors hover:bg-stone-100 disabled:opacity-50"
                  disabled={!modelTestEnabled || !hasEligiblePoolModelTestCredential || modelTestMutation.isPending}
                  onClick={openRouteTestDialog}
                  title={
                    !modelTestEnabled
                      ? modelTestReason
                      : !hasEligiblePoolModelTestCredential
                        ? "当前算力池没有可测试账号"
                        : "发送测试"
                  }
                  type="button"
                >
                  <Send aria-hidden="true" className="h-3.5 w-3.5" />
                </button>
                <button
                  aria-expanded={modelTestMenuOpen}
                  aria-haspopup="menu"
                  aria-label="打开算力池测试菜单"
                  className="grid h-6 w-5 place-items-center rounded-r-md border border-stone-300 bg-transparent text-sky-700 transition-colors hover:bg-stone-100"
                  onClick={() => setModelTestMenuOpen((open) => !open)}
                  title="更多测试操作"
                  type="button"
                >
                  {modelTestMenuCopied ? (
                    <Check aria-hidden="true" className="h-3 w-3 text-emerald-600" />
                  ) : (
                    <ChevronDown
                      aria-hidden="true"
                      className={"h-3 w-3 transition-transform " + (modelTestMenuOpen ? "rotate-180" : "")}
                    />
                  )}
                </button>
                {modelTestMenuOpen ? (
                  <div
                    aria-label="算力池测试菜单"
                    className="absolute right-0 top-full z-50 mt-1 min-w-44 rounded-lg border border-stone-200 bg-white p-1 shadow-lg"
                    role="menu"
                  >
                    <button
                      aria-label="复制 curl 执行语句"
                      className="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-[12px] font-medium text-stone-700 transition-colors hover:bg-stone-100"
                      onClick={() => void copyModelTestCurl("posix")}
                      role="menuitem"
                      type="button"
                    >
                      <Copy aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
                      复制 curl（PowerShell / Git Bash）
                    </button>
                    <button
                      aria-label="复制 CMD curl 执行语句"
                      className="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-[12px] font-medium text-stone-700 transition-colors hover:bg-stone-100"
                      onClick={() => void copyModelTestCurl("windows")}
                      role="menuitem"
                      type="button"
                    >
                      <Copy aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
                      复制 CMD curl
                    </button>
                    <button
                      aria-label="复制 Base URL"
                      className="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-[12px] font-medium text-stone-700 transition-colors hover:bg-stone-100 disabled:cursor-not-allowed disabled:opacity-45"
                      disabled={!routeProxyQuery.data?.running || !routeProxyQuery.data?.base_url}
                      onClick={() => void copyModelTestBaseUrl()}
                      role="menuitem"
                      title={!routeProxyQuery.data?.running ? "本地路由代理尚未启动" : "复制当前路由代理 Base URL"}
                      type="button"
                    >
                      <Copy aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
                      复制 Base URL
                    </button>
                    <button
                      aria-label="复制 sk"
                      className="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-[12px] font-medium text-stone-700 transition-colors hover:bg-stone-100"
                      onClick={() => void copyModelTestSk()}
                      role="menuitem"
                      type="button"
                    >
                      <KeyRound aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
                      复制 sk
                    </button>
                    <button
                      aria-label="查看模型列表"
                      className="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-[12px] font-medium text-stone-700 transition-colors hover:bg-stone-100"
                      onClick={openRoutePoolModelsDialog}
                      role="menuitem"
                      type="button"
                    >
                      <List aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
                      查看模型列表
                    </button>
                    <button
                      aria-label="实时日志"
                      className="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-[12px] font-medium text-stone-700 transition-colors hover:bg-stone-100"
                      onClick={() => {
                        setModelTestMenuOpen(false);
                        setLiveLogOpen(true);
                      }}
                      role="menuitem"
                      type="button"
                    >
                      <ScrollText aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
                      实时日志
                    </button>
                  </div>
                ) : null}
              </div>
            </div>
          </div>
          <div className={`flex min-w-0 flex-1 justify-end transition-all duration-300 max-[599px]:hidden ${toolbarAutoHidden ? "pointer-events-none -translate-y-3 opacity-0" : "translate-y-0 opacity-100"}`}>
            <button
              aria-label="会话管理"
              className="grid h-7 w-7 shrink-0 place-items-center border border-stone-300 bg-white text-stone-700 transition-colors hover:bg-stone-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-stone-400"
              onClick={() => onOpenSessions?.(activePlatform)}
              title="会话管理"
              type="button"
            >
              <MessageSquareText aria-hidden="true" className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>

        <div
          className="min-h-0 overflow-y-auto overscroll-contain bg-transparent"
          data-testid="account-workspace-scroll-region"
        >
        {configWriteOutcomes.length > 0 && (
          <div className="mx-4 mb-3 space-y-1 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] text-stone-600">
            <p className="font-semibold text-stone-950">配置写入结果</p>
            {configWriteOutcomes.map((outcome) => (
              <div
                className="rounded-lg border border-stone-200 bg-white px-2.5 py-2"
                key={`${outcome.operation_id}:${outcome.target_key}:${outcome.snapshot_id ?? "none"}`}
              >
                <p>
                  {outcome.target_key} · {outcome.platform}: {outcome.path || "未解析路径"} ({outcome.status})
                </p>
                <p className="mt-1 font-mono text-[11px] text-stone-500">
                  operation {outcome.operation_id} · snapshot {outcome.snapshot_id ?? "none"}
                </p>
                <p className="mt-1 font-mono text-[11px] text-stone-500">
                  before {outcome.before_hash ?? "none"} · after {outcome.after_hash ?? "none"}
                </p>
                {outcome.error_code ? (
                  <p className="mt-1 font-mono text-[11px] text-red-600">{outcome.error_code}</p>
                ) : null}
              </div>
            ))}
          </div>
        )}
        {configWriteError ? (
          <p
            className="mx-4 mb-3 rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-[12px] font-semibold text-red-700"
            role="alert"
          >
            {configWriteError}
          </p>
        ) : null}
        {modelTestMutation.isPending ? (
          <div
            aria-label="真实生成测试进行中"
            aria-live="polite"
            className="mx-4 mb-3 mt-3 rounded-xl border border-sky-200 bg-sky-50 px-3 py-3 text-[12px] text-sky-950"
          >
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <p className="flex items-center gap-2 font-semibold">
                  <RefreshCw className="h-3.5 w-3.5 animate-spin" />
                  真实生成测试：正在请求中...
                </p>
                <p className="mt-1 text-[11px] opacity-80">
                  {(modelTestAccount?.display_name ?? "算力池路由")}
                  {routeTestModel.trim() ? ` · 模型 ${routeTestModel.trim()}` : " · 默认测试模型"}
                </p>
                <p className="mt-1 text-[11px] text-sky-800/80">
                  请求已发出，等待上游响应；完成后会显示在此区域。
                </p>
              </div>
              <span className="shrink-0 rounded-full bg-white/80 px-2 py-0.5 font-mono text-[11px] text-sky-800">
                pending
              </span>
            </div>
          </div>
        ) : null}
        {modelTestOutcome ? (
          <div
            aria-label="真实生成测试结果"
            className={`mx-4 mb-3 mt-3 space-y-3 rounded-xl border px-3 py-2 text-[12px] ${
              modelTestOutcome.success
                ? "border-emerald-200 bg-emerald-50 text-emerald-950"
                : "border-red-200 bg-red-50 text-red-950"
            }`}
          >
            <div className="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
              <div>
                <p className="font-semibold">
                  真实生成测试：{modelTestOutcome.success ? "通过" : "失败"}
                </p>
                <p className="text-[11px] opacity-80">
                  {modelTestOutcome.selected_account_name} · {interfaceFormatLabel(modelTestOutcome.interface_format)} · {modelTestTargetText(modelTestOutcome)}
                </p>
              </div>
              <div className="flex items-center gap-2">
                <p className="font-mono text-[11px]">{modelTestStatusLine(modelTestOutcome)}</p>
                <button
                  aria-label="关闭真实生成测试结果"
                  className="grid h-7 w-7 place-items-center rounded-lg text-current opacity-70 transition hover:bg-white/70 hover:opacity-100"
                  onClick={closeModelTestOutcome}
                  type="button"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>

            {modelTestOutcome.response_text ? (
              <div>
                <p className="font-semibold">模型输出</p>
                <p className="mt-1 rounded-lg bg-white/80 px-2 py-1 font-mono text-[11px] text-stone-800">
                  {modelTestOutcome.response_text}
                </p>
              </div>
            ) : null}

            {modelTestOutcome.via_route_proxy ? (
              <div
                aria-label="算力池请求链路"
                className="rounded-lg bg-white/80 px-2 py-2 text-[11px] text-stone-800"
              >
                <div className="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
                  <p className="font-semibold text-stone-700">算力池请求链路</p>
                  {modelTestOutcome.route_proxy_trace_id ? (
                    <p className="truncate font-mono text-[10px] text-stone-500">
                      trace {modelTestOutcome.route_proxy_trace_id}
                    </p>
                  ) : null}
                </div>
                <div className="mt-2 grid gap-2 lg:grid-cols-3">
                  {modelTestRouteChainItems(modelTestOutcome).map((item) => (
                    <div
                      className="min-w-0 rounded-md border border-stone-200/80 bg-white px-2 py-1.5"
                      key={item.label}
                    >
                      <p className="text-[10px] font-semibold uppercase tracking-wide text-stone-400">
                        {item.label}
                      </p>
                      <p className="mt-0.5 truncate font-mono text-[11px] text-stone-800" title={item.value}>
                        {item.value}
                      </p>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}

            {modelTestOutcome.error_message ? (
              <p className="rounded-lg bg-white/80 px-2 py-1 font-mono text-[11px] text-red-800">
                {modelTestOutcome.error_message}
              </p>
            ) : null}

            <details className="rounded-lg bg-white/80 px-2 py-1">
              <summary className="cursor-pointer font-semibold">查看输入输出</summary>
              <div className="mt-2 grid gap-2 lg:grid-cols-2">
                <div>
                  <p className="mb-1 font-semibold text-stone-600">请求 JSON</p>
                  <pre className="max-h-56 overflow-auto rounded-lg border border-stone-200 bg-white p-2 font-mono text-[11px] leading-relaxed text-stone-700">
                    {prettyJsonOrText(modelTestOutcome.request_body_json)}
                  </pre>
                </div>
                <div>
                  <p className="mb-1 font-semibold text-stone-600">响应 Body</p>
                  <pre className="max-h-56 overflow-auto rounded-lg border border-stone-200 bg-white p-2 font-mono text-[11px] leading-relaxed text-stone-700">
                    {prettyJsonOrText(modelTestOutcome.response_body)}
                  </pre>
                </div>
              </div>
            </details>
          </div>
        ) : null}
        {modelTestMutation.isError ? (
          <div className="mx-4 mb-3 flex items-start justify-between gap-3 rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-[12px] text-red-800">
            <p>
              真实生成测试失败：
              {formatApiError(
                modelTestMutation.error,
                "请检查算力池账号和网络。",
              )}
            </p>
            <button
              aria-label="关闭真实生成测试错误"
              className="grid h-7 w-7 shrink-0 place-items-center rounded-lg text-red-800 opacity-70 transition hover:bg-white/70 hover:opacity-100"
              onClick={closeModelTestOutcome}
              type="button"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        ) : null}
        {statsOpen && (
          <div className="space-y-3 border-t border-stone-200/80 px-3 py-3">
            <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
              <div>
                <p className="text-[13px] font-semibold text-stone-950">请求统计</p>
                <p className="text-[12px] text-stone-500">统计当前 {platformLabels[activePlatform]} 的历史路由请求</p>
              </div>
              <div className="grid grid-cols-4 gap-1 rounded-xl bg-stone-100 p-1">
                {routeStatsPeriods.map((period) => (
                  <button
                    className={`rounded-lg px-2.5 py-1.5 text-[12px] font-semibold transition-colors ${
                      statsPeriod === period.key
                        ? "bg-white text-stone-950 shadow-sm"
                        : "text-stone-500 hover:text-stone-900"
                    }`}
                    key={period.key}
                    onClick={() => selectStatsPeriod(period.key)}
                    type="button"
                  >
                    {period.label}
                  </button>
                ))}
              </div>
            </div>

            <div className="grid gap-2 sm:grid-cols-3 lg:grid-cols-6">
              <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
                <p className="text-[11px] font-medium text-stone-500">请求</p>
                <p className="mt-1 text-lg font-semibold text-stone-950">{routeStats?.request_count ?? 0}</p>
              </div>
              <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
                <p className="text-[11px] font-medium text-stone-500">输入 Token</p>
                <p className="mt-1 text-lg font-semibold text-stone-950">
                  {(routeStats?.input_token_count ?? 0).toLocaleString()}
                </p>
              </div>
              <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
                <p className="text-[11px] font-medium text-stone-500">输出 Token</p>
                <p className="mt-1 text-lg font-semibold text-stone-950">
                  {(routeStats?.output_token_count ?? 0).toLocaleString()}
                </p>
              </div>
              <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
                <p className="text-[11px] font-medium text-stone-500">缓存 Token</p>
                <p className="mt-1 text-lg font-semibold text-stone-950">
                  {(routeStats?.cache_token_count ?? 0).toLocaleString()}
                </p>
              </div>
              <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
                <p className="text-[11px] font-medium text-stone-500">Token 总计</p>
                <p className="mt-1 text-lg font-semibold text-stone-950">
                  {(routeStats?.token_count ?? 0).toLocaleString()}
                </p>
              </div>
              <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
                <p className="text-[11px] font-medium text-stone-500">总费用（USD）</p>
                <p className="mt-1 text-lg font-semibold text-stone-950">${costTotal.toFixed(2)}</p>
              </div>
            </div>

            <div className="overflow-hidden rounded-xl border border-stone-200 bg-white">
              <div className="flex items-center justify-between border-b border-stone-100 bg-stone-50 px-3 py-2">
                <p className="text-[12px] font-semibold text-stone-700">请求列表</p>
                <p className="text-[11px] font-medium text-stone-500">
                  {requestRowCount} 条
                </p>
              </div>
              {(routeStats?.requests ?? []).length === 0 ? (
                <p className="px-3 py-4 text-[12px] text-stone-500">当前筛选范围内暂无请求。</p>
              ) : (
                <div className="divide-y divide-stone-100">
                  {(routeStats?.requests ?? []).map((request) => {
                    const metadata = parseUsageMetadata(request.metadata_json);
                    const expanded = expandedRequestId === request.id;
                    return (
                      <div className="bg-white" data-route-request-row key={request.id}>
                        <div className="grid grid-cols-2 gap-2 px-3 py-2.5 text-[12px] text-stone-600 sm:grid-cols-4 lg:grid-cols-[1.2fr_1fr_0.5fr_1.4fr_1.4fr_0.8fr_0.8fr_0.8fr_auto] lg:items-center">
                          <span className="font-medium text-stone-800">{formatUsageTime(request.created_at)}</span>
                          <span className="truncate">{request.account_name ?? request.account_id ?? "-"}</span>
                          <span className="rounded-lg bg-stone-100 px-2 py-1 text-center font-semibold text-stone-700">
                            {metadata.status}
                          </span>
                          <span className="truncate font-mono text-[11px]">{metadata.path}</span>
                          <span className="truncate" title={metadata.model}>
                            <span className="mr-1 text-[10px] text-stone-400 lg:hidden">模型</span>
                            {metadata.model}
                          </span>
                          <span
                            aria-label={usageTokenTooltip(request)}
                            title={usageTokenTooltip(request)}
                          >
                            <span className="mr-1 text-[10px] text-stone-400 lg:hidden">Token</span>
                            {formatUsageTotalTokens(request)}
                          </span>
                          <span title="价格">
                            <span className="mr-1 text-[10px] text-stone-400 lg:hidden">价格</span>
                            {formatUsagePrice(request)}
                          </span>
                          <span className="truncate">{request.source_label}</span>
                          <button
                            aria-controls={`route-request-detail-${request.id}`}
                            aria-expanded={expanded}
                            aria-label={`${expanded ? "隐藏" : "查看"}请求 ${request.id} 详情`}
                            className="inline-flex items-center justify-center rounded-lg border border-stone-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50"
                            onClick={() => setExpandedRequestId(expanded ? null : request.id)}
                            type="button"
                          >
                            详情
                          </button>
                        </div>
                        {expanded ? <RouteRequestDetail metadata={metadata} request={request} /> : null}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </div>
        )}
      {accountView !== "stats" && (
      <section className="flex min-h-full flex-col border-t border-stone-300/80 bg-transparent pt-2">
        <div className="sticky top-0 z-20 flex flex-wrap items-center justify-between gap-2 border-y border-stone-300/80 bg-stone-100/90 px-2 py-1.5 backdrop-blur-sm">
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <span className="shrink-0 text-[12px] font-semibold text-stone-600 max-[599px]:hidden">筛选：</span>
              <div className="relative w-72 max-w-full flex-none" ref={accountFilterMenuRef}>
                <div
                  className="flex min-h-8 w-full min-w-0 flex-wrap items-center gap-1.5 rounded-md border border-stone-300 bg-white px-2 py-1"
                  onClick={() => setAccountFilterMenuOpen(true)}
                >
                  {accountFilters.length === 0 ? (
                    <span className="px-1 text-[12px] text-stone-400">选择批量名或单账号</span>
                  ) : (
                    accountFilters.map((filterKey) => (
                      <span
                        className="inline-flex items-center gap-1 rounded-md border border-blue-200 bg-blue-50 px-2 py-0.5 text-[11px] font-semibold text-blue-800"
                        key={filterKey}
                      >
                        {accountFilterLabels.get(filterKey) ?? credentialBatchFilterLabel(filterKey)}
                        <button
                          aria-label={`移除筛选 ${accountFilterLabels.get(filterKey) ?? credentialBatchFilterLabel(filterKey)}`}
                            className="p-0.5 text-blue-700 transition-colors hover:bg-blue-100"
                          onClick={(event) => {
                            event.stopPropagation();
                            removeAccountFilter(filterKey);
                          }}
                          type="button"
                        >
                          <X className="h-3 w-3" />
                        </button>
                      </span>
                    ))
                  )}
                  <button
                    aria-label="打开账号筛选"
                    className="ml-auto inline-flex items-center gap-1 px-1.5 py-1 text-[12px] font-semibold text-stone-600 transition-colors hover:bg-stone-50"
                    onClick={(event) => {
                      event.stopPropagation();
                      setAccountFilterMenuOpen((open) => !open);
                    }}
                    type="button"
                  >
                    <ChevronDown className={`h-3.5 w-3.5 transition-transform ${accountFilterMenuOpen ? "rotate-180" : ""}`} />
                  </button>
                </div>
                {accountFilterMenuOpen && (
                  <div className="absolute left-0 right-0 z-20 mt-1 max-h-56 overflow-auto rounded-md border border-stone-300 bg-white p-1 shadow-lg">
                    {accountFilterOptions.length === 0 ? (
                      <p className="px-2 py-2 text-[12px] text-stone-500">暂无可筛选项</p>
                    ) : (
                      accountFilterOptions.map((option) => {
                        const checked = accountFilters.includes(option);
                        return (
                          <button
                            aria-label={`筛选 ${accountFilterLabels.get(option) ?? credentialBatchFilterLabel(option)}`}
                            className={`flex w-full items-center justify-between rounded-sm px-2.5 py-1.5 text-left text-[12px] font-semibold transition-colors ${
                              checked ? "bg-blue-50 text-blue-800" : "text-stone-700 hover:bg-stone-50"
                            }`}
                            key={option}
                            onClick={() => toggleAccountFilter(option)}
                            type="button"
                          >
                            <span>{accountFilterLabels.get(option) ?? credentialBatchFilterLabel(option)}</span>
                            {checked ? <Check className="h-3.5 w-3.5" /> : null}
                          </button>
                        );
                      })
                    )}
                    {accountFilters.length > 0 && (
                      <button
                        aria-label="清空账号筛选"
                        className="mt-1 w-full rounded-sm border border-stone-300 px-2.5 py-1.5 text-[12px] font-semibold text-stone-600 transition-colors hover:bg-stone-50"
                        onClick={() => { setAccountFilters([]); setAccountPage(1); }}
                        type="button"
                      >
                        清空筛选
                      </button>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {selectedAccountIds.size > 0 && (
              <button
                aria-label="导出选中账号"
                className="grid h-7 w-7 place-items-center border border-stone-300 bg-white text-stone-700 transition-colors hover:bg-stone-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-stone-400"
                onClick={openExport}
                title="导出选中账号"
                type="button"
              >
                <Download aria-hidden="true" className="h-3.5 w-3.5" />
              </button>
            )}
            <button
              aria-label="新增账号"
              className="grid h-7 w-7 place-items-center border border-stone-700 bg-stone-800 text-white transition-colors hover:bg-stone-700 focus:outline-none focus-visible:ring-2 focus-visible:ring-stone-400"
              onClick={() => {
                setJoinPoolOnCreate(true);
                setCreateOpen(true);
              }}
              title="新增账号"
              type="button"
            >
              <Plus aria-hidden="true" className="h-3.5 w-3.5" />
            </button>
            <div className="relative" ref={refreshMenuRef}>
              <div className="flex overflow-hidden rounded-lg border border-stone-300 bg-white shadow-sm">
                <button
                  aria-label="刷新账号列表"
                  className="grid h-7 w-7 place-items-center bg-white text-stone-700 transition-colors hover:bg-stone-100 disabled:opacity-50"
                  disabled={credentialsQuery.isFetching || quotaRefreshPlatformMutation.isPending}
                  onClick={() => {
                    setRefreshMenuOpen(false);
                    void invalidateAccountData();
                  }}
                  title="刷新账号列表"
                  type="button"
                >
                  <RefreshCw
                    aria-hidden="true"
                    className={`h-3.5 w-3.5 ${credentialsQuery.isFetching ? "animate-spin" : ""}`}
                  />
                  <span className="sr-only">刷新账号列表</span>
                </button>
                <button
                  aria-expanded={refreshMenuOpen}
                  aria-haspopup="menu"
                  aria-label="打开刷新菜单"
                  className="grid h-7 w-6 place-items-center border-l border-stone-200 bg-white text-stone-600 transition-colors hover:bg-stone-100 disabled:opacity-50"
                  disabled={credentialsQuery.isFetching || quotaRefreshPlatformMutation.isPending}
                  onClick={() => setRefreshMenuOpen((open) => !open)}
                  title="更多刷新操作"
                  type="button"
                >
                  <ChevronDown
                    aria-hidden="true"
                    className={`h-3.5 w-3.5 transition-transform ${refreshMenuOpen ? "rotate-180" : ""}`}
                  />
                </button>
              </div>
              {refreshMenuOpen ? (
                <div
                  aria-label="刷新操作"
                  className="absolute right-0 top-full z-30 mt-1 min-w-36 overflow-hidden rounded-lg border border-stone-200 bg-white p-1 shadow-lg"
                  role="menu"
                >
                  <button
                    aria-label="刷新账号列表"
                    className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50 disabled:opacity-50"
                    disabled={credentialsQuery.isFetching || quotaRefreshPlatformMutation.isPending}
                    onClick={() => {
                      setRefreshMenuOpen(false);
                      void invalidateAccountData();
                    }}
                    role="menuitem"
                    type="button"
                  >
                    <RefreshCw aria-hidden="true" className="h-3.5 w-3.5" />
                    刷新账号列表
                  </button>
                  <button
                    aria-label="刷新官方账号额度"
                    className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-[12px] font-semibold text-violet-700 transition-colors hover:bg-violet-50 disabled:opacity-50"
                    disabled={
                      !officialQuotaEnabled ||
                      quotaRefreshPlatformMutation.isPending ||
                      credentialsQuery.isFetching
                    }
                    onClick={() => {
                      setRefreshMenuOpen(false);
                      quotaRefreshPlatformMutation.mutate();
                    }}
                    role="menuitem"
                    title={!officialQuotaEnabled ? officialQuotaReason : undefined}
                    type="button"
                  >
                    <RefreshCw
                      aria-hidden="true"
                      className={`h-3.5 w-3.5 ${quotaRefreshPlatformMutation.isPending ? "animate-spin" : ""}`}
                    />
                    刷新账号额度
                  </button>
                </div>
              ) : null}
            </div>
          </div>
        </div>

        <div className="flex min-h-0 flex-1 flex-col gap-3 p-3">
          {selectedAccountIds.size > 0 && (
            <div className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-amber-200 bg-amber-50/80 px-3 py-2">
              <div className="flex min-w-0 flex-wrap items-center gap-2 text-[12px] font-semibold text-amber-900">
                <span>已选 {selectedAccountIds.size} 个账号</span>
                <button
                  aria-label="取消账号选择"
                  className="rounded-lg border border-amber-200 bg-white px-2 py-1 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50"
                  onClick={clearAccountSelection}
                  type="button"
                >
                  取消选择
                </button>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                {accountView !== "archived" && (
                  <>
                    <select
                      aria-label="批量设置状态"
                      className="h-7 rounded-lg border border-amber-300 bg-white px-2 text-[12px] font-semibold text-stone-700"
                      onChange={(event) => setBatchStatus(event.target.value as AccountStatus | "")}
                      value={batchStatus}
                    >
                      <option value="">批量设置状态</option>
                      <option value="ok">正常</option>
                      <option value="paused">暂停</option>
                      <option value="warning">警告</option>
                      <option value="error">异常</option>
                      <option value="revoked">revoked</option>
                    </select>
                    {batchStatus && (
                      <button
                        aria-label="应用批量状态"
                        className="inline-flex items-center justify-center rounded-lg bg-amber-700 px-2.5 py-1.5 text-[12px] font-semibold text-white transition-colors hover:bg-amber-800 disabled:opacity-50"
                        disabled={batchStatusMutation.isPending}
                        onClick={setSelectedAccountsStatus}
                        type="button"
                      >
                        {batchStatusMutation.isPending ? "应用中..." : "应用状态"}
                      </button>
                    )}
                  </>
                )}
                {accountView === "archived" ? (
                  <button
                    aria-label="批量恢复账号"
                    className="grid h-7 w-7 place-items-center border border-emerald-200 bg-white text-emerald-800 transition-colors hover:bg-emerald-50 disabled:opacity-50"
                    disabled={archiveMutation.isPending || restoreMutation.isPending}
                    onClick={restoreSelectedAccounts}
                    title="批量恢复账号"
                    type="button"
                  >
                    <ArchiveRestore aria-hidden="true" className="h-3.5 w-3.5" />
                    <span className="sr-only">批量恢复账号</span>
                  </button>
                ) : (
                  <>
                    {accountView === "out_of_pool" ? (
                      <button
                        aria-label="批量加入算力池"
                        className="inline-flex items-center justify-center gap-1.5 rounded-lg bg-emerald-700 px-2.5 py-1.5 text-[12px] font-semibold text-white transition-colors hover:bg-emerald-800 disabled:opacity-50"
                        disabled={routePoolMutation.isPending}
                        onClick={addSelectedToPool}
                        type="button"
                      >
                        加入算力池
                      </button>
                    ) : (
                      <button
                        aria-label="批量移出算力池"
                        className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-emerald-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-emerald-800 transition-colors hover:bg-emerald-50 disabled:opacity-50"
                        disabled={routePoolMutation.isPending}
                        onClick={removeSelectedFromPool}
                        type="button"
                      >
                        移出算力池
                      </button>
                    )}
                    <button
                      aria-label="批量归档账号"
                      className="inline-flex h-7 items-center justify-center gap-1.5 border border-amber-200 bg-white px-2.5 text-[12px] font-semibold text-amber-800 transition-colors hover:bg-amber-50 disabled:opacity-50"
                      disabled={archiveMutation.isPending || restoreMutation.isPending}
                      onClick={archiveSelectedAccounts}
                      title="批量归档账号"
                      type="button"
                    >
                      <Archive aria-hidden="true" className="h-3.5 w-3.5" />
                      归档
                    </button>
                  </>
                )}
                <button
                  aria-label="批量删除账号"
                  className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-red-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-red-700 transition-colors hover:bg-red-50 disabled:opacity-50"
                  disabled={batchDeleteMutation.isPending}
                  onClick={deleteSelectedAccounts}
                  type="button"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  批量删除
                </button>
              </div>
            </div>
          )}
          {quotaRefreshMessage && (
            <p className="rounded-xl bg-violet-50 px-3 py-2 text-[12px] font-medium text-violet-800">
              {quotaRefreshMessage}
            </p>
          )}
          {credentialsQuery.isLoading && <p className="rounded-xl bg-stone-50 p-4 text-sm text-stone-500">正在加载账号...</p>}
          {credentialsQuery.error && <p className="rounded-xl bg-red-50 p-4 text-sm text-red-700">账号加载失败。</p>}
          {!credentialsQuery.isLoading && credentials.length === 0 && (
            <div
              className="flex min-h-0 flex-1 items-center justify-center rounded-xl border border-dashed border-stone-300 bg-stone-50 p-6 text-center text-sm text-stone-500"
              data-testid="account-empty-state"
            >
              空空如也
            </div>
          )}
          {batchStatusMutation.error && (
            <p className="rounded-xl bg-red-50 px-3 py-2 text-[12px] font-semibold text-red-700">
              {formatApiError(batchStatusMutation.error, "批量设置状态失败。")}
            </p>
          )}
          <div
            onDragOver={(event) => {
              if (!draggedAccountId) return;
              event.preventDefault();
              const rect = event.currentTarget.getBoundingClientRect();
              if (event.clientY <= rect.top + 20) scheduleAccountEdgePage(-1);
              if (event.clientY >= rect.bottom - 20) scheduleAccountEdgePage(1);
            }}
          >
          {credentials.map((credential, credentialIndex) => {
                  const subscriptionType = officialSubscriptionType(credential);
                  const primaryRemain = officialPrimaryRemain(credential);
                  const weeklyRemain = officialWeeklyRemain(credential);
                  const latestReset = officialLatestResetLabel(credential);
                  const retryLabel = credentialRetryLabel(credential);
                  const modelMappings = parseModelMappingsFromConfig(credential.config_json);
                  const baseUrlLink = credentialBaseUrlLink(credential);
                  return (
                  <div
                    aria-label={`放置在 ${credential.display_name} 前`}
                    className={`mx-1 mb-0.5 grid grid-cols-[auto_auto_minmax(0,1fr)_auto] items-center gap-2 rounded-md border px-3 py-2.5 transition-colors last:mb-0 ${
                      draggedAccountId &&
                      draggedAccountId !== credential.id &&
                      dragTargetIndex === credentialIndex
                        ? "border-blue-400 bg-blue-50/70"
                        : "border-stone-200 bg-white"
                    }`}
                    key={credential.id}
                    onDragOver={(event) => {
                      if (!draggedAccountId || draggedAccountId === credential.id) return;
                      event.preventDefault();
                      event.dataTransfer.dropEffect = "move";
                      setDragTargetIndex(credentialIndex);
                    }}
                    onDrop={(event) => {
                      event.preventDefault();
                      if (draggedAccountId && draggedAccountId !== credential.id) {
                        commitAccountReorder(draggedAccountId, credentialIndex);
                      }
                      setDraggedAccountId(null);
                      setDragTargetIndex(null);
                    }}
                  >
                    <button
                      aria-grabbed={draggedAccountId === credential.id}
                      aria-label={`拖动 ${credential.display_name}`}
                      className={`grid h-7 w-7 shrink-0 place-items-center rounded border border-stone-200 px-0 text-stone-400 hover:bg-stone-50 ${
                        draggedAccountId === credential.id ? "cursor-grabbing bg-stone-100" : "cursor-grab"
                      }`}
                      draggable
                      onDragEnd={() => { setDraggedAccountId(null); setDragTargetIndex(null); }}
                      onDragStart={(event) => {
                        event.dataTransfer.effectAllowed = "move";
                        event.dataTransfer.setData("text/plain", credential.id);
                        setDraggedAccountId(credential.id);
                        setDragTargetIndex(credentialIndex);
                      }}
                      onKeyDown={(event) => {
                        if (event.key === " " || event.key === "Enter") {
                          event.preventDefault();
                          setDraggedAccountId((current) => current === credential.id ? null : credential.id);
                        } else if (draggedAccountId === credential.id && event.key === "ArrowUp") {
                          event.preventDefault();
                          commitAccountReorder(credential.id, Math.max(0, credentialIndex - 1));
                        } else if (draggedAccountId === credential.id && event.key === "ArrowDown") {
                          event.preventDefault();
                          commitAccountReorder(credential.id, Math.min(credentials.length - 1, credentialIndex + 1));
                        } else if (event.key === "Escape") {
                          setDraggedAccountId(null);
                        }
                      }}
                      type="button"
                    >
                      <GripVertical className="h-4 w-4" />
                    </button>
                    <input
                      aria-label={`选择 ${credential.display_name}`}
                      checked={selectedAccountIds.has(credential.id)}
                      className="h-4 w-4 rounded border-stone-300 text-amber-500 focus:ring-blue-400"
                      onChange={() => toggleAccountSelection(credential.id)}
                      type="checkbox"
                    />
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <p
                          className="w-[36ch] max-w-full shrink-0 truncate text-[13px] font-semibold text-stone-950"
                          title={`P${credential.route_priority}-${credential.display_name}`}
                        >
                          <span className="text-stone-500">{`P${credential.route_priority}-`}</span>
                          <span>{credential.display_name}</span>
                        </p>
                        {baseUrlLink && (
                          <button
                            aria-label={`打开 ${baseUrlLink.host}`}
                            className="shrink-0 text-stone-400 transition-colors hover:text-blue-600"
                            onClick={(event) => {
                              event.stopPropagation();
                              window.open(baseUrlLink.href, "_blank", "noopener,noreferrer");
                            }}
                            title={baseUrlLink.href}
                            type="button"
                          >
                            <ExternalLink aria-hidden="true" className="h-3.5 w-3.5" />
                          </button>
                        )}
                        <span className="rounded-full bg-amber-50 px-2 py-0.5 text-[11px] font-semibold text-amber-800">
                          {kindLabel(credential.kind)}
                        </span>
                        {credential.archived_at && (
                          <span className="rounded-full bg-stone-200 px-2 py-0.5 text-[11px] font-semibold text-stone-700">
                            已归档
                          </span>
                        )}
                        <CredentialFailureTooltip credential={credential}>
                          <span
                            className={`rounded-full px-2 py-0.5 text-[11px] font-semibold ${accountStatusClass(credential.status)}`}
                            title={credential.status}
                          >
                            {accountStatusLabel(credential.status)}
                          </span>
                        </CredentialFailureTooltip>
                        {(credential.active_request_count ?? 0) > 0 && (
                          <span
                            aria-label={`正在处理请求，当前 ${credential.active_request_count}/${credential.max_concurrency}`}
                            className="inline-flex items-center gap-1 text-[10px] font-semibold text-emerald-700"
                            data-testid={`credential-activity-${credential.id}`}
                          >
                            <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-500" />
                            {credential.active_request_count}/{credential.max_concurrency}
                          </span>
                        )}
                        <ModelMappingSummary platform={activePlatform} mappings={modelMappings} />
                        {retryLabel && (
                          <CredentialFailureTooltip credential={credential}>
                            <span
                              className="rounded-full bg-orange-50 px-2 py-0.5 text-[11px] font-semibold text-orange-800"
                              title="临时失败退避时间"
                            >
                              冷却至 {retryLabel}
                            </span>
                          </CredentialFailureTooltip>
                        )}
                        {subscriptionType && (
                          <span
                            className="rounded-full bg-sky-50 px-2 py-0.5 text-[11px] font-semibold text-sky-800"
                            title="订阅类型"
                          >
                            订阅 {subscriptionType}
                          </span>
                        )}
                        {primaryRemain != null && (
                          <span
                            className="rounded-full bg-violet-50 px-2 py-0.5 text-[11px] font-semibold text-violet-800"
                            title="主额度剩余"
                          >
                            主额度 {primaryRemain}
                          </span>
                        )}
                        {weeklyRemain != null && (
                          <span
                            className="rounded-full bg-indigo-50 px-2 py-0.5 text-[11px] font-semibold text-indigo-800"
                            title="周额度剩余"
                          >
                            周额度 {weeklyRemain}
                          </span>
                        )}
                        {latestReset && (
                          <span
                            className="rounded-full bg-slate-100 px-2 py-0.5 text-[11px] font-semibold text-slate-700"
                            title="最近重置时间"
                          >
                            重置 {latestReset}
                          </span>
                        )}
                      </div>
                      <p className="mt-0.5 truncate text-[12px] text-stone-500">
                        {(() => {
                          const requestStats = credentialRequestStats(credential);
                          return (
                            <>
                              {requestStats.requestCount <= 0 ? (
                                <span>暂无请求</span>
                              ) : (
                                <>
                                  <span>请求 {requestStats.requestCount}</span>
                                  <span className={sidebarCollapsed ? "hidden" : "max-[599px]:hidden"}>
                                    {` · 成功 ${requestStats.successCount} · 失败 ${requestStats.failureCount}`}
                                  </span>
                                  <span>{` · 成功率 ${requestStats.rateLabel}`}</span>
                                </>
                              )}
                              {credential.batch_id && (
                                <span title={credential.batch_name?.trim() || credential.batch_id}>
                                  {` · 批量 ${credential.batch_name?.trim() || shortId(credential.batch_id)}`}
                                </span>
                              )}
                            </>
                          );
                        })()}
                      </p>
                    </div>
                    <div className="flex shrink-0 flex-nowrap items-center justify-end gap-1">
                      {credential.kind === "official" && !credential.archived_at && (
                        <button
                          aria-label={`刷新 ${credential.display_name} 额度`}
                          className="grid h-7 w-7 place-items-center border border-violet-200 text-violet-700 transition-colors hover:bg-violet-50 disabled:opacity-50"
                          disabled={
                            !officialQuotaEnabled ||
                            quotaRefreshMutation.isPending ||
                            quotaRefreshPlatformMutation.isPending
                          }
                          onClick={() => quotaRefreshMutation.mutate(credential.id)}
                          title={!officialQuotaEnabled ? officialQuotaReason : `刷新 ${credential.display_name} 额度`}
                          type="button"
                        >
                           <RefreshCw aria-hidden="true" className={`h-3.5 w-3.5 ${refreshingQuotaId === credential.id ? "animate-spin" : ""}`} />
                           <span className="sr-only">{refreshingQuotaId === credential.id ? "刷新中" : "额度"}</span>
                        </button>
                      )}
                      <button
                        aria-label={`复制 ${credential.display_name}`}
                        className="grid h-7 w-7 place-items-center border border-stone-200 text-stone-700 transition-colors hover:bg-stone-50 disabled:opacity-50"
                        disabled={copyCredentialMutation.isPending}
                        onClick={() => {
                          copyCredential(credential);
                        }}
                        title="复制账号"
                        type="button"
                      >
                        {copiedCredentialId === credential.id ? (
                          <Check aria-hidden="true" className="h-3.5 w-3.5 text-emerald-600" />
                        ) : (
                          <Copy aria-hidden="true" className="h-3.5 w-3.5" />
                        )}
                        <span className="sr-only">{copyCredentialMutation.isPending && copyCredentialMutation.variables === credential.id
                          ? "复制中"
                          : copiedCredentialId === credential.id
                            ? "已复制"
                            : "复制"}</span>
                      </button>
                      {!credential.archived_at && (
                        <button
                          aria-label={`测试 ${credential.display_name}`}
                          className="grid h-7 w-7 place-items-center border border-emerald-200 text-emerald-700 transition-colors hover:bg-emerald-50 disabled:opacity-50"
                          disabled={
                            !credentialKindAllowed(modelTestRule, credential.kind) ||
                            modelTestMutation.isPending
                          }
                          onClick={() => openAccountTestDialog(credential)}
                          title={
                            !credentialKindAllowed(modelTestRule, credential.kind)
                              ? modelTestReason
                              : "测试账号"
                          }
                          type="button"
                        >
                          <Send aria-hidden="true" className="h-3.5 w-3.5" />
                          <span className="sr-only">{testingAccountId === credential.id && modelTestMutation.isPending ? "测试中" : "测试"}</span>
                        </button>
                      )}
                      <button
                        aria-label={`编辑 ${credential.display_name}`}
                        className="grid h-7 w-7 place-items-center border border-stone-200 text-stone-700 transition-colors hover:bg-stone-50"
                        onClick={() => {
                          updateMutation.reset();
                          deleteMutation.reset();
                          setEditingCredential(credential);
                        }}
                        title="编辑账号"
                        type="button"
                      >
                        <Edit3 aria-hidden="true" className="h-3.5 w-3.5" />
                        <span className="sr-only">编辑</span>
                      </button>
                    </div>
                  </div>
                  );
                })}
          </div>
          <div data-testid="account-list-edge-bottom" className="h-1" onDragOver={(event) => { event.preventDefault(); scheduleAccountEdgePage(1); }} />
          {accountPageData && accountPageData.total > 0 && (
            <div className="hidden flex-wrap items-center justify-between gap-2 border-t border-stone-100 pt-3">
              <label className="flex items-center gap-2 text-[12px] font-semibold text-stone-600">
                <span>账号每页数量</span>
                <select
                  aria-label="账号每页数量"
                  className="rounded-lg border border-stone-200 bg-white px-2 py-1.5 text-[12px]"
                  onChange={(event) => {
                    setAccountPageSize(Number(event.target.value));
                    setAccountPage(1);
                  }}
                  value={accountPageSize}
                >
                  {[20, 50, 100].map((size) => <option key={size} value={size}>{size}</option>)}
                </select>
              </label>
              <div className="flex items-center gap-2">
                <button
                  aria-label="上一页账号"
                  className="inline-flex items-center gap-1 rounded-lg border border-stone-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold disabled:opacity-50"
                  disabled={(accountPageData.page ?? accountPage) <= 1}
                  onClick={() => setAccountPage((page) => Math.max(1, page - 1))}
                  type="button"
                ><ChevronLeft className="h-3.5 w-3.5" />上一页</button>
                <span className="min-w-20 text-center text-[12px] font-semibold text-stone-600">
                  第 {accountPageData.page} / {accountPageData.page_count} 页
                </span>
                <button
                  aria-label="下一页账号"
                  className="inline-flex items-center gap-1 rounded-lg border border-stone-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold disabled:opacity-50"
                  disabled={accountPageData.page >= accountPageData.page_count}
                  onClick={() => setAccountPage((page) => Math.min(accountPageData.page_count, page + 1))}
                  type="button"
                >下一页<ChevronRight className="h-3.5 w-3.5" /></button>
              </div>
            </div>
          )}
          {archiveMutation.error ? (
            <p className="rounded-xl bg-red-50 px-3 py-2 text-[12px] font-semibold text-red-700" role="alert">
              {formatApiError(archiveMutation.error, "归档账号失败。")}
            </p>
          ) : null}
          {restoreMutation.error ? (
            <p className="rounded-xl bg-red-50 px-3 py-2 text-[12px] font-semibold text-red-700" role="alert">
              {formatApiError(restoreMutation.error, "恢复账号失败。")}
            </p>
          ) : null}
        </div>
      </section>
      )}

        </div>
        <footer
          className="flex h-8 min-h-0 items-center justify-between gap-1 overflow-hidden border-t border-stone-300 bg-stone-100 px-2 text-[11px] text-stone-600"
          data-testid="account-workspace-status-bar"
        >
          <div aria-label="账号视图" className="flex h-7 shrink-0 items-center rounded-lg border border-stone-200 bg-white p-0.5 shadow-sm" role="group">
            {accountViewOptions.map((option) => {
              const active = accountView === option.key;
              return (
                <button
                  aria-label={option.label}
                  aria-pressed={active}
                  className={`grid h-6 place-items-center rounded-md px-1 text-[11px] font-semibold transition-colors ${active ? "bg-stone-900 text-white shadow-sm" : "text-stone-600 hover:bg-stone-100"}`}
                  key={option.key}
                  onClick={() => selectAccountView(option.key)}
                  title={option.label}
                  type="button"
                >
                  {option.label}
                </button>
              );
            })}
          </div>
          {routePoolFeedback ? (
            <span
              aria-live="polite"
              className={`min-w-0 flex-1 truncate px-2 text-[11px] ${routePoolFeedback.type === "error" ? "text-red-700" : "text-emerald-700"}`}
              role={routePoolFeedback.type === "error" ? "alert" : "status"}
            >
              {routePoolFeedback.message}
            </span>
          ) : <span className="min-w-0 flex-1" />}
          <div className="flex min-w-0 items-center gap-1">
            {statsOpen ? (
              <>
                <span className="hidden truncate sm:inline">{requestRowCount} 条请求</span>
                <button
                  aria-label="上一页请求"
                  className="grid h-6 w-6 place-items-center border border-stone-300 bg-white text-stone-700 hover:bg-stone-200 disabled:cursor-not-allowed disabled:opacity-40"
                  disabled={resolvedRequestPage <= 1}
                  onClick={() => setRequestPage((page) => Math.max(1, page - 1))}
                  title="上一页请求"
                  type="button"
                ><ChevronLeft aria-hidden="true" className="h-3.5 w-3.5" /></button>
                <span className="whitespace-nowrap font-mono text-[11px]">请求 {resolvedRequestPage}/{requestPageCount}</span>
                <button
                  aria-label="下一页请求"
                  className="grid h-6 w-6 place-items-center border border-stone-300 bg-white text-stone-700 hover:bg-stone-200 disabled:cursor-not-allowed disabled:opacity-40"
                  disabled={resolvedRequestPage >= requestPageCount}
                  onClick={() => setRequestPage((page) => Math.min(requestPageCount, page + 1))}
                  title="下一页请求"
                  type="button"
                ><ChevronRight aria-hidden="true" className="h-3.5 w-3.5" /></button>
              </>
            ) : (
              <>
                <span className="hidden truncate sm:inline">{accountPageData?.total ?? 0} 个账号</span>
                {accountPageData && accountPageData.total > 0 ? (
                  <>
                    <label className="flex items-center gap-1">
                      <span className="sr-only">账号每页数量</span>
                      <select
                        aria-label="账号每页数量"
                        className="h-6 border border-stone-300 bg-white px-1 text-[11px] text-stone-700 outline-none focus:border-stone-500"
                        onChange={(event) => {
                          setAccountPageSize(Number(event.target.value));
                          setAccountPage(1);
                        }}
                        value={accountPageSize}
                      >
                        {[20, 50, 100].map((size) => <option key={size} value={size}>{size}/页</option>)}
                      </select>
                    </label>
                    <button
                      aria-label="上一页账号"
                      className="grid h-6 w-6 place-items-center border border-stone-300 bg-white text-stone-700 hover:bg-stone-200 disabled:cursor-not-allowed disabled:opacity-40"
                      disabled={(accountPageData.page ?? accountPage) <= 1}
                      onClick={() => setAccountPage((page) => Math.max(1, page - 1))}
                      title="上一页"
                      type="button"
                    ><ChevronLeft aria-hidden="true" className="h-3.5 w-3.5" /></button>
                    <span className="whitespace-nowrap font-mono text-[11px]">{accountPageData.page}/{accountPageData.page_count}</span>
                    <button
                      aria-label="下一页账号"
                      className="grid h-6 w-6 place-items-center border border-stone-300 bg-white text-stone-700 hover:bg-stone-200 disabled:cursor-not-allowed disabled:opacity-40"
                      disabled={accountPageData.page >= accountPageData.page_count}
                      onClick={() => setAccountPage((page) => Math.min(accountPageData.page_count, page + 1))}
                      title="下一页"
                      type="button"
                    ><ChevronRight aria-hidden="true" className="h-3.5 w-3.5" /></button>
                  </>
                ) : null}
              </>
            )}
          </div>
        </footer>

        </div>

      {modelTestDialogOpen && (
        <div className="fixed inset-0 z-50 grid place-items-center bg-stone-950/35 p-4 backdrop-blur-sm">
          <div
            aria-label="真实生成测试弹窗"
            className="w-full max-w-md rounded-2xl border border-stone-200 bg-white p-4 shadow-2xl"
          >
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
                  {platformLabels[activePlatform]}
                </p>
                <h3 className="mt-0.5 text-lg font-semibold text-stone-950">
                  {modelTestAccount ? `真实生成测试 ${modelTestAccount.display_name}` : "真实生成测试算力池路由"}
                </h3>
                <p className="mt-1 text-[12px] text-stone-500">
                  会向上游发起一次真实生成请求；cc-switch 的站点可达测试仅代表 Base URL 可访问。模型可选，留空使用当前平台默认测试模型。
                </p>
              </div>
              <button
                aria-label="关闭真实生成测试弹窗"
                className="rounded-xl border border-stone-200 p-1.5 text-stone-500 transition-colors hover:bg-stone-50"
                onClick={() => setModelTestDialogOpen(false)}
                type="button"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            {activePlatform === "codex" && (
              <fieldset className="mt-4">
                <legend className="text-[12px] font-semibold text-stone-600">测试接口</legend>
                <div className="mt-1.5 grid grid-cols-2 gap-1 rounded-lg bg-stone-100 p-1">
                  {(["/responses", "/chat/completions"] as const).map((endpoint) => {
                    const selected = codexModelTestEndpoint === endpoint;
                    return (
                      <button
                        aria-label={`测试接口 ${endpoint}`}
                        aria-pressed={selected}
                        className={`h-9 min-w-0 cursor-pointer whitespace-nowrap rounded-md px-1 font-mono text-[11px] font-semibold transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400 ${
                          selected
                            ? "bg-white text-stone-950 shadow-sm"
                            : "text-stone-600 hover:text-stone-900"
                        }`}
                        key={endpoint}
                        onClick={() => selectCodexModelTestEndpoint(endpoint)}
                        type="button"
                      >
                        {endpoint}
                      </button>
                    );
                  })}
                </div>
              </fieldset>
            )}

            <label className={`${labelClass} mt-4`}>
              测试模型（可选）
              <input
                aria-label="弹窗测试模型"
                className={fieldClass}
                onChange={(event) =>
                  setRouteTestModelsByPlatform((current) => ({
                    ...current,
                    [activePlatform]: event.target.value,
                  }))
                }
                placeholder={defaultRequestedModel(activePlatform)}
                value={routeTestModel}
              />
            </label>
            {activePlatform === "claude" && (
              <p className="mt-2 rounded-xl bg-amber-50 px-3 py-2 text-[12px] font-medium text-amber-800">
                claude-opus-4-8 等内部角色模型需要在账号模型映射里指向站点真实模型；不确定时留空使用默认 Claude 测试模型。
              </p>
            )}

            <div className="mt-4 flex justify-end gap-2 border-t border-stone-100 pt-3">
              <button
                className={secondaryButtonClass}
                onClick={() => setModelTestDialogOpen(false)}
                type="button"
              >
                取消
              </button>
              <button
                aria-label="开始真实生成测试"
                className={primaryButtonClass}
                disabled={
                  modelTestMutation.isPending ||
                  !modelTestEnabled ||
                  (modelTestAccount
                    ? !credentialKindAllowed(modelTestRule, modelTestAccount.kind)
                    : !hasEligiblePoolModelTestCredential)
                }
                onClick={submitModelTest}
                title={!modelTestEnabled ? modelTestReason : undefined}
                type="button"
              >
                {modelTestMutation.isPending ? "测试中..." : "开始测试"}
              </button>
            </div>
          </div>
        </div>
      )}

      {liveLogOpen && (
        <div className="fixed inset-0 z-50 grid place-items-center bg-stone-950/35 p-4 backdrop-blur-sm">
          <div
            aria-label="实时日志弹窗"
            className="flex max-h-[85vh] w-full max-w-3xl flex-col rounded-2xl border border-stone-200 bg-white p-4 shadow-2xl"
            role="dialog"
          >
            <div className="flex items-start justify-between gap-4">
              <div>
                <h2 className="text-sm font-semibold text-stone-900">实时日志</h2>
                <p className="mt-0.5 text-[11px] text-stone-500">
                  实时显示经本机路由代理转发的请求，含协议转换的四个阶段（原始请求 / 发往上游 / 上游原始返回 / 最终返回），便于排查出错。仅当前平台，最多保留最近 200 条。
                </p>
              </div>
              <div className="flex items-center gap-2">
                <button className={secondaryButtonClass} onClick={() => setLiveLogEntries([])} type="button">
                  清空
                </button>
                <button aria-label="关闭" onClick={() => setLiveLogOpen(false)} type="button">
                  <X className="h-4 w-4 text-stone-500" />
                </button>
              </div>
            </div>
            <div className="mt-3 flex-1 overflow-auto rounded-lg border border-stone-200">
              {liveLogEntries.length === 0 ? (
                <p className="p-6 text-center text-[12px] text-stone-400">
                  暂无请求。通过算力池发起一次请求后会实时出现在这里。
                </p>
              ) : (
                <ul className="divide-y divide-stone-100">
                  {[...liveLogEntries].reverse().map((entry) => (
                    <li key={entry.id}>
                      <button
                        className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-stone-50"
                        onClick={() =>
                          setExpandedLiveLogId((current) => (current === entry.id ? null : entry.id))
                        }
                        type="button"
                      >
                        <span
                          className={`inline-flex min-w-9 justify-center rounded px-1.5 py-0.5 text-[10px] font-semibold ${entry.success ? "bg-emerald-50 text-emerald-600" : "bg-red-50 text-red-600"}`}
                        >
                          {entry.status ?? "ERR"}
                        </span>
                        <span className="font-mono text-[11px] text-stone-600">
                          {entry.requested_model ?? "?"}
                        </span>
                        {entry.bridge ? (
                          <span className="rounded bg-indigo-50 px-1.5 py-0.5 text-[10px] text-indigo-600">
                            协议转换
                          </span>
                        ) : null}
                        <span className="truncate text-[11px] text-stone-500">{entry.credential_name}</span>
                        <span className="ml-auto shrink-0 text-[10px] text-stone-400">
                          {formatUsageTime(entry.created_at)} · {entry.duration_ms}ms
                        </span>
                      </button>
                      {expandedLiveLogId === entry.id ? (
                        <div className="space-y-2 bg-stone-50 px-3 pb-3 pt-1">
                          {entry.error_message ? (
                            <p className="text-[11px] text-red-600">{entry.error_message}</p>
                          ) : null}
                          <LiveLogStage title="原始请求" body={entry.client_request} />
                          <LiveLogStage title="发往上游" body={entry.upstream_request} />
                          <LiveLogStage title="上游原始返回" body={entry.upstream_response} />
                          <LiveLogStage
                            title="最终返回"
                            body={
                              liveLogStagesIdentical(entry) ? "（与上游原始返回一致）" : entry.final_response
                            }
                          />
                          {entry.truncated ? (
                            <p className="text-[10px] text-stone-400">（部分内容已截断，每段最多 64KB）</p>
                          ) : null}
                        </div>
                      ) : null}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        </div>
      )}
      {routePoolModelsDialogOpen && (
        <div className="fixed inset-0 z-50 grid place-items-center bg-stone-950/35 p-4 backdrop-blur-sm">
          <div
            aria-label="算力池模型列表"
            className="w-full max-w-lg rounded-2xl border border-stone-200 bg-white p-4 shadow-2xl"
            role="dialog"
          >
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
                  {platformLabels[activePlatform]}
                </p>
                <h3 className="mt-0.5 text-lg font-semibold text-stone-950">算力池模型列表</h3>
                <p className="mt-1 text-[12px] text-stone-500">
                  当前列表来自本地路由代理 `/v1/models`，表示算力池对外公开的模型集合。
                </p>
              </div>
              <button
                aria-label="关闭算力池模型列表"
                className="rounded-xl border border-stone-200 p-1.5 text-stone-500 transition-colors hover:bg-stone-50"
                onClick={closeRoutePoolModelsDialog}
                type="button"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            {routePoolModelsMutation.isPending ? (
              <div className="mt-4 flex items-center gap-2 rounded-xl border border-sky-200 bg-sky-50 px-3 py-3 text-[12px] text-sky-900">
                <RefreshCw className="h-3.5 w-3.5 animate-spin" />
                正在读取模型列表...
              </div>
            ) : routePoolModelsMutation.isError ? (
              <p className="mt-4 rounded-xl border border-amber-200 bg-amber-50 px-3 py-3 text-[12px] font-medium text-amber-900" role="alert">
                {formatApiError(routePoolModelsMutation.error, "获取算力池模型列表失败。")}
              </p>
            ) : (
              <div className="mt-4">
                {routePoolModelsMutation.data && routePoolModelsMutation.data.length > 0 ? (
                  <div className="grid max-h-72 gap-1.5 overflow-y-auto rounded-xl border border-stone-200 bg-stone-50 p-2">
                    {routePoolModelsMutation.data.map((model) => {
                      const mappingTargets = poolModelMappingTargets.get(model.id.trim().toLowerCase());
                      const reasoningLevels = model.supported_reasoning_levels ?? [];
                      return (
                        <div className="flex gap-3 rounded-lg bg-white px-3 py-2" key={model.id}>
                          <div className="min-w-0 flex-1">
                            <div className="flex items-center justify-between gap-3">
                              <span
                                className="min-w-0 truncate font-mono text-[12px] font-semibold text-stone-800"
                                title={model.id}
                              >
                                {model.id}
                              </span>
                              {model.owned_by ? (
                                <span className="shrink-0 text-[11px] text-stone-500">{model.owned_by}</span>
                              ) : null}
                            </div>
                            {mappingTargets?.length ? (
                              <p className="mt-0.5 break-words text-[11px] leading-4 text-sky-700">
                                映射的上游模型：{mappingTargets.join("、")}
                              </p>
                            ) : null}
                            {reasoningLevels.length > 0 ? (
                              <p className="mt-0.5 break-words text-[11px] leading-4 text-violet-700">
                                推理等级：{reasoningLevels.map((level) => level.effort).join("、")}
                                {model.default_reasoning_level
                                  ? ` · 默认 ${model.default_reasoning_level}`
                                  : ""}
                              </p>
                            ) : null}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                ) : (
                  <p className="rounded-xl border border-dashed border-stone-300 bg-stone-50 px-3 py-4 text-center text-[12px] text-stone-500">
                    当前算力池没有可公开的模型。
                  </p>
                )}
              </div>
            )}

            <div className="mt-4 flex justify-end border-t border-stone-100 pt-3">
              <button className={secondaryButtonClass} onClick={closeRoutePoolModelsDialog} type="button">
                关闭
              </button>
            </div>
          </div>
        </div>
      )}

      {exportRequest ? (
        <RouteCredentialExportDialog
          open
          credential_ids={exportRequest.credential_ids}
          onClose={() => setExportRequest(null)}
          selection_context={exportRequest.selection_context}
        />
      ) : null}

      {createOpen && (
        <div className="fixed inset-0 z-50 grid place-items-center bg-stone-950/35 p-4 backdrop-blur-sm">
          <div className="max-h-[92vh] w-full max-w-2xl overflow-y-auto rounded-2xl border border-stone-200 bg-white p-4 shadow-2xl">
            <div className="flex items-center justify-between gap-4">
              <div>
                <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
                  {platformLabels[activePlatform]}
                </p>
                <h3 className="text-lg font-semibold text-stone-950">新增账号</h3>
              </div>
              <button
                aria-label="关闭新增账号"
                className="rounded-xl border border-stone-200 p-1.5 text-stone-500 transition-colors hover:bg-stone-50"
                onClick={() => setCreateOpen(false)}
                type="button"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="mt-4 grid gap-1 rounded-xl bg-stone-100 p-1 sm:grid-cols-2">
              {[
                ["api", "API 账号"],
                ["official", "批量导入"],
              ].map(([mode, label]) => (
                <button
                  className={`rounded-lg px-3 py-1.5 text-[13px] font-semibold transition-colors ${
                    createMode === mode ? "bg-white text-stone-950 shadow-sm" : "text-stone-500"
                  }`}
                  disabled={mode === "official" && !officialImportEnabled}
                  key={mode}
                  onClick={() => setCreateMode(mode as CreateMode)}
                  title={mode === "official" && !officialImportEnabled ? officialImportReason : undefined}
                  type="button"
                >
                  {label}
                </button>
              ))}
            </div>

            <label className="mt-3 flex items-start gap-2 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] font-medium text-stone-700">
              <input
                aria-label="创建后加入算力池"
                checked={joinPoolOnCreate}
                className="mt-0.5 h-4 w-4 rounded border-stone-300 text-amber-500 focus:ring-blue-400"
                disabled={createMutation.isPending}
                onChange={(event) => setJoinPoolOnCreate(event.target.checked)}
                type="checkbox"
              />
              <span className="grid gap-0.5">
                <span>创建后加入算力池</span>
                <span className="text-[11px] font-medium text-stone-500">
                  创建成功后自动切换到对应列表；取消则放入未入池。
                </span>
              </span>
            </label>

            {createMode === "api" && (
              <div className="mt-4 grid gap-3">
                <label className={labelClass}>
                  账号名称
                  <input
                    aria-label="API 账号名称"
                    className={fieldClass}
                    onChange={(event) => setApiName(event.target.value)}
                    value={apiName}
                  />
                </label>
                <label className={labelClass}>
                  API Key
                  <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
                    <textarea
                      aria-label="API Key"
                      className={`${monoFieldClass} min-h-24`}
                      onChange={(event) => {
                        setApiKey(event.target.value);
                        setApiKeyDecodeError(null);
                        setApiKeyOcrError(null);
                        setApiFetchedModels([]);
                        setApiFetchModelsError(null);
                      }}
                      placeholder={"每行一个 API Key；多行会自动创建为同一批量。\nsk-...\nsk-..."}
                      value={apiKey}
                    />
                    <div className="flex flex-col gap-2 sm:w-28">
                      <button
                        aria-label="Base64 解码 API Key"
                        className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] font-semibold text-stone-700 transition-colors hover:bg-white"
                        onClick={decodeApiKey}
                        type="button"
                      >
                        Base64 解码
                      </button>
                      <button
                        aria-label="OCR识别 API Key"
                        className="inline-flex items-center justify-center gap-1.5 rounded-xl border border-blue-200 bg-blue-50 px-3 py-2 text-[13px] font-semibold text-blue-700 transition-colors hover:bg-white disabled:opacity-50"
                        disabled={apiKeyOcrRecognizing}
                        onClick={runApiKeyOcr}
                        type="button"
                      >
                        <ScanText className="h-3.5 w-3.5" />
                        {apiKeyOcrRecognizing ? "识别中..." : "OCR识别"}
                      </button>
                      <input
                        accept="image/*"
                        aria-label="选择图片识别 API Key"
                        className="sr-only"
                        onChange={handleApiKeyOcrFileChange}
                        ref={apiKeyOcrFileInputRef}
                        type="file"
                      />
                    </div>
                  </div>
                  {apiKeyDecodeError && <span className="text-[12px] font-semibold text-red-700">{apiKeyDecodeError}</span>}
                  {apiKeyOcrError && <span className="text-[12px] font-semibold text-red-700">{apiKeyOcrError}</span>}
                </label>
                <label className={labelClass}>
                  Base URL
                  <input
                    aria-label="Base URL"
                    className={fieldClass}
                    onChange={(event) => {
                      setApiBaseUrl(event.target.value);
                      setApiFetchedModels([]);
                      setApiFetchModelsError(null);
                    }}
                    value={apiBaseUrl}
                  />
                </label>
                <UserAgentFields
                  fieldClass={fieldClass}
                  idPrefix="创建"
                  labelClass={labelClass}
                  onChange={setApiUserAgent}
                  value={apiUserAgent}
                />
                {shouldShowInterfaceFormatSelect(activePlatform) ? (
                  <label className={labelClass}>
                    接口格式
                    <select
                      aria-label="接口格式"
                      className={fieldClass}
                      onChange={(event) => {
                        setApiInterfaceFormat(event.target.value as InterfaceFormat);
                        setApiFetchedModels([]);
                        setApiFetchModelsError(null);
                      }}
                      value={apiInterfaceFormat}
                    >
                      {interfaceFormatsForPlatform(activePlatform).map((format) => (
                        <option key={format} value={format}>
                          {interfaceFormatLabel(format)}
                        </option>
                      ))}
                    </select>
                  </label>
                ) : null}
                {isAnthropicInterfaceFormat(apiInterfaceFormat) ? (
                  <label className={labelClass}>
                    Claude 鉴权字段
                    <select
                      aria-label="Claude 鉴权字段"
                      className={fieldClass}
                      onChange={(event) => {
                        setApiKeyField(event.target.value as AnthropicApiKeyField);
                        setApiFetchedModels([]);
                        setApiFetchModelsError(null);
                      }}
                      value={apiKeyField}
                    >
                      {anthropicApiKeyFields.map((field) => (
                        <option key={field.value} value={field.value}>
                          {field.label}
                        </option>
                      ))}
                    </select>
                    <span className="text-[11px] font-medium text-stone-500">
                      {anthropicApiKeyFieldDescription(apiKeyField)}
                    </span>
                  </label>
                ) : null}
                {shouldShowResponsesCustomToolCompatForFormat(activePlatform, apiInterfaceFormat) ? (
                  <label className="flex items-start gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[12px] font-medium text-stone-700">
                    <input
                      aria-label="兼容 custom 工具（Responses 中转）"
                      checked={apiResponsesCustomToolCompat}
                      className="mt-0.5"
                      onChange={(event) => setApiResponsesCustomToolCompat(event.target.checked)}
                      type="checkbox"
                    />
                    <span className="grid gap-1">
                      <span>兼容 custom 工具（Responses 中转）</span>
                      <span className="text-[11px] font-medium text-stone-500">
                        仅当上游为 Responses 中转且不支持 custom 工具时勾选，把 custom 改写成 function。Chat/Anthropic/Gemini 上游会自动处理，无需勾选。
                      </span>
                    </span>
                  </label>
                ) : null}
                <ModelMappingsEditor
                  error={apiMappingsError}
                  fetchError={apiFetchModelsError}
                  fetchedModels={apiFetchedModels}
                  interfaceFormat={apiInterfaceFormat}
                  isFetchingModels={apiFetchModelsMutation.isPending}
                  label="模型映射"
                  onChange={(next) => {
                    setApiMappings(next);
                    setApiMappingsError(null);
                  }}
                  onFetchModels={fetchApiModels}
                  platform={activePlatform}
                  value={apiMappings}
                />
                <label className={labelClass}>
                  预览 JSON（可选）
                  <textarea
                    aria-label="预览 JSON"
                    className={`${monoFieldClass} min-h-20`}
                    onChange={(event) => setApiPreviewJson(event.target.value)}
                    value={apiPreviewJson}
                  />
                </label>
              </div>
            )}

            {createMode === "official" && (
              <div className="mt-4 grid gap-3">
                <p className="text-[13px] leading-5 text-stone-600">
                  粘贴 OAuth CPA、API Key CPA、session JSON、auth.json、Sub2API JSON、accessToken 或 refresh_token。
                </p>
                <details className="overflow-hidden rounded-xl border border-stone-200 bg-white">
                  <summary className="flex cursor-pointer list-none items-center gap-2 border-b border-stone-100 px-3 py-2 text-[12px] font-semibold text-stone-700">
                    <ChevronDown className="h-3.5 w-3.5" />
                    必填字段与示例（点击展开）
                  </summary>
                  <div className="space-y-3 p-3 text-[12px] text-stone-600">
                    <p>支持 OAuth CPA、API Key CPA（api-key / api-key-entries）、完整 tokens（id_token + access_token）、Sub2API 导出 JSON、仅 accessToken 或仅 refresh_token。</p>
                    <div>
                      <p className="mb-1 font-semibold text-stone-500">完整 tokens 示例</p>
                      <pre className="overflow-auto rounded-xl border border-stone-200 bg-slate-100 p-3 font-mono text-[12px] leading-5 text-slate-900">{`{
  "tokens": {
    "id_token": "eyJ...",
    "access_token": "eyJ...",
    "refresh_token": "rt_..."
  }
}`}</pre>
                    </div>
                    <div>
                      <p className="mb-1 font-semibold text-stone-500">session / accessToken / refresh_token 示例</p>
                      <pre className="overflow-auto rounded-xl border border-stone-200 bg-slate-100 p-3 font-mono text-[12px] leading-5 text-slate-900">{`{
  "user": {
    "email": "user@example.com"
  },
  "account": {
    "id": "account-id"
  },
  "accessToken": "eyJ...",
  "authProvider": "openai"
}

{
  "refresh_token": "rt_..."
}`}</pre>
                    </div>
                    <div>
                      <p className="mb-1 font-semibold text-stone-500">批量示例</p>
                      <pre className="overflow-auto rounded-xl border border-stone-200 bg-slate-100 p-3 font-mono text-[12px] leading-5 text-slate-900">{`[
  {
    "id": "codex_demo_1",
    "email": "user@example.com",
    "tokens": {
      "id_token": "eyJ...",
      "access_token": "eyJ...",
      "refresh_token": "rt_..."
    },
    "created_at": 1730000000,
    "last_used": 1730000000
  }
]`}</pre>
                    </div>
                  </div>
                </details>

                <label className={labelClass}>
                  名称
                  <input
                    aria-label="导入批量名称"
                    className={fieldClass}
                    onChange={(event) => setOfficialBatchName(event.target.value)}
                    placeholder="必填，用于标记本次批量导入"
                    required
                    value={officialBatchName}
                  />
                </label>
                <label className={labelClass}>
                  账号 JSON
                  <textarea
                    aria-label="账号 JSON"
                    className={`${monoFieldClass} min-h-32`}
                    onChange={(event) => {
                      setOfficialText(event.target.value);
                      if (event.target.value.trim()) {
                        setOfficialFilePaths([]);
                      }
                    }}
                    placeholder={'示例：直接粘贴 session JSON、accessToken、Sub2API 导出 JSON，或 {"accessToken":"eyJ..."}'}
                    value={officialText}
                  />
                </label>
                <div className="grid gap-2">
                  <button
                    aria-label="导入 JSON 文件"
                    className="inline-flex items-center justify-center gap-1.5 rounded-xl border border-blue-200 bg-blue-50 px-3 py-2 text-[13px] font-semibold text-blue-900 transition-colors hover:bg-blue-100"
                    onClick={() => void chooseOfficialFiles()}
                    type="button"
                  >
                    <FileCode2 className="h-3.5 w-3.5" />
                    导入 JSON 文件
                  </button>
                  {officialFilePaths.length > 0 && (
                    <div className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] text-stone-600">
                      已选择 {officialFilePaths.length} 个文件
                    </div>
                  )}
                </div>
              </div>
            )}

            {createMutation.error && (
              <p className="mt-4 rounded-xl bg-red-50 p-3 text-[13px] font-semibold text-red-700">
                {formatApiError(createMutation.error, "新增账号失败。")}
              </p>
            )}

            <div className="mt-4 flex justify-end gap-2 border-t border-stone-100 pt-3">
              <button
                className={secondaryButtonClass}
                onClick={() => setCreateOpen(false)}
                type="button"
              >
                取消
              </button>
              <button
                className={primaryButtonClass}
                disabled={createMutation.isPending || (createMode === "official" && !officialImportEnabled)}
                onClick={() => createMutation.mutate()}
                type="button"
              >
                {createMutation.isPending ? "正在保存..." : "保存账号"}
              </button>
            </div>
          </div>
        </div>
      )}

      {editingCredential && (
        <div className="fixed inset-0 z-50 flex justify-end bg-stone-950/28 backdrop-blur-sm">
          <aside className="m-3 h-[calc(100%-1.5rem)] w-full max-w-2xl overflow-y-auto rounded-2xl border border-stone-200 bg-white p-4 shadow-2xl">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
                  {kindLabel(editingCredential.kind)} Account
                </p>
                <h3 className="mt-0.5 text-lg font-semibold text-stone-950">{editingCredential.display_name}</h3>
                <p className="mt-1 text-[12px] text-stone-500">{editingCredential.id}</p>
              </div>
              <button
                aria-label="关闭编辑账号"
                className="rounded-xl border border-stone-200 p-1.5 text-stone-500 transition-colors hover:bg-stone-50"
                onClick={() => setEditingCredential(null)}
                type="button"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="mt-4 grid gap-3">
              <label className={labelClass}>
                账号名称
                <input
                  aria-label="编辑账号名称"
                  className={fieldClass}
                  onChange={(event) => setEditName(event.target.value)}
                  value={editName}
                />
              </label>
              {editingCredential.kind === "official" && (
                <label className={labelClass}>
                  邮箱
                  <input
                    aria-label="编辑邮箱"
                    className={fieldClass}
                    onChange={(event) => setEditEmail(event.target.value)}
                    value={editEmail}
                  />
                </label>
              )}
              <label className={labelClass}>
                状态
                <select
                  aria-label="编辑状态"
                  className={fieldClass}
                  onChange={(event) => setEditStatus(event.target.value as AccountStatus)}
                  value={editStatus}
                >
                  <option value="ok">正常 (ok)</option>
                  <option value="warning">警告 (warning)</option>
                  <option value="error">异常 (error)</option>
                  <option value="revoked">revoked</option>
                  <option value="paused">暂停 (paused)</option>
                </select>
              </label>
              <div className="grid gap-3 sm:grid-cols-2">
                <label className={labelClass}>
                  路由优先级
                  <select
                    aria-label="编辑路由优先级"
                    className={fieldClass}
                    onChange={(event) => setEditPriority(Number(event.target.value))}
                    value={editPriority}
                  >
                    {[1, 2, 3, 4, 5].map((priority) => (
                      <option key={priority} value={priority}>
                        {priority}（数字越小优先级越高）
                      </option>
                    ))}
                  </select>
                </label>
                <label className={labelClass}>
                  最大并发数
                  <input
                    aria-label="编辑最大并发数"
                    className={fieldClass}
                    min={1}
                    onChange={(event) => setEditMaxConcurrency(event.target.value)}
                    step={1}
                    type="number"
                    value={editMaxConcurrency}
                  />
                </label>
              </div>
              {editingCredential.kind === "official" && (
                <UserAgentFields
                  fieldClass={fieldClass}
                  idPrefix="编辑"
                  labelClass={labelClass}
                  onChange={handleEditUserAgentChange}
                  value={editUserAgent}
                />
              )}
              {editingCredential.kind === "api" ? (
                <>
                  <label className={labelClass}>
                    API Key
                    <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
                      <input
                        aria-label="编辑 API Key"
                        className={fieldClass}
                        onChange={(event) => {
                          setEditApiKey(event.target.value);
                          setEditApiKeyDecodeError(null);
                          setEditApiKeyOcrError(null);
                          setEditFetchedModels([]);
                          setEditFetchModelsError(null);
                        }}
                        value={editApiKey}
                      />
                      <div className="flex gap-2 sm:w-52">
                        <button
                          aria-label="编辑 Base64 解码 API Key"
                          className="flex-1 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] font-semibold text-stone-700 transition-colors hover:bg-white"
                          onClick={decodeEditApiKey}
                          type="button"
                        >
                          Base64
                        </button>
                        <button
                          aria-label="编辑 OCR识别 API Key"
                          className="inline-flex flex-1 items-center justify-center gap-1.5 rounded-xl border border-blue-200 bg-blue-50 px-3 py-2 text-[13px] font-semibold text-blue-700 transition-colors hover:bg-white disabled:opacity-50"
                          disabled={editApiKeyOcrRecognizing}
                          onClick={runEditApiKeyOcr}
                          type="button"
                        >
                          <ScanText className="h-3.5 w-3.5" />
                          {editApiKeyOcrRecognizing ? "识别中" : "OCR"}
                        </button>
                        <input
                          accept="image/*"
                          aria-label="选择图片识别编辑 API Key"
                          className="sr-only"
                          onChange={handleEditApiKeyOcrFileChange}
                          ref={editApiKeyOcrFileInputRef}
                          type="file"
                        />
                      </div>
                    </div>
                    {editApiKeyDecodeError && <span className="text-[12px] font-semibold text-red-700">{editApiKeyDecodeError}</span>}
                    {editApiKeyOcrError && <span className="text-[12px] font-semibold text-red-700">{editApiKeyOcrError}</span>}
                  </label>
                  <label className={labelClass}>
                    Base URL
                    <input
                      aria-label="编辑 Base URL"
                      className={fieldClass}
                      onChange={(event) => {
                        setEditApiBaseUrl(event.target.value);
                        setEditFetchedModels([]);
                        setEditFetchModelsError(null);
                      }}
                      value={editApiBaseUrl}
                    />
                  </label>
                  <UserAgentFields
                    fieldClass={fieldClass}
                    idPrefix="编辑"
                    labelClass={labelClass}
                    onChange={handleEditUserAgentChange}
                    value={editUserAgent}
                  />
                  {shouldShowInterfaceFormatSelect(activePlatform) ? (
                    <label className={labelClass}>
                      接口格式
                      <select
                        aria-label="编辑接口格式"
                        className={fieldClass}
                        onChange={(event) => {
                          setEditApiInterfaceFormat(event.target.value as InterfaceFormat);
                          setEditFetchedModels([]);
                          setEditFetchModelsError(null);
                        }}
                        value={editApiInterfaceFormat}
                      >
                        {interfaceFormatsForPlatform(activePlatform).map((format) => (
                          <option key={format} value={format}>
                            {interfaceFormatLabel(format)}
                          </option>
                        ))}
                      </select>
                    </label>
                  ) : null}
                  {isAnthropicInterfaceFormat(editApiInterfaceFormat) ? (
                    <label className={labelClass}>
                      Claude 鉴权字段
                      <select
                        aria-label="编辑 Claude 鉴权字段"
                        className={fieldClass}
                        onChange={(event) => {
                          setEditApiKeyField(event.target.value as AnthropicApiKeyField);
                          setEditFetchedModels([]);
                          setEditFetchModelsError(null);
                        }}
                        value={editApiKeyField}
                      >
                        {anthropicApiKeyFields.map((field) => (
                          <option key={field.value} value={field.value}>
                            {field.label}
                          </option>
                        ))}
                      </select>
                      <span className="text-[11px] font-medium text-stone-500">
                        {anthropicApiKeyFieldDescription(editApiKeyField)}
                      </span>
                    </label>
                  ) : null}
                  {shouldShowResponsesCustomToolCompatForFormat(activePlatform, editApiInterfaceFormat) ? (
                    <label className="flex items-start gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[12px] font-medium text-stone-700">
                      <input
                        aria-label="兼容 custom 工具（Responses 中转）"
                        checked={editResponsesCustomToolCompat}
                        className="mt-0.5"
                        onChange={(event) => setEditResponsesCustomToolCompat(event.target.checked)}
                        type="checkbox"
                      />
                      <span className="grid gap-1">
                        <span>兼容 custom 工具（Responses 中转）</span>
                        <span className="text-[11px] font-medium text-stone-500">
                          仅当上游为 Responses 中转且不支持 custom 工具时勾选，把 custom 改写成 function。Chat/Anthropic/Gemini 上游会自动处理，无需勾选。
                        </span>
                      </span>
                    </label>
                  ) : null}
                  <label className="flex items-start gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[12px] font-medium text-stone-700">
                    <input
                      aria-label="内联远程图片"
                      checked={editInlineRemoteImages}
                      className="mt-0.5"
                      onChange={(event) => setEditInlineRemoteImages(event.target.checked)}
                      type="checkbox"
                    />
                    <span className="grid gap-1">
                      <span>内联远程图片</span>
                      <span className="text-[11px] font-medium text-stone-500">
                        转发前把请求里的 http(s) 图片链接抓取并转成 base64 data URL。用于上游会抓取图片链接、且因图床返回非 image/* 类型而报错的情况（如 OSS 对象被当作 text/plain）。会增加延迟，仅在需要时勾选。
                      </span>
                    </span>
                  </label>
                  <ModelMappingsEditor
                    error={editModelMappingsError}
                    fetchError={editFetchModelsError}
                    fetchedModels={editFetchedModels}
                    interfaceFormat={editApiInterfaceFormat}
                    isFetchingModels={editFetchModelsMutation.isPending}
                    label="模型映射"
                    onChange={(next) => {
                      setEditModelMappings(next);
                      setEditModelMappingsError(null);
                    }}
                    onFetchModels={fetchEditModels}
                    platform={activePlatform}
                    value={editModelMappings}
                  />
                </>
              ) : (
                <>
                  <label className={labelClass}>
                    Secret JSON
                    <textarea
                      aria-label="编辑 Secret JSON"
                      className={`${monoFieldClass} min-h-24`}
                      onChange={(event) => {
                        setEditSecretJson(event.target.value);
                        setEditFetchedModels([]);
                        setEditFetchModelsError(null);
                      }}
                      value={editSecretJson}
                    />
                  </label>
                  <label className={labelClass}>
                    Config JSON
                    <textarea
                      aria-label="编辑 Config JSON"
                      className={`${monoFieldClass} min-h-24`}
                      onChange={(event) => {
                        const nextConfigJson = event.target.value;
                        setEditConfigJson(nextConfigJson);
                        setEditUserAgent(readUserAgentFromConfig(parseJsonObject(nextConfigJson)));
                        setEditModelMappings(parseModelMappingsFromConfig(nextConfigJson));
                        setEditModelMappingsError(null);
                        setEditFetchedModels([]);
                        setEditFetchModelsError(null);
                      }}
                      value={editConfigJson}
                    />
                  </label>
                </>
              )}
              <label className={labelClass}>
                Preview JSON
                <textarea
                  aria-label="编辑 Preview JSON"
                  className={`${monoFieldClass} min-h-24`}
                  onChange={(event) => setEditPreviewJson(event.target.value)}
                  readOnly={editingCredential.kind === "api"}
                  value={editingCredential.kind === "api" ? generatedEditApiPreviewJson : editPreviewJson}
                />
                {editingCredential.kind === "api" && (
                  <span className="text-[11px] font-medium text-stone-500">
                    API 账号预览会根据 API Key、Base URL、接口格式和模型映射自动同步。
                  </span>
                )}
              </label>
            </div>

            {updateMutation.error && (
              <p className="mt-4 rounded-xl bg-red-50 p-3 text-[13px] font-semibold text-red-700">保存账号失败。</p>
            )}
            {deleteMutation.error && (
              <p className="mt-4 rounded-xl bg-red-50 p-3 text-[13px] font-semibold text-red-700">删除账号失败。</p>
            )}

            <div className="mt-4 flex flex-wrap justify-between gap-2 border-t border-stone-100 pt-3">
              <button
                className="inline-flex items-center gap-1.5 rounded-xl border border-red-200 bg-white px-3 py-2 text-[13px] font-semibold text-red-700 transition-colors hover:bg-red-50"
                disabled={deleteMutation.isPending}
                onClick={() => deleteMutation.mutate(editingCredential.id)}
                type="button"
              >
                <Trash2 className="h-3.5 w-3.5" />
                删除账号
              </button>
              <div className="flex gap-2">
                <button
                  className={secondaryButtonClass}
                  onClick={() => setEditingCredential(null)}
                  type="button"
                >
                  取消
                </button>
                <button
                  className={primaryButtonClass}
                  disabled={updateMutation.isPending}
                  onClick={() => updateMutation.mutate()}
                  type="button"
                >
                  {updateMutation.isPending ? "正在保存..." : "保存修改"}
                </button>
              </div>
            </div>
          </aside>
        </div>
      )}
    </section>
  );
}
