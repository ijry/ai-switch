import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowRight,
  BarChart3,
  ChevronDown,
  Edit3,
  FileCode2,
  KeyRound,
  MessageSquareText,
  Play,
  Plus,
  Power,
  PowerOff,
  ScanText,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import {
  createBatch,
  createApiRouteCredential,
  deleteRouteCredential,
  getRoutePool,
  getRouteProxyStatus,
  importOfficialRouteCredentialsFromFiles,
  importOfficialRouteCredentialsFromText,
  listRouteCredentials,
  routePoolRouteOnce,
  setRoutePoolMembers,
  startRouteProxy,
  stopRouteProxy,
  updateRouteCredential,
  writeRouteProxyConfigs,
} from "../lib/api/client";
import type {
  AccountStatus,
  InterfaceFormat,
  ModelMapping,
  RouteConfigWriteOutcome,
  RouteCredential,
} from "../lib/api/types";
import {
  ClipboardImageReadError,
  readClipboardImageBlob,
  recognizeApiKeysFromImageBlob,
} from "../lib/ocr/apiKeyOcr";

type PlatformKey = "codex" | "claude" | "gemini" | "opencode" | "openclaw" | "hermes";
type CreateMode = "api" | "official";

type AccountsScreenProps = {
  platform?: PlatformKey;
  onOpenSessions?: (platform: PlatformKey) => void;
};

const platformLabels: Record<PlatformKey, string> = {
  codex: "Codex",
  claude: "Claude",
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

const claudeModelSources = [
  { value: "claude-opus", label: "Claude Opus" },
  { value: "claude-sonnet", label: "Claude Sonnet" },
  { value: "claude-haiku", label: "Claude Haiku" },
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

function defaultModelMappings(platform: PlatformKey): ModelMapping[] {
  if (platform === "claude") {
    return [];
  }
  return [{ from: "gpt-5", to: "upstream-model" }];
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
      }));
  } catch {
    return [];
  }
}

function normalizeModelMappings(mappings: ModelMapping[]) {
  const normalized: ModelMapping[] = [];
  for (const mapping of mappings) {
    const from = mapping.from.trim();
    const to = mapping.to.trim();
    if (!from && !to) {
      continue;
    }
    if (!from || !to) {
      return {
        error: "模型映射需要同时填写请求模型和上游模型。",
        mappings: [],
      };
    }
    normalized.push({ from, to });
  }

  return { error: null, mappings: normalized };
}

function configJsonWithModelMappings(configJson: string, mappings: ModelMapping[]) {
  const parsed = JSON.parse(configJson || "{}") as Record<string, unknown>;
  parsed.model_mappings = mappings;
  return JSON.stringify(parsed, null, 2);
}

type ModelMappingsEditorProps = {
  error?: string | null;
  label: string;
  onChange: (mappings: ModelMapping[]) => void;
  platform: PlatformKey;
  value: ModelMapping[];
};

function ModelMappingsEditor({ error, label, onChange, platform, value }: ModelMappingsEditorProps) {
  const rows = value.length > 0 ? value : [{ from: "", to: "" }];
  const sourceOptions =
    platform === "claude"
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
    onChange(next.length > 0 ? next : []);
  };

  return (
    <div className="grid gap-2">
      <div className="flex items-center justify-between gap-3">
        <p className="text-[12px] font-semibold text-stone-600">{label}</p>
        <button
          className="inline-flex items-center gap-1.5 rounded-lg border border-stone-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50"
          onClick={() => onChange([...rows, { from: "", to: "" }])}
          type="button"
        >
          <Plus className="h-3.5 w-3.5" />
          新增映射
        </button>
      </div>

      <div className="space-y-2 rounded-xl border border-stone-200 bg-stone-50/70 p-2">
        {rows.map((mapping, index) => (
          <div
            className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)_auto] sm:items-center"
            key={index}
          >
            {platform === "claude" ? (
              <select
                aria-label={`请求模型 ${index + 1}`}
                className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
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
            ) : (
              <input
                aria-label={`请求模型 ${index + 1}`}
                className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
                onChange={(event) => updateRow(index, { from: event.target.value })}
                placeholder="gpt-5"
                value={mapping.from}
              />
            )}
            <ArrowRight className="hidden h-4 w-4 text-stone-400 sm:block" />
            <input
              aria-label={`上游模型 ${index + 1}`}
              className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
              onChange={(event) => updateRow(index, { to: event.target.value })}
              placeholder="upstream-model"
              value={mapping.to}
            />
            <button
              aria-label={`删除模型映射 ${index + 1}`}
              className="grid h-9 w-9 place-items-center rounded-xl border border-stone-200 bg-white text-stone-500 transition-colors hover:bg-red-50 hover:text-red-700"
              onClick={() => removeRow(index)}
              type="button"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}
      </div>

      {error && <p className="text-[12px] font-semibold text-red-700">{error}</p>}
    </div>
  );
}

