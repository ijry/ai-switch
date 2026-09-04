import {
  AlertTriangle,
  Check,
  CheckCircle2,
  FileJson,
  FileUp,
  LoaderCircle,
  RotateCcw,
  Upload,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { importRouteCredentials, previewRouteCredentialImport } from "../../lib/api/client";
import type {
  ImportRouteCredentialsInput,
  PreviewRouteCredentialImportInput,
  RouteCredentialImportOutcome,
  RouteCredentialImportPreview,
  RouteCredentialImportPreviewItem,
  TransferPlatformChoice,
} from "../../lib/api/types";

export type RouteCredentialImportDialogProps = {
  open: boolean;
  onClose: () => void;
  onImported: (outcome: RouteCredentialImportOutcome) => void;
};

const MAX_SOURCE_BYTES = 8 * 1024 * 1024;
const platformOptions = [
  { value: "codex", label: "Codex" },
  { value: "claude", label: "Claude" },
  { value: "gemini", label: "Gemini" },
  { value: "grok", label: "Grok" },
  { value: "opencode", label: "OpenCode" },
  { value: "openclaw", label: "OpenClaw" },
  { value: "hermes", label: "Hermes" },
] as const;
const interfaceOptions = [
  { value: "", label: "自动判断" },
  { value: "openai", label: "OpenAI" },
  { value: "openai-responses", label: "OpenAI Responses" },
  { value: "anthropic", label: "Anthropic" },
  { value: "gemini", label: "Gemini" },
] as const;

type ChoiceMap = Record<string, TransferPlatformChoice>;
type ImportStage = "input" | "preview" | "complete";

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (error && typeof error === "object") {
    const record = error as { message?: unknown; details?: unknown; code?: unknown };
    for (const value of [record.message, record.details, record.code]) {
      if (typeof value === "string" && value.trim()) {
        return value;
      }
    }
  }
  return typeof error === "string" && error.trim() ? error : "导入预览失败。";
}

function utf8ByteLength(text: string): number {
  if (typeof TextEncoder !== "undefined") {
    return new TextEncoder().encode(text).byteLength;
  }
  return new Blob([text]).size;
}

function validateSourceText(text: string): string | null {
  if (utf8ByteLength(text) > MAX_SOURCE_BYTES) {
    return "JSON 文件不能超过 8 MiB。";
  }
  return null;
}

async function readJsonFile(file: File): Promise<string> {
  const lowerName = file.name.toLowerCase();
  if (!lowerName.endsWith(".json") && file.type !== "application/json") {
    throw new Error("请选择 JSON 文件。");
  }
  if (file.size > MAX_SOURCE_BYTES) {
    throw new Error("JSON 文件不能超过 8 MiB。");
  }
  const bytes = typeof file.arrayBuffer === "function"
    ? await file.arrayBuffer()
    : await new Promise<ArrayBuffer>((resolve, reject) => {
      const reader = new FileReader();
      reader.onerror = () => reject(new Error("无法读取 JSON 文件。"));
      reader.onload = () => {
        if (reader.result instanceof ArrayBuffer) {
          resolve(reader.result);
        } else {
          reject(new Error("无法读取 JSON 文件。"));
        }
      };
      reader.readAsArrayBuffer(file);
    });
  if (bytes.byteLength > MAX_SOURCE_BYTES) {
    throw new Error("JSON 文件不能超过 8 MiB。");
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error("文件不是有效的 UTF-8 JSON。");
  }
}

function choiceKey(choice: TransferPlatformChoice): string {
  return `${choice.item_index}:${choice.platform}:${choice.interface_format ?? ""}`;
}

function choicesAsArray(choices: ChoiceMap): TransferPlatformChoice[] {
  return Object.values(choices)
    .filter((choice) => choice.platform.trim())
    .sort((left, right) => left.item_index - right.item_index)
    .map((choice) => ({
      item_index: choice.item_index,
      platform: choice.platform,
      interface_format: choice.interface_format?.trim() || null,
    }));
}

