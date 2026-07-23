import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowRight,
  BarChart3,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Edit3,
  FileCode2,
  KeyRound,
  MessageSquareText,
  Play,
  Plus,
  Power,
  PowerOff,
  RefreshCw,
  ScanText,
  Trash2,
  Wand2,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import {
  createBatch,
  createApiRouteCredential,
  deleteRouteCredential,
  fetchRouteModels,
  getRoutePool,
  getRouteProxyStatus,
  importOfficialRouteCredentialsFromFiles,
  importOfficialRouteCredentialsFromText,
  listRouteCredentials,
  refreshRouteCredentialQuota,
  refreshRouteCredentialsQuota,
  routePoolTestModel,
  setRoutePoolMembers,
  startRouteProxy,
  stopRouteProxy,
  updateRouteCredential,
  writeRouteProxyConfigs,
} from "../lib/api/client";
import type {
  AccountStatus,
  AnthropicApiKeyField,
  FetchedRouteModel,
  InterfaceFormat,
  ModelMapping,
  RouteConfigWriteOutcome,
  QuotaRefreshOutcome,
  RouteCredential,
  RouteModelsFetchRequest,
  RoutePoolModelTestOutcome,
  RoutePoolModelTestRequest,
  RoutePoolUsageLog,
} from "../lib/api/types";
import {
  ClipboardImageReadError,
  readClipboardImageBlob,
  recognizeApiKeysFromImageBlob,
} from "../lib/ocr/apiKeyOcr";

type PlatformKey = "codex" | "claude" | "grok" | "gemini" | "opencode" | "openclaw" | "hermes";
type CreateMode = "api" | "official";

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