export function AccountsScreen({ onOpenSessions, platform = "codex" }: AccountsScreenProps) {
  const queryClient = useQueryClient();
  const activePlatform = platform;
  const [draftPoolIds, setDraftPoolIds] = useState<Set<string>>(() => new Set());
  const [statsOpen, setStatsOpen] = useState(false);
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
  const [apiBaseUrl, setApiBaseUrl] = useState("https://api.example.com/v1");
  const [apiInterfaceFormat, setApiInterfaceFormat] = useState<InterfaceFormat>("openai");
  const [apiMappings, setApiMappings] = useState<ModelMapping[]>(() => defaultModelMappings(activePlatform));
  const [apiMappingsError, setApiMappingsError] = useState<string | null>(null);
  const [apiPreviewJson, setApiPreviewJson] = useState("");
  const [editingCredential, setEditingCredential] = useState<RouteCredential | null>(null);
  const [editName, setEditName] = useState("");
  const [editEmail, setEditEmail] = useState("");
  const [editStatus, setEditStatus] = useState<AccountStatus>("ok");
  const [editSecretJson, setEditSecretJson] = useState("{}");
  const [editConfigJson, setEditConfigJson] = useState("{}");
  const [editModelMappings, setEditModelMappings] = useState<ModelMapping[]>([]);
  const [editModelMappingsError, setEditModelMappingsError] = useState<string | null>(null);
  const [editPreviewJson, setEditPreviewJson] = useState("{}");
  const [lastRouteAccount, setLastRouteAccount] = useState<string | null>(null);
  const [configWriteOutcomes, setConfigWriteOutcomes] = useState<RouteConfigWriteOutcome[]>([]);

  const credentialsQuery = useQuery({
    queryKey: ["route-credentials", activePlatform],
    queryFn: () => listRouteCredentials(activePlatform),
  });
  const routePoolQuery = useQuery({
    queryKey: ["route-pool", activePlatform],
    queryFn: () => getRoutePool(activePlatform),
  });
  const routeProxyQuery = useQuery({
    queryKey: ["route-proxy-status"],
    queryFn: getRouteProxyStatus,
  });

  useEffect(() => {
    if (routePoolQuery.data) {
      setDraftPoolIds(new Set(routePoolQuery.data.account_ids));
    }
  }, [routePoolQuery.data]);

  useEffect(() => {
    setOfficialText(defaultOfficialJson(activePlatform));
    setOfficialFilePaths([]);
    setApiMappings(defaultModelMappings(activePlatform));
    setApiMappingsError(null);
  }, [activePlatform]);

  useEffect(() => {
    if (!editingCredential) {
      return;
    }
    setEditName(editingCredential.display_name);
    setEditEmail(editingCredential.email ?? "");
    setEditStatus(editingCredential.status);
    setEditSecretJson(parseJsonPreview(editingCredential.secret_payload_json, editingCredential.secret_payload_json));
    setEditConfigJson(parseJsonPreview(editingCredential.config_json, editingCredential.config_json));
    setEditModelMappings(parseModelMappingsFromConfig(editingCredential.config_json));
    setEditModelMappingsError(null);
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

  const invalidateAccountData = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["route-credentials", activePlatform] }),
      queryClient.invalidateQueries({ queryKey: ["route-pool", activePlatform] }),
    ]);
  };

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
      const normalizedMappings = normalizeModelMappings(apiMappings);
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
      for (const [index, key] of apiKeys.entries()) {
        imported.push(
          await createApiRouteCredential({
            platform: activePlatform,
            display_name: apiKeys.length > 1 ? `${apiName.trim()} ${index + 1}` : apiName.trim(),
            api_key: key,
            base_url: apiBaseUrl,
            interface_format: apiInterfaceFormat,
            model_mappings_json: JSON.stringify(normalizedMappings.mappings),
            preview_json: apiPreviewJson.trim() || null,
            batch_id: batch?.id ?? null,
          }),
        );
      }
      return { imported, failed: [] };
    },
    onSuccess: async () => {
      setCreateOpen(false);
      await invalidateAccountData();
    },
  });

  const routePoolMutation = useMutation({
    mutationFn: (input: { platform: string; account_ids: string[] }) => setRoutePoolMembers(input),
    onSuccess: (state) => {
      queryClient.setQueryData(["route-pool", activePlatform], state);
      setDraftPoolIds(new Set(state.account_ids));
    },
    onError: () => {
      if (routePoolQuery.data) {
        setDraftPoolIds(new Set(routePoolQuery.data.account_ids));
      }
    },
  });
  const routeOnceMutation = useMutation({
    mutationFn: (request: {
      platform: string;
      token_count?: number | null;
      cost_micros?: number | null;
      metadata_json?: string | null;
    }) => routePoolRouteOnce(request),
    onSuccess: (outcome) => {
      queryClient.setQueryData(["route-pool", activePlatform], {
        platform: outcome.platform,
        account_ids: routePoolQuery.data?.account_ids ?? Array.from(draftPoolIds),
        stats: outcome.stats,
      });
      setStatsOpen(true);
      setLastRouteAccount(outcome.selected_account_name);
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
      const normalizedMappings = normalizeModelMappings(editModelMappings);
      if (editingCredential.kind === "api" && normalizedMappings.error) {
        setEditModelMappingsError(normalizedMappings.error);
        throw new Error(normalizedMappings.error);
      }
      setEditModelMappingsError(null);
      return updateRouteCredential(editingCredential.id, {
        display_name: editName.trim(),
        email: editEmail.trim() || null,
        status: editStatus,
        secret_payload_json: editSecretJson.trim() || "{}",
        config_json:
          editingCredential.kind === "api"
            ? configJsonWithModelMappings(editConfigJson.trim() || "{}", normalizedMappings.mappings)
            : editConfigJson.trim() || "{}",
        preview_json: editPreviewJson.trim() || "{}",
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

  const testRoute = () => {
    routeOnceMutation.mutate({
      platform: activePlatform,
      token_count: 1024,
      cost_micros: 1200,
      metadata_json: JSON.stringify({ source: "ui_test_route" }),
    });
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
          <div className="flex items-center justify-between gap-3">
            <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
              <span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-emerald-600 text-white shadow-sm">
                <KeyRound className="h-4 w-4" />
              </span>
              <span className="inline-flex shrink-0 items-center gap-1.5 rounded-xl border border-emerald-200 bg-white/90 px-2.5 py-1.5 text-[12px] font-semibold text-emerald-900">
                算力池
              </span>
              <span className="shrink-0 rounded-xl border border-emerald-100 bg-white/90 px-2.5 py-1.5 text-[12px] font-medium text-stone-600">
                已加入 {draftPoolIds.size} 个账号
              </span>
              <span className="min-w-0 truncate rounded-xl border border-emerald-100 bg-white/90 px-2.5 py-1.5 text-[12px] font-medium text-stone-600">
                本地代理：{routeProxyQuery.data?.running ? routeProxyQuery.data.base_url ?? "运行中" : "未启动"}
              </span>
              {lastRouteAccount && (
                <span className="min-w-0 truncate rounded-xl border border-emerald-100 bg-white/90 px-2.5 py-1.5 text-[12px] font-medium text-stone-600">
                  最近路由到：{lastRouteAccount}
                </span>
              )}
            </div>

            <div className="flex shrink-0 flex-nowrap items-center gap-2">
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
                aria-label="测试算力池路由"
                className="inline-flex items-center justify-center gap-1.5 rounded-xl bg-emerald-700 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-emerald-800 disabled:opacity-50"
                disabled={draftPoolIds.size === 0 || routeOnceMutation.isPending}
                onClick={testRoute}
                type="button"
              >
                <Play className="h-3.5 w-3.5" />
                测试路由
              </button>
              <button
                aria-label="查看算力池统计"
                className="inline-flex items-center justify-center gap-1.5 rounded-xl border border-emerald-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-800 transition-colors hover:bg-emerald-50"
                onClick={() => setStatsOpen((open) => !open)}
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
        {statsOpen && (
          <div className="grid gap-2 border-t border-stone-200 px-4 py-3 sm:grid-cols-3">
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
        )}
      </div>

      <section className="rounded-2xl border border-stone-200 bg-white/82 shadow-sm">
        <div className="flex items-center justify-between gap-3 border-b border-stone-200 px-4 py-3">
          <div>
            <h2 className="text-[15px] font-semibold text-stone-950">{platformLabels[activePlatform]} 账号</h2>
          </div>
          <span className="rounded-full bg-stone-100 px-2.5 py-1 text-[12px] font-semibold text-stone-600">
            {credentials.length} 个
          </span>
        </div>

        <div className="space-y-3 p-3">
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
                {group.items.map((credential) => (
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
                        <p className="truncate text-[13px] font-semibold text-stone-950">{credential.display_name}</p>
                        <span className="rounded-full bg-amber-50 px-2 py-0.5 text-[11px] font-semibold text-amber-800">
                          {kindLabel(credential.kind)}
                        </span>
                        <span className="rounded-full bg-stone-100 px-2 py-0.5 text-[11px] font-medium text-stone-600">
                          {credential.status}
                        </span>
                      </div>
                      <p className="mt-0.5 truncate text-[12px] text-stone-500">
                        {credential.email ?? credential.platform} · {shortId(credential.id)}
                      </p>
                    </div>
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
                ))}
              </div>
            </div>
          ))}
        </div>
      </section>

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
                    onChange={(event) => setApiBaseUrl(event.target.value)}
                    value={apiBaseUrl}
                  />
                </label>
                <label className={labelClass}>
                  接口格式
                  <select
                    aria-label="接口格式"
                    className={fieldClass}
                    onChange={(event) => setApiInterfaceFormat(event.target.value as InterfaceFormat)}
                    value={apiInterfaceFormat}
                  >
                    {interfaceFormats.map((format) => (
                      <option key={format} value={format}>
                        {format}
                      </option>
                    ))}
                  </select>
                </label>
                <ModelMappingsEditor
                  error={apiMappingsError}
                  label="模型映射"
                  onChange={(next) => {
                    setApiMappings(next);
                    setApiMappingsError(null);
                  }}
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
                {(createMutation.error as Error).message || "新增账号失败。"}
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
          <aside className="m-3 h-[calc(100%-1.5rem)] w-full max-w-lg overflow-y-auto rounded-2xl border border-stone-200 bg-white p-4 shadow-2xl">
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
              <label className={labelClass}>
                邮箱
                <input
                  aria-label="编辑邮箱"
                  className={fieldClass}
                  onChange={(event) => setEditEmail(event.target.value)}
                  value={editEmail}
                />
              </label>
              <label className={labelClass}>
                状态
                <select
                  aria-label="编辑状态"
                  className={fieldClass}
                  onChange={(event) => setEditStatus(event.target.value as AccountStatus)}
                  value={editStatus}
                >
                  <option value="ok">ok</option>
                  <option value="warning">warning</option>
                  <option value="error">error</option>
                </select>
              </label>
              <label className={labelClass}>
                Secret JSON
                <textarea
                  aria-label="编辑 Secret JSON"
                  className={`${monoFieldClass} min-h-24`}
                  onChange={(event) => setEditSecretJson(event.target.value)}
                  value={editSecretJson}
                />
              </label>
              {editingCredential.kind === "api" && (
                <ModelMappingsEditor
                  error={editModelMappingsError}
                  label="模型映射"
                  onChange={(next) => {
                    setEditModelMappings(next);
                    setEditModelMappingsError(null);
                  }}
                  platform={activePlatform}
                  value={editModelMappings}
                />
              )}
              <label className={labelClass}>
                Config JSON
                <textarea
                  aria-label="编辑 Config JSON"
                  className={`${monoFieldClass} min-h-24`}
                  onChange={(event) => {
                    setEditConfigJson(event.target.value);
                    setEditModelMappings(parseModelMappingsFromConfig(event.target.value));
                    setEditModelMappingsError(null);
                  }}
                  value={editConfigJson}
                />
              </label>
              <label className={labelClass}>
                Preview JSON
                <textarea
                  aria-label="编辑 Preview JSON"
                  className={`${monoFieldClass} min-h-24`}
                  onChange={(event) => setEditPreviewJson(event.target.value)}
                  value={editPreviewJson}
                />
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