function issueLabel(code: string): string {
  const labels: Record<string, string> = {
    "transfer.choice_required": "需要选择平台",
    "transfer.input_duplicate": "输入内容重复",
    "transfer.source_duplicate": "来源中已存在",
    "transfer.possible_duplicate": "可能已存在",
    "transfer.conflict": "来源内容冲突",
    "transfer.unknown_platform": "平台无法识别",
  };
  return labels[code] ?? code;
}

function dispositionLabel(disposition: string): string {
  switch (disposition) {
    case "import":
      return "可导入";
    case "possible_duplicate":
      return "可能重复";
    case "input_duplicate":
      return "输入重复";
    case "source_duplicate":
      return "来源重复";
    case "conflict":
      return "冲突";
    case "error":
      return "需修正";
    default:
      return disposition;
  }
}

function dispositionClass(disposition: string): string {
  switch (disposition) {
    case "import":
      return "border-emerald-200 bg-emerald-50 text-emerald-800";
    case "possible_duplicate":
      return "border-amber-200 bg-amber-50 text-amber-800";
    case "input_duplicate":
    case "source_duplicate":
      return "border-stone-200 bg-stone-100 text-stone-700";
    case "conflict":
      return "border-orange-200 bg-orange-50 text-orange-800";
    default:
      return "border-red-200 bg-red-50 text-red-800";
  }
}

function isChoiceRequired(item: RouteCredentialImportPreviewItem): boolean {
  return item.issue_codes.includes("transfer.choice_required");
}

function previewRequestKey(text: string, choices: ChoiceMap): string {
  return `${text}\u0000${choicesAsArray(choices).map(choiceKey).join("\u0001")}`;
}

function countLabel(preview: RouteCredentialImportPreview): string {
  const counts = preview.counts;
  return `${counts.total} 项 · 可导入 ${counts.importable} · 重复 ${counts.duplicates} · 冲突 ${counts.conflicts} · 错误 ${counts.errors}`;
}