type ParsedUsageMetadata = {
  path: string;
  status: string;
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

function parseUsageMetadata(metadataJson: string): ParsedUsageMetadata {
  try {
    const value = JSON.parse(metadataJson) as unknown;
    const formattedJson = JSON.stringify(value, null, 2) ?? metadataJson;
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      return {
        path: "-",
        status: "-",
        formattedJson,
        raw: metadataJson,
        valid: true,
      };
    }
    const record = value as Record<string, unknown>;
    return {
      path: metadataField(record, "path"),
      status: metadataField(record, "status"),
      formattedJson,
      raw: metadataJson,
      valid: true,
    };
  } catch {
    return {
      path: "-",
      status: "-",
      formattedJson: metadataJson,
      raw: metadataJson,
      valid: false,
    };
  }
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
          <p className="text-[11px] font-medium text-stone-500">时间</p>
          <p className="mt-0.5 text-stone-800">{formatUsageTime(request.created_at)}</p>
        </div>
      </div>
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

const interfaceFormats: InterfaceFormat[] = [
  "openai",
  "openai-responses",
  "anthropic",
  "anthropic-messages",
  "gemini",
];

const interfaceFormatLabels: Record<InterfaceFormat, string> = {
  openai: "OpenAI Chat Completions",
  "openai-responses": "OpenAI Responses",
  anthropic: "Claude Messages",
  "anthropic-messages": "Claude Messages（兼容）",
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

function isAnthropicInterfaceFormat(value: InterfaceFormat | string) {
  return value === "anthropic" || value === "anthropic-messages";
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
  if (platform === "claude" || interfaceFormat === "anthropic" || interfaceFormat === "anthropic-messages") {
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

function interfaceFormatFromConfig(config: Record<string, unknown>): InterfaceFormat {
  const value = stringFromRecord(config, "interface_format");
  return interfaceFormats.includes(value as InterfaceFormat) ? (value as InterfaceFormat) : "openai";
}

function apiSecretJsonWithKey(secretJson: string, apiKey: string) {
  const secret = parseJsonObject(secretJson);
  secret.api_key = apiKey.trim();
  return JSON.stringify(secret, null, 2);
}

function responsesCustomToolCompatFromConfig(config: Record<string, unknown>): boolean {
  return config.responses_custom_tool_compat === true;
}

function apiConfigJsonWithFields(
  configJson: string,
  baseUrl: string,
  interfaceFormat: InterfaceFormat,
  mappings: ModelMapping[],
  apiKeyField: AnthropicApiKeyField,
  responsesCustomToolCompat = false,
) {
  const config = parseJsonObject(configJson);
  config.base_url = baseUrl.trim();
  config.interface_format = interfaceFormat;
  config.model_mappings = mappings;
  config.responses_custom_tool_compat = responsesCustomToolCompat;
  if (isAnthropicInterfaceFormat(interfaceFormat)) {
    config.api_key_field = apiKeyField;
  } else {
    delete config.api_key_field;
  }
  return JSON.stringify(config, null, 2);
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
            暂无模型映射。需要改写上游模型时再新增。
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

function prettyJsonOrText(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}

export function AccountsScreen({ onOpenSessions, platform = "codex" }: AccountsScreenProps) {
  const queryClient = useQueryClient();
  const activePlatform = platform;
  const [draftPoolIds, setDraftPoolIds] = useState<Set<string>>(() => new Set());
  const [statsOpen, setStatsOpen] = useState(false);
  const [statsPeriod, setStatsPeriod] = useState<RouteStatsPeriod>("today");
  const [requestPage, setRequestPage] = useState(1);
  const [expandedRequestId, setExpandedRequestId] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [createMode, setCreateMode] = useState<CreateMode>("api");
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
  const [editApiKey, setEditApiKey] = useState("");
  const [editApiKeyDecodeError, setEditApiKeyDecodeError] = useState<string | null>(null);
  const [editApiKeyOcrError, setEditApiKeyOcrError] = useState<string | null>(null);
  const [editApiKeyOcrRecognizing, setEditApiKeyOcrRecognizing] = useState(false);
  const editApiKeyOcrFileInputRef = useRef<HTMLInputElement | null>(null);
  const [editApiBaseUrl, setEditApiBaseUrl] = useState("");
  const [editApiInterfaceFormat, setEditApiInterfaceFormat] = useState<InterfaceFormat>("openai");
  const [editResponsesCustomToolCompat, setEditResponsesCustomToolCompat] = useState(false);
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
  const [modelTestDialogOpen, setModelTestDialogOpen] = useState(false);
  const [modelTestAccount, setModelTestAccount] = useState<RouteCredential | null>(null);
  const [testingAccountId, setTestingAccountId] = useState<string | null>(null);
  const [refreshingQuotaId, setRefreshingQuotaId] = useState<string | null>(null);
  const [quotaRefreshMessage, setQuotaRefreshMessage] = useState<string | null>(null);
  const autoQuotaRefreshedPlatform = useRef<string | null>(null);
  const [modelTestOutcome, setModelTestOutcome] = useState<RoutePoolModelTestOutcome | null>(null);
  const [configWriteOutcomes, setConfigWriteOutcomes] = useState<RouteConfigWriteOutcome[]>([]);
  const routeTestModel = routeTestModelsByPlatform[activePlatform] ?? "";
  const statsSince = useMemo(() => routeStatsSince(statsPeriod), [statsPeriod]);

  const credentialsQuery = useQuery({
    queryKey: ["route-credentials", activePlatform],
    queryFn: () => listRouteCredentials(activePlatform),
    staleTime: 0,
    refetchOnMount: "always",
    refetchOnWindowFocus: true,
  });

  const routePoolQuery = useQuery({
    queryKey: ["route-pool", activePlatform, statsSince, requestPage, routeStatsPageSize],
    queryFn: () => getRoutePool(activePlatform, statsSince, requestPage, routeStatsPageSize),
    refetchInterval: statsOpen ? routeStatsRefreshMs : false,
  });
  const routeProxyQuery = useQuery({
    queryKey: ["route-proxy-status"],
    queryFn: getRouteProxyStatus,
  });

  useEffect(() => {
    setRequestPage(1);
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
    const secret = parseJsonObject(editingCredential.secret_payload_json);
    const config = parseJsonObject(editingCredential.config_json);
    setEditSecretJson(parseJsonPreview(editingCredential.secret_payload_json, editingCredential.secret_payload_json));
    setEditConfigJson(parseJsonPreview(editingCredential.config_json, editingCredential.config_json));
    if (editingCredential.kind === "api") {
      const interfaceFormat = interfaceFormatFromConfig(config);
      setEditApiKey(stringFromRecord(secret, "api_key"));
      setEditApiBaseUrl(stringFromRecord(config, "base_url"));
      setEditApiInterfaceFormat(interfaceFormat);
      setEditApiKeyField(anthropicApiKeyFieldFromConfig(config, "ANTHROPIC_API_KEY"));
      setEditResponsesCustomToolCompat(responsesCustomToolCompatFromConfig(config));
      setEditApiKeyDecodeError(null);
      setEditApiKeyOcrError(null);
    } else {
      setEditApiKey("");
      setEditApiBaseUrl("");
      setEditApiInterfaceFormat("openai");
      setEditApiKeyField("ANTHROPIC_API_KEY");
      setEditResponsesCustomToolCompat(false);
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

  const credentials = credentialsQuery.data ?? [];
  const groupedCredentials = useMemo(() => {
    const groups = new Map<string, RouteCredential[]>();
    for (const credential of credentials) {
      const key = credential.batch_id ? `批量 ${shortId(credential.batch_id)}` : "单个账号";
      groups.set(key, [...(groups.get(key) ?? []), credential]);
    }
    return Array.from(groups.entries()).map(([name, items]) => ({ name, items }));
  }, [credentials]);

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
    editingCredential?.kind,
  ]);

  const invalidateAccountData = async () => {
    await Promise.all([
      queryClient.invalidateQueries({
        queryKey: ["route-credentials", activePlatform],
        refetchType: "active",
      }),
      queryClient.invalidateQueries({
        queryKey: ["route-pool", activePlatform],
        refetchType: "active",
      }),
    ]);
    // Force network refetch so status/import changes show immediately.
    await Promise.all([
      queryClient.refetchQueries({ queryKey: ["route-credentials", activePlatform], type: "active" }),
      queryClient.refetchQueries({ queryKey: ["route-pool", activePlatform], type: "active" }),
    ]);
  };

  const mergeCredentialsIntoCache = (imported: RouteCredential[]) => {
    if (!imported.length) {
      return;
    }
    queryClient.setQueryData<RouteCredential[]>(
      ["route-credentials", activePlatform],
      (current) => {
        const byId = new Map((current ?? []).map((item) => [item.id, item]));
        for (const item of imported) {
          byId.set(item.id, item);
        }
        return Array.from(byId.values()).sort((left, right) => {
          if (left.sort_order !== right.sort_order) {
            return left.sort_order - right.sort_order;
          }
          return right.created_at.localeCompare(left.created_at);
        });
      },
    );
  };

  useEffect(() => {
    if (credentialsQuery.isLoading || credentialsQuery.isFetching) {
      return;
    }
    if (autoQuotaRefreshedPlatform.current === activePlatform) {
      return;
    }
    const officialIds = (credentialsQuery.data ?? [])
      .filter((item) => item.kind === "official" && item.status === "ok")
      .map((item) => item.id);
    if (!officialIds.length) {
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
  }, [activePlatform, credentialsQuery.data, credentialsQuery.isFetching, credentialsQuery.isLoading]);

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
        if (officialFilePaths.length > 0) {
          return importOfficialRouteCredentialsFromFiles({
            platform: activePlatform,
            file_paths: officialFilePaths,
            batch_name: officialBatchName.trim() || null,
          });
        }
        if (!officialText.trim()) {
          throw new Error("请粘贴账号 JSON，或选择 JSON 文件导入。");
        }
        return importOfficialRouteCredentialsFromText({
          platform: activePlatform,
          text: officialText,
          batch_name: officialBatchName.trim() || null,
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
      if (result && typeof result === "object" && "imported" in result) {
        const imported = (result as { imported?: RouteCredential[] }).imported ?? [];
        mergeCredentialsIntoCache(imported);
      }
      await invalidateAccountData();
    },
  });

  const routePoolMutation = useMutation({
    mutationFn: (input: { platform: string; account_ids: string[] }) => setRoutePoolMembers(input),
    onSuccess: (state) => {
      setDraftPoolIds(new Set(state.account_ids));
      void queryClient.invalidateQueries({ queryKey: ["route-pool", activePlatform] });
    },
    onError: () => {
      if (routePoolQuery.data) {
        setDraftPoolIds(new Set(routePoolQuery.data.account_ids));
      }
    },
  });
  const modelTestMutation = useMutation({
    mutationFn: (request: RoutePoolModelTestRequest) => routePoolTestModel(request),
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
    mutationFn: (id: string) => refreshRouteCredentialQuota(id),
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
    mutationFn: () => refreshRouteCredentialsQuota(activePlatform),
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
    mutationFn: () => writeRouteProxyConfigs(routeProxyQuery.data?.base_url ?? null, activePlatform),
    onSuccess: setConfigWriteOutcomes,
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
            )
          : editConfigJson.trim() || "{}";
      const nextPreviewJson =
        editingCredential.kind === "api"
          ? apiPreviewJsonFromPayloads(activePlatform, nextSecretJson, nextConfigJson)
          : editPreviewJson.trim() || "{}";
      return updateRouteCredential(editingCredential.id, {
        display_name: editName.trim(),
        email: editingCredential.kind === "api" ? null : editEmail.trim() || null,
        status: editStatus,
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

  const togglePool = (credentialId: string) => {
    const next = new Set(draftPoolIds);
    if (next.has(credentialId)) {
      next.delete(credentialId);
    } else {
      next.add(credentialId);
    }
    setDraftPoolIds(next);
    routePoolMutation.mutate({
      platform: activePlatform,
      account_ids: Array.from(next),
    });
  };

  const openRouteTestDialog = () => {
    setTestingAccountId(null);
    setModelTestAccount(null);
    setModelTestDialogOpen(true);
  };

  const openAccountTestDialog = (credential: RouteCredential) => {
    setModelTestAccount(credential);
    setModelTestDialogOpen(true);
  };

  const submitModelTest = () => {
    const accountId = modelTestAccount?.id ?? null;
    setTestingAccountId(accountId);
    setModelTestOutcome(null);
    modelTestMutation.reset();
    modelTestMutation.mutate({
      platform: activePlatform,
      ...(accountId ? { account_id: accountId } : {}),
      model: routeTestModel.trim() || null,
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

  const toggleStatsPanel = () => {
    if (!statsOpen) {
      void routePoolQuery.refetch();
    }
    setStatsOpen((open) => !open);
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

  return (
    <section className="space-y-3">
      <div className="rounded-2xl border border-stone-200 bg-white/82 shadow-sm">
        <div className="flex flex-col gap-3 border-b border-stone-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0">
            <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
              {platformLabels[activePlatform]}
            </p>
            <h1 className="mt-0.5 text-lg font-semibold tracking-tight text-stone-950">账号列表</h1>
          </div>
          <div className="flex flex-wrap gap-2">
            <button
              className="inline-flex items-center justify-center gap-1.5 rounded-xl border border-emerald-200 bg-emerald-50 px-3 py-2 text-[13px] font-semibold text-emerald-900 shadow-sm transition-colors hover:bg-emerald-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-emerald-300"
              onClick={() => onOpenSessions?.(activePlatform)}
              type="button"
            >
              <MessageSquareText className="h-3.5 w-3.5" />
              会话管理
            </button>
            <button
              className="inline-flex items-center justify-center gap-1.5 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white shadow-sm transition-colors hover:bg-stone-800 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400"
              onClick={() => setCreateOpen(true)}
              type="button"
            >
              <Plus className="h-3.5 w-3.5" />
              新增账号
            </button>
          </div>
        </div>

        <div className="mx-4 mt-3 rounded-2xl border border-emerald-200 bg-gradient-to-r from-emerald-50 to-white px-3 py-2.5">
          <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
            <div className="flex min-w-0 flex-1 items-start gap-2">
              <span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-emerald-600 text-white shadow-sm">
                <KeyRound className="h-4 w-4" />
              </span>
              <div className="min-w-0 flex-1 space-y-1">
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                  <span className="inline-flex shrink-0 items-center gap-1.5 rounded-xl border border-emerald-200 bg-white/90 px-2.5 py-1.5 text-[12px] font-semibold text-emerald-900">
                    算力池
                  </span>
                  <span className="text-[12px] font-medium text-stone-600">
                    已加入 {draftPoolIds.size} 个账号
                  </span>
                </div>
                <div className="flex min-w-0 flex-wrap gap-x-4 gap-y-1 text-[12px] font-medium text-stone-500">
                  <span className="min-w-0 break-all">
                    本地代理：{routeProxyQuery.data?.running ? routeProxyQuery.data.base_url ?? "运行中" : "未启动"}
                  </span>
                  {lastRouteAccount && (
                    <span className="min-w-0 break-all">
                      最近路由到：{lastRouteAccount}
                    </span>
                  )}
                </div>
              </div>
            </div>

            <div className="flex shrink-0 flex-wrap items-center gap-2 lg:flex-nowrap">
              <button
                aria-label={routeProxyQuery.data?.running ? "停止本地路由代理" : "启动本地路由代理"}
                className="inline-flex items-center justify-center gap-1.5 rounded-xl border border-emerald-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-800 transition-colors hover:bg-emerald-50 disabled:opacity-50"
                disabled={startProxyMutation.isPending || stopProxyMutation.isPending}
                onClick={() => {
                  if (routeProxyQuery.data?.running) {
                    stopProxyMutation.mutate();
                  } else {
                    startProxyMutation.mutate();
                  }
                }}
                type="button"
              >
                {routeProxyQuery.data?.running ? <PowerOff className="h-3.5 w-3.5" /> : <Power className="h-3.5 w-3.5" />}
                {routeProxyQuery.data?.running ? "停止代理" : "启动代理"}
              </button>
              <button
                aria-label="写入路由配置文件"
                className="inline-flex items-center justify-center gap-1.5 rounded-xl border border-emerald-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-800 transition-colors hover:bg-emerald-50 disabled:opacity-50"
                disabled={!routeProxyQuery.data?.running || writeConfigsMutation.isPending}
                onClick={() => writeConfigsMutation.mutate()}
                type="button"
              >
                <FileCode2 className="h-3.5 w-3.5" />
                写入配置
              </button>
              <button
                aria-label="真实生成测试算力池路由"
                className="inline-flex items-center justify-center gap-1.5 rounded-xl bg-emerald-700 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-emerald-800 disabled:opacity-50"
                disabled={draftPoolIds.size === 0 || modelTestMutation.isPending}
                onClick={openRouteTestDialog}
                type="button"
              >
                <Play className="h-3.5 w-3.5" />
                生成测试
              </button>
              <button
                aria-label="查看算力池统计"
                className="inline-flex items-center justify-center gap-1.5 rounded-xl border border-emerald-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-800 transition-colors hover:bg-emerald-50"
                onClick={toggleStatsPanel}
                type="button"
              >
                <BarChart3 className="h-3.5 w-3.5" />
                统计
              </button>
            </div>
          </div>
        </div>

        {configWriteOutcomes.length > 0 && (
          <div className="mx-4 mb-3 space-y-1 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] text-stone-600">
            <p className="font-semibold text-stone-950">配置写入结果</p>
            {configWriteOutcomes.map((outcome) => (
              <p key={`${outcome.target_key}:${outcome.path}`}>
                {outcome.target_key}: {outcome.path} ({outcome.status}) · {outcome.route_proxy_key}
              </p>
            ))}
          </div>
        )}
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
          <div className="space-y-3 border-t border-stone-200 px-4 py-3">
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

            <div className="grid gap-2 sm:grid-cols-3">
              <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
                <p className="text-[11px] font-medium text-stone-500">请求</p>
                <p className="mt-1 text-lg font-semibold text-stone-950">{routeStats?.request_count ?? 0}</p>
              </div>
              <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
                <p className="text-[11px] font-medium text-stone-500">Token</p>
                <p className="mt-1 text-lg font-semibold text-stone-950">
                  {(routeStats?.token_count ?? 0).toLocaleString()}
                </p>
              </div>
              <div className="rounded-xl border border-stone-200 bg-stone-50 p-3">
                <p className="text-[11px] font-medium text-stone-500">费用</p>
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
                        <div className="grid gap-2 px-3 py-2.5 text-[12px] text-stone-600 lg:grid-cols-[1.2fr_1fr_0.5fr_1.4fr_0.8fr_auto] lg:items-center">
                          <span className="font-medium text-stone-800">{formatUsageTime(request.created_at)}</span>
                          <span className="truncate">{request.account_name ?? request.account_id ?? "-"}</span>
                          <span className="rounded-lg bg-stone-100 px-2 py-1 text-center font-semibold text-stone-700">
                            {metadata.status}
                          </span>
                          <span className="truncate font-mono text-[11px]">{metadata.path}</span>
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
              <div className="flex flex-col gap-2 border-t border-stone-100 bg-stone-50 px-3 py-2 sm:flex-row sm:items-center sm:justify-between">
                <p className="text-[11px] font-medium text-stone-500">
                  共 {requestRowCount} 条 · 每页 {resolvedRequestPageSize} 条
                </p>
                <div className="flex items-center gap-2">
                  <button
                    aria-label="上一页请求"
                    className="inline-flex items-center gap-1.5 rounded-lg border border-stone-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50 disabled:opacity-50"
                    disabled={resolvedRequestPage <= 1}
                    onClick={() => setRequestPage((page) => Math.max(1, page - 1))}
                    type="button"
                  >
                    <ChevronLeft className="h-3.5 w-3.5" />
                    上一页
                  </button>
                  <span className="min-w-20 text-center text-[12px] font-semibold text-stone-600">
                    第 {resolvedRequestPage} / {requestPageCount} 页
                  </span>
                  <button
                    aria-label="下一页请求"
                    className="inline-flex items-center gap-1.5 rounded-lg border border-stone-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50 disabled:opacity-50"
                    disabled={resolvedRequestPage >= requestPageCount}
                    onClick={() => setRequestPage((page) => page + 1)}
                    type="button"
                  >
                    下一页
                    <ChevronRight className="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>

      <section className="rounded-2xl border border-stone-200 bg-white/82 shadow-sm">
        <div className="flex items-center justify-between gap-3 border-b border-stone-200 px-4 py-3">
          <div>
            <h2 className="text-[15px] font-semibold text-stone-950">{platformLabels[activePlatform]} 账号</h2>
          </div>
          <div className="flex items-center gap-2">
            <button
              aria-label="刷新账号列表"
              className="inline-flex items-center gap-1.5 rounded-lg border border-stone-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50 disabled:opacity-50"
              disabled={credentialsQuery.isFetching}
              onClick={() => {
                void invalidateAccountData();
              }}
              type="button"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${credentialsQuery.isFetching ? "animate-spin" : ""}`} />
              刷新
            </button>
            <button
              aria-label="刷新官方账号额度"
              className="inline-flex items-center gap-1.5 rounded-lg border border-violet-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-violet-700 transition-colors hover:bg-violet-50 disabled:opacity-50"
              disabled={quotaRefreshPlatformMutation.isPending || credentialsQuery.isFetching}
              onClick={() => quotaRefreshPlatformMutation.mutate()}
              type="button"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${quotaRefreshPlatformMutation.isPending ? "animate-spin" : ""}`} />
              刷新额度
            </button>
            <span className="rounded-full bg-stone-100 px-2.5 py-1 text-[12px] font-semibold text-stone-600">
              {credentials.length} 个
            </span>
          </div>
        </div>

        <div className="space-y-3 p-3">
          {quotaRefreshMessage && (
            <p className="rounded-xl bg-violet-50 px-3 py-2 text-[12px] font-medium text-violet-800">
              {quotaRefreshMessage}
            </p>
          )}
          {credentialsQuery.isLoading && <p className="rounded-xl bg-stone-50 p-4 text-sm text-stone-500">正在加载账号...</p>}
          {credentialsQuery.error && <p className="rounded-xl bg-red-50 p-4 text-sm text-red-700">账号加载失败。</p>}
          {!credentialsQuery.isLoading && credentials.length === 0 && (
            <div className="rounded-xl border border-dashed border-stone-300 bg-stone-50 p-6 text-center text-sm text-stone-500">
              暂无账号
            </div>
          )}

          {groupedCredentials.map((group) => (
            <div className="overflow-hidden rounded-xl border border-stone-200 bg-white" key={group.name}>
              <div className="flex items-center justify-between border-b border-stone-100 bg-stone-50/80 px-3 py-2">
                <p className="text-[12px] font-semibold text-stone-700">{group.name}</p>
                <p className="text-[11px] font-medium text-stone-500">{group.items.length} 个账号</p>
              </div>
              <div className="divide-y divide-stone-100">
                {group.items.map((credential) => {
                  const subscriptionType = officialSubscriptionType(credential);
                  const primaryRemain = officialPrimaryRemain(credential);
                  const weeklyRemain = officialWeeklyRemain(credential);
                  const latestReset = officialLatestResetLabel(credential);
                  return (
                  <div className="grid gap-2 px-3 py-2.5 lg:grid-cols-[auto_1fr_auto] lg:items-center" key={credential.id}>
                    <input
                      aria-label={`将 ${credential.display_name} 加入算力池`}
                      checked={draftPoolIds.has(credential.id)}
                      className="h-4 w-4 rounded border-stone-300 text-amber-500 focus:ring-blue-400"
                      disabled={routePoolMutation.isPending}
                      onChange={() => togglePool(credential.id)}
                      type="checkbox"
                    />
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <p
                          className="w-[36ch] max-w-full shrink-0 truncate text-[13px] font-semibold text-stone-950"
                          title={credential.display_name}
                        >
                          {credential.display_name}
                        </p>
                        <span className="rounded-full bg-amber-50 px-2 py-0.5 text-[11px] font-semibold text-amber-800">
                          {kindLabel(credential.kind)}
                        </span>
                        <span
                          className={`rounded-full px-2 py-0.5 text-[11px] font-semibold ${accountStatusClass(credential.status)}`}
                          title={credential.status}
                        >
                          {accountStatusLabel(credential.status)}
                        </span>
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
                        {credential.email ?? credential.platform} · {shortId(credential.id)}
                      </p>
                    </div>
                    <div className="flex items-center justify-end gap-2">
                      {credential.kind === "official" && (
                        <button
                          aria-label={`刷新 ${credential.display_name} 额度`}
                          className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-violet-200 px-2.5 py-1.5 text-[12px] font-semibold text-violet-700 transition-colors hover:bg-violet-50 disabled:opacity-50"
                          disabled={quotaRefreshMutation.isPending || quotaRefreshPlatformMutation.isPending}
                          onClick={() => quotaRefreshMutation.mutate(credential.id)}
                          type="button"
                        >
                          <RefreshCw className={`h-3.5 w-3.5 ${refreshingQuotaId === credential.id ? "animate-spin" : ""}`} />
                          {refreshingQuotaId === credential.id ? "刷新中" : "额度"}
                        </button>
                      )}
                      <button
                        aria-label={`测试 ${credential.display_name}`}
                        className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-emerald-200 px-2.5 py-1.5 text-[12px] font-semibold text-emerald-700 transition-colors hover:bg-emerald-50 disabled:opacity-50"
                        disabled={modelTestMutation.isPending}
                        onClick={() => openAccountTestDialog(credential)}
                        type="button"
                      >
                        <Play className="h-3.5 w-3.5" />
                        {testingAccountId === credential.id && modelTestMutation.isPending ? "测试中" : "测试"}
                      </button>
                      <button
                        aria-label={`编辑 ${credential.display_name}`}
                        className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-stone-200 px-2.5 py-1.5 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50"
                        onClick={() => {
                          updateMutation.reset();
                          deleteMutation.reset();
                          setEditingCredential(credential);
                        }}
                        type="button"
                      >
                        <Edit3 className="h-3.5 w-3.5" />
                        编辑
                      </button>
                    </div>
                  </div>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      </section>

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
                disabled={modelTestMutation.isPending || (!modelTestAccount && draftPoolIds.size === 0)}
                onClick={submitModelTest}
                type="button"
              >
                {modelTestMutation.isPending ? "测试中..." : "开始测试"}
              </button>
            </div>
          </div>
        </div>
      )}

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
                ["official", "官方导入"],
              ].map(([mode, label]) => (
                <button
                  className={`rounded-lg px-3 py-1.5 text-[13px] font-semibold transition-colors ${
                    createMode === mode ? "bg-white text-stone-950 shadow-sm" : "text-stone-500"
                  }`}
                  key={mode}
                  onClick={() => setCreateMode(mode as CreateMode)}
                  type="button"
                >
                  {label}
                </button>
              ))}
            </div>

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
                    {interfaceFormats.map((format) => (
                      <option key={format} value={format}>
                        {interfaceFormatLabel(format)}
                      </option>
                    ))}
                  </select>
                </label>
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
                      把 custom 工具改写成 function，给不支持 custom 的中转站用。默认关闭。
                    </span>
                  </span>
                </label>
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
                  粘贴 session JSON、auth.json、账号 JSON、Sub2API JSON、accessToken 或 refresh_token。
                </p>
                <details className="overflow-hidden rounded-xl border border-stone-200 bg-white">
                  <summary className="flex cursor-pointer list-none items-center gap-2 border-b border-stone-100 px-3 py-2 text-[12px] font-semibold text-stone-700">
                    <ChevronDown className="h-3.5 w-3.5" />
                    必填字段与示例（点击展开）
                  </summary>
                  <div className="space-y-3 p-3 text-[12px] text-stone-600">
                    <p>支持 session JSON、完整 tokens（id_token + access_token）、Sub2API 导出 JSON、仅 accessToken 或仅 refresh_token。</p>
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
                  批量名称（可选）
                  <input
                    aria-label="导入批量名称"
                    className={fieldClass}
                    onChange={(event) => setOfficialBatchName(event.target.value)}
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
                disabled={createMutation.isPending}
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
                </select>
              </label>
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
                      {interfaceFormats.map((format) => (
                        <option key={format} value={format}>
                          {interfaceFormatLabel(format)}
                        </option>
                      ))}
                    </select>
                  </label>
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
                        把 custom 工具改写成 function，给不支持 custom 的中转站用。默认关闭。
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
                        setEditConfigJson(event.target.value);
                        setEditModelMappings(parseModelMappingsFromConfig(event.target.value));
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