function PreviewRow({
  item,
  choices,
  onChoiceChange,
}: {
  item: RouteCredentialImportPreviewItem;
  choices: ChoiceMap;
  onChoiceChange: (itemIndex: number, field: "platform" | "interface_format", value: string) => void;
}) {
  const choice = choices[String(item.item_index)];
  return (
    <li className="border-b border-stone-200 px-3 py-2.5 last:border-b-0">
      <div className="flex min-w-0 items-start gap-2">
        <span className="mt-0.5 w-8 shrink-0 font-mono text-[11px] text-stone-400">#{item.item_index + 1}</span>
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
            <span className="min-w-0 truncate text-[12px] font-semibold text-stone-900">{item.display_name_masked}</span>
            {item.platform ? <span className="font-mono text-[10px] text-stone-500">{item.platform}</span> : null}
            {item.kind ? <span className="font-mono text-[10px] text-stone-500">{item.kind}</span> : null}
            {item.cpa_section ? <span className="font-mono text-[10px] text-stone-500">{item.cpa_section}</span> : null}
          </div>
          {isChoiceRequired(item) ? (
            <div className="mt-2 grid gap-2 sm:grid-cols-2">
              <label className="grid gap-1 text-[10px] font-semibold uppercase tracking-wide text-stone-500">
                平台
                <select
                  aria-label={`第 ${item.item_index + 1} 项平台`}
                  className="h-8 rounded border border-stone-300 bg-white px-2 text-[12px] font-normal normal-case tracking-normal text-stone-900 outline-none focus:border-stone-500 focus:ring-1 focus:ring-stone-300"
                  onChange={(event) => onChoiceChange(item.item_index, "platform", event.target.value)}
                  value={choice?.platform ?? ""}
                >
                  <option value="">请选择</option>
                  {platformOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
                </select>
              </label>
              <label className="grid gap-1 text-[10px] font-semibold uppercase tracking-wide text-stone-500">
                接口
                <select
                  aria-label={`第 ${item.item_index + 1} 项接口格式`}
                  className="h-8 rounded border border-stone-300 bg-white px-2 text-[12px] font-normal normal-case tracking-normal text-stone-900 outline-none focus:border-stone-500 focus:ring-1 focus:ring-stone-300"
                  onChange={(event) => onChoiceChange(item.item_index, "interface_format", event.target.value)}
                  value={choice?.interface_format ?? ""}
                >
                  {interfaceOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
                </select>
              </label>
            </div>
          ) : null}
          {item.issue_codes.length ? (
            <div className="mt-1 flex flex-wrap gap-x-2 gap-y-1 text-[10px] text-stone-500">
              {item.issue_codes.map((code) => <span key={code}>{issueLabel(code)}</span>)}
            </div>
          ) : null}
        </div>
        <span className={`shrink-0 rounded border px-1.5 py-0.5 text-[10px] font-semibold ${dispositionClass(item.disposition)}`}>
          {dispositionLabel(item.disposition)}
        </span>
      </div>
    </li>
  );
}

export function RouteCredentialImportDialog({ open, onClose, onImported }: RouteCredentialImportDialogProps) {
  const [sourceText, setSourceText] = useState("");
  const [sourceFileName, setSourceFileName] = useState<string | null>(null);
  const [choices, setChoices] = useState<ChoiceMap>({});
  const [preview, setPreview] = useState<RouteCredentialImportPreview | null>(null);
  const [previewKey, setPreviewKey] = useState<string | null>(null);
  const [previewPending, setPreviewPending] = useState(false);
  const [restorePoolMembership, setRestorePoolMembership] = useState(false);
  const [outcome, setOutcome] = useState<RouteCredentialImportOutcome | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [stage, setStage] = useState<ImportStage>("input");
  const requestSequenceRef = useRef(0);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const previousOpenRef = useRef(false);

  const choicesList = useMemo(() => choicesAsArray(choices), [choices]);
  const requestKey = useMemo(() => previewRequestKey(sourceText, choices), [choices, sourceText]);
  const sourceSizeError = useMemo(() => validateSourceText(sourceText), [sourceText]);
  const choiceRequired = preview?.items.some(isChoiceRequired) ?? false;
  const previewMatchesInput = preview != null && previewKey === requestKey;
  const canConfirm = Boolean(
    previewMatchesInput &&
      preview &&
      !choiceRequired &&
      preview.counts.importable > 0 &&
      !previewPending &&
      !sourceSizeError,
  );

  function clearSensitiveState() {
    requestSequenceRef.current += 1;
    setSourceText("");
    setSourceFileName(null);
    setChoices({});
    setPreview(null);
    setPreviewKey(null);
    setPreviewPending(false);
    setRestorePoolMembership(false);
    setOutcome(null);
    setError(null);
    setStage("input");
  }

  useEffect(() => {
    if (!open) {
      if (previousOpenRef.current) {
        clearSensitiveState();
      }
      previousOpenRef.current = false;
      return;
    }
    previousOpenRef.current = true;
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    closeButtonRef.current?.focus();
    return () => {
      if (previousFocus?.isConnected) {
        previousFocus.focus();
      }
    };
  }, [open]);

  useEffect(() => {
    if (!open || stage === "complete" || !sourceText) {
      return;
    }
    const sizeError = validateSourceText(sourceText);
    const sequence = ++requestSequenceRef.current;
    setPreview(null);
    setPreviewKey(null);
    setError(sizeError);
    if (sizeError) {
      setPreviewPending(false);
      return;
    }
    setPreviewPending(true);
    const input: PreviewRouteCredentialImportInput = {
      text: sourceText,
      ambiguous_platform_choices: choicesList,
    };
    void previewRouteCredentialImport(input).then(
      (nextPreview) => {
        if (sequence !== requestSequenceRef.current) return;
        setPreview(nextPreview);
        setPreviewKey(requestKey);
        setPreviewPending(false);
        setError(null);
        setStage("preview");
      },
      (nextError: unknown) => {
        if (sequence !== requestSequenceRef.current) return;
        setPreviewPending(false);
        setError(errorMessage(nextError));
        setStage("input");
      },
    );
    return () => {
      if (sequence === requestSequenceRef.current) {
        requestSequenceRef.current += 1;
      }
    };
  }, [choicesList, open, requestKey, sourceText, stage]);

  useEffect(() => {
    if (!open) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab") return;
      const dialog = dialogRef.current;
      if (!dialog) return;
      const focusable = Array.from(dialog.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ));
      const first = focusable[0];
      const last = focusable.at(-1);
      if (!first || !last) {
        event.preventDefault();
        dialog.focus();
      } else if (!dialog.contains(document.activeElement)) {
        event.preventDefault();
        first.focus();
      } else if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose, open]);

  if (!open) return null;

  function handleClose() {
    clearSensitiveState();
    onClose();
  }

  function updateChoice(itemIndex: number, field: "platform" | "interface_format", value: string) {
    setChoices((current) => {
      const key = String(itemIndex);
      const previous = current[key] ?? { item_index: itemIndex, platform: "", interface_format: null };
      const next = { ...previous, [field]: value || null };
      return next.platform ? { ...current, [key]: next } : { ...current, [key]: next };
    });
  }

  async function handleFileChange(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0] ?? null;
    event.target.value = "";
    if (!file) return;
    try {
      const text = await readJsonFile(file);
      setSourceText(text);
      setSourceFileName(file.name || "credentials.json");
      setChoices({});
      setError(null);
      setStage("input");
    } catch (nextError) {
      setSourceText("");
      setSourceFileName(null);
      setPreview(null);
      setPreviewKey(null);
      setError(errorMessage(nextError));
      setStage("input");
    }
  }

  async function handleConfirm() {
    if (!canConfirm || !preview) return;
    setError(null);
    setPreviewPending(true);
    const input: ImportRouteCredentialsInput = {
      text: sourceText,
      ambiguous_platform_choices: choicesList,
      restore_pool_membership: restorePoolMembership,
    };
    try {
      const nextOutcome = await importRouteCredentials(input);
      setOutcome(nextOutcome);
      setSourceText("");
      setSourceFileName(null);
      setChoices({});
      setPreview(null);
      setPreviewKey(null);
      setPreviewPending(false);
      setStage("complete");
      onImported(nextOutcome);
    } catch (nextError) {
      setPreviewPending(false);
      setError(errorMessage(nextError));
    }
  }

  const title = stage === "complete" ? "账号导入完成" : stage === "preview" ? "确认导入账号" : "导入账号";
  return (
    <div
      className="motion-overlay fixed inset-0 z-[80] flex items-center justify-center bg-stone-950/45 p-3"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) handleClose();
      }}
    >
      <div
        ref={dialogRef}
        aria-labelledby="route-credential-import-title"
        aria-modal="true"
        className="flex max-h-[min(760px,calc(100vh-1.5rem))] w-full max-w-3xl flex-col overflow-hidden rounded-md border border-stone-400 bg-stone-50 shadow-2xl"
        role="dialog"
        tabIndex={-1}
      >
        <header className="flex h-11 shrink-0 items-center justify-between gap-3 border-b border-stone-300 bg-stone-100 px-3">
          <div className="flex min-w-0 items-center gap-2">
            <FileJson aria-hidden="true" className="h-4 w-4 shrink-0 text-stone-600" />
            <h2 id="route-credential-import-title" className="truncate text-sm font-semibold text-stone-950">{title}</h2>
          </div>
          <button
            ref={closeButtonRef}
            aria-label="关闭导入账号"
            className="grid h-7 w-7 shrink-0 place-items-center border border-stone-300 bg-white text-stone-600 hover:bg-stone-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-stone-500"
            onClick={handleClose}
            title="关闭"
            type="button"
          >
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
          {stage === "complete" && outcome ? (
            <div className="mx-auto grid max-w-lg gap-4 py-8 text-center">
              <CheckCircle2 aria-hidden="true" className="mx-auto h-10 w-10 text-emerald-600" />
              <div>
                <h3 className="text-base font-semibold text-stone-950">已完成导入</h3>
                <p className="mt-1 text-xs text-stone-500">导入结果已写入本机账号库。</p>
              </div>
              <dl className="grid grid-cols-2 gap-px overflow-hidden border border-stone-300 bg-stone-300 text-left text-xs sm:grid-cols-4">
                {[
                  ["已导入", outcome.imported],
                  ["跳过重复", outcome.skipped_duplicates],
                  ["冲突", outcome.conflicts],
                  ["失败", outcome.failed],
                ].map(([label, value]) => (
                  <div className="bg-white px-2.5 py-2" key={label as string}>
                    <dt className="text-stone-500">{label}</dt>
                    <dd className="mt-0.5 font-mono font-semibold text-stone-950">{value}</dd>
                  </div>
                ))}
              </dl>
              <p className="text-xs text-stone-500">恢复入池：{outcome.restored_pool_members}</p>
            </div>
          ) : (
            <div className="grid min-h-0 gap-3">
              <div className="grid gap-2 border border-stone-300 bg-white p-3">
                <div className="flex items-center justify-between gap-2">
                  <label className="text-xs font-semibold text-stone-800" htmlFor="route-credential-import-source">JSON 数组</label>
                  <span className="font-mono text-[10px] text-stone-500">最大 8 MiB</span>
                </div>
                <textarea
                  aria-label="账号 JSON"
                  className="min-h-32 resize-y rounded border border-stone-300 bg-stone-50 p-2 font-mono text-[11px] leading-5 text-stone-900 outline-none focus:border-stone-500 focus:ring-1 focus:ring-stone-300"
                  id="route-credential-import-source"
                  onChange={(event) => {
                    setSourceText(event.target.value);
                    setSourceFileName(null);
                    setChoices({});
                    setError(null);
                    setStage("input");
                  }}
                  placeholder="粘贴 CPA / AI Switch JSON 数组"
                  spellCheck={false}
                  value={sourceText}
                />
                <div className="flex flex-wrap items-center justify-between gap-2 text-[10px] text-stone-500">
                  <span>{sourceFileName ? `文件：${sourceFileName}` : "仅接受裸 JSON 数组，不接受外层包装对象"}</span>
                  <button
                    aria-label="选择 JSON 文件"
                    className="inline-flex h-7 items-center gap-1 border border-stone-300 bg-white px-2 text-[11px] font-semibold text-stone-700 hover:bg-stone-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-stone-500"
                    onClick={() => fileInputRef.current?.click()}
                    type="button"
                  >
                    <FileUp aria-hidden="true" className="h-3.5 w-3.5" />
                    选择文件
                  </button>
                  <input
                    ref={fileInputRef}
                    accept=".json,application/json"
                    aria-label="选择账号 JSON 文件"
                    className="sr-only"
                    onChange={(event) => void handleFileChange(event)}
                    type="file"
                  />
                </div>
              </div>

              {error ? <p aria-live="assertive" className="border border-red-300 bg-red-50 px-3 py-2 text-xs text-red-800" role="alert">{error}</p> : null}

              {previewPending && !preview ? (
                <p aria-live="polite" className="flex items-center gap-2 border border-stone-300 bg-white px-3 py-2 text-xs text-stone-600">
                  <LoaderCircle aria-hidden="true" className="h-3.5 w-3.5 animate-spin" />
                  正在检查账号内容...
                </p>
              ) : null}

              {preview ? (
                <div className="grid min-h-0 gap-3">
                  <div className="grid gap-2 border border-stone-300 bg-white px-3 py-2.5">
                    <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-stone-700">
                      <span className="font-semibold">预览结果</span>
                      <span className="font-mono text-[10px]">{countLabel(preview)}</span>
                    </div>
                    <div className="grid grid-cols-2 gap-1 text-[10px] text-stone-500 sm:grid-cols-4">
                      <span>官方 {preview.counts.official}</span>
                      <span>API {preview.counts.api}</span>
                      <span>批量 {preview.counts.batch_count}</span>
                      <span>可恢复入池 {preview.counts.restorable_pool_count}</span>
                    </div>
                  </div>

                  {preview.counts.restorable_pool_count > 0 ? (
                    <label className="flex items-start gap-2 border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-950">
                      <input
                        aria-label="恢复算力池成员"
                        checked={restorePoolMembership}
                        className="mt-0.5 accent-stone-900"
                        onChange={(event) => setRestorePoolMembership(event.target.checked)}
                        type="checkbox"
                      />
                      <span>
                        <span className="block font-semibold">恢复算力池成员</span>
                        <span className="mt-0.5 block text-[10px] text-amber-800">
                          仅追加本次新导入账号，默认关闭；不会覆盖现有算力池顺序。{` `}
                          {Object.entries(preview.counts.restorable_pool_counts).map(([platform, count], index) => (
                            <span key={platform}>{index ? " · " : ""}{platform} {count}</span>
                          ))}
                        </span>
                      </span>
                    </label>
                  ) : null}

                  <div className="min-h-0 overflow-hidden border border-stone-300 bg-white">
                    <div className="flex items-center gap-2 border-b border-stone-300 bg-stone-100 px-3 py-2 text-[10px] font-semibold uppercase tracking-wide text-stone-600">
                      <Check aria-hidden="true" className="h-3.5 w-3.5" />
                      逐项检查
                    </div>
                    <ul className="max-h-72 overflow-y-auto">
                      {preview.items.map((item) => (
                        <PreviewRow item={item} key={item.item_index} choices={choices} onChoiceChange={updateChoice} />
                      ))}
                    </ul>
                  </div>
                  {choiceRequired ? <p className="text-[11px] text-amber-800">请为标记为“需要选择平台”的项目选择平台后继续。</p> : null}
                </div>
              ) : null}
            </div>
          )}
        </div>

        <footer className="flex min-h-10 shrink-0 items-center justify-between gap-2 border-t border-stone-300 bg-stone-100 px-3 py-2">
          <div className="min-w-0 truncate text-[10px] text-stone-500" aria-live="polite">
            {stage === "complete" ? "可以关闭此窗口。" : stage === "preview" && preview ? (canConfirm ? "检查完成，可以确认导入。" : "请处理预览中的问题。") : "粘贴内容或选择 JSON 文件。"}
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            {stage === "complete" ? (
              <button className="inline-flex h-7 items-center gap-1 border border-stone-400 bg-stone-800 px-2.5 text-[11px] font-semibold text-white hover:bg-stone-700 focus:outline-none focus-visible:ring-2 focus-visible:ring-stone-500" onClick={handleClose} type="button">
                完成
              </button>
            ) : (
              <>
                <button className="inline-flex h-7 items-center gap-1 border border-stone-300 bg-white px-2.5 text-[11px] font-semibold text-stone-700 hover:bg-stone-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-stone-500" onClick={handleClose} type="button">
                  取消
                </button>
                <button
                  aria-label="确认导入账号"
                  className="inline-flex h-7 items-center gap-1 border border-stone-700 bg-stone-800 px-2.5 text-[11px] font-semibold text-white hover:bg-stone-700 disabled:cursor-not-allowed disabled:opacity-40 focus:outline-none focus-visible:ring-2 focus-visible:ring-stone-500"
                  disabled={!canConfirm || previewPending}
                  onClick={() => void handleConfirm()}
                  type="button"
                >
                  {previewPending ? <LoaderCircle aria-hidden="true" className="h-3.5 w-3.5 animate-spin" /> : <Upload aria-hidden="true" className="h-3.5 w-3.5" />}
                  确认导入
                </button>
              </>
            )}
          </div>
        </footer>
      </div>
    </div>
  );
}
