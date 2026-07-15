# AI Switch System Utilities Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two local system utilities: reversible text encoding/decoding and lightweight offline English/number OCR.

**Architecture:** Keep both utilities in the existing React/Vite front end and register them in the existing in-memory screen switcher. Put pure transform logic and OCR integration behind small `src/lib` modules so screens stay focused on UI state and validation.

**Tech Stack:** React 18, TypeScript, Vite, Vitest, Testing Library, UnoCSS utility classes, `ocrad.js` for offline browser-side OCR.

## Global Constraints

- Work directly on `main`; do not create or switch branches/worktrees.
- The utilities are local-only. They do not call the app backend, external APIs, or network services.
- OCR must run from bundled app assets after install and must not fetch language data at recognition time.
- Chinese OCR is out of scope for this iteration.
- New labels, hints, buttons, and error messages must be added to both English and Simplified Chinese dictionaries.
- Do not overwrite unrelated existing worktree changes.

---

## File Structure

- Create `src/lib/cryptoTransforms.ts`: pure reversible transform functions and validation errors.
- Create `src/screens/CryptoToolsScreen.tsx`: UI for selecting a transform, entering text, viewing output, and copying results.
- Create `src/lib/ocr/recognizeText.ts`: `ocrad.js` wrapper that accepts an image element or canvas and returns trimmed text.
- Create `src/types/ocrad-js.d.ts`: local TypeScript declaration for the untyped `ocrad.js` package.
- Create `src/screens/OcrScreen.tsx`: image picker, preview, recognition trigger, status, errors, and text output.
- Modify `package.json` and `pnpm-lock.yaml`: add `ocrad.js`.
- Modify `src/components/layout/AppLayout.tsx`: add `CryptoTools` and `OCR` system nav entries.
- Modify `src/App.tsx`: import and register both new screens.
- Modify `src/lib/i18n.tsx`: add English and Simplified Chinese strings for nav and screen UI.
- Create `tests/CryptoToolsScreen.test.tsx`: verify Base64 success and invalid Hex validation.
- Create `tests/OcrScreen.test.tsx`: mock OCR wrapper and verify file selection plus recognition output.
- Create `tests/AppLayout.test.tsx`: verify system navigation renders the two new entries and emits screen names.

---

### Task 1: Crypto Transform Logic And Screen

**Files:**
- Create: `src/lib/cryptoTransforms.ts`
- Create: `src/screens/CryptoToolsScreen.tsx`
- Create: `tests/CryptoToolsScreen.test.tsx`
- Modify: `src/lib/i18n.tsx`

**Interfaces:**
- Produces: `type CryptoOperation = "base64-encode" | "base64-decode" | "url-encode" | "url-decode" | "hex-encode" | "hex-decode"`
- Produces: `function transformCryptoText(input: string, operation: CryptoOperation): { output: string; error: string | null }`
- Consumes: existing `useI18n()`.

- [ ] **Step 1: Write the failing crypto screen tests**

Create `tests/CryptoToolsScreen.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "../src/lib/i18n";
import { CryptoToolsScreen } from "../src/screens/CryptoToolsScreen";

function renderScreen() {
  return render(
    <I18nProvider initialLanguage="zh-CN">
      <CryptoToolsScreen />
    </I18nProvider>,
  );
}

describe("CryptoToolsScreen", () => {
  it("encodes UTF-8 text as Base64", async () => {
    renderScreen();

    await userEvent.type(screen.getByLabelText("输入文本"), "hello 世界");
    expect(screen.getByLabelText("输出文本")).toHaveValue("aGVsbG8g5LiW55WM");
  });

  it("shows a validation error for invalid hex input", async () => {
    renderScreen();

    await userEvent.selectOptions(screen.getByLabelText("转换方式"), "hex-decode");
    await userEvent.type(screen.getByLabelText("输入文本"), "abc");

    expect(screen.getByText("Hex 内容必须是偶数长度。")).toBeInTheDocument();
    expect(screen.getByLabelText("输出文本")).toHaveValue("");
  });
});
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `pnpm vitest run tests/CryptoToolsScreen.test.tsx`

Expected: fail because `CryptoToolsScreen` does not exist.

- [ ] **Step 3: Implement the pure transform module**

Create `src/lib/cryptoTransforms.ts`:

```ts
export type CryptoOperation =
  | "base64-encode"
  | "base64-decode"
  | "url-encode"
  | "url-decode"
  | "hex-encode"
  | "hex-decode";

export type CryptoTransformResult = {
  output: string;
  error: "invalid-base64" | "invalid-url" | "invalid-hex-length" | "invalid-hex" | null;
};

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder("utf-8", { fatal: true });

function bytesToBase64(bytes: Uint8Array) {
  let binary = "";
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return btoa(binary);
}

function base64ToBytes(input: string) {
  const normalized = input.replace(/\s+/g, "");
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(normalized) || normalized.length % 4 === 1) {
    throw new Error("invalid-base64");
  }
  const binary = atob(normalized);
  return Uint8Array.from(binary, (char) => char.charCodeAt(0));
}

function bytesToHex(bytes: Uint8Array) {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function hexToBytes(input: string) {
  const normalized = input.replace(/\s+/g, "");
  if (normalized.length % 2 !== 0) {
    throw new Error("invalid-hex-length");
  }
  if (!/^[0-9a-fA-F]*$/.test(normalized)) {
    throw new Error("invalid-hex");
  }
  const bytes = new Uint8Array(normalized.length / 2);
  for (let index = 0; index < normalized.length; index += 2) {
    bytes[index / 2] = Number.parseInt(normalized.slice(index, index + 2), 16);
  }
  return bytes;
}

export function transformCryptoText(input: string, operation: CryptoOperation): CryptoTransformResult {
  try {
    if (operation === "base64-encode") {
      return { output: bytesToBase64(textEncoder.encode(input)), error: null };
    }
    if (operation === "base64-decode") {
      return { output: textDecoder.decode(base64ToBytes(input)), error: null };
    }
    if (operation === "url-encode") {
      return { output: encodeURIComponent(input), error: null };
    }
    if (operation === "url-decode") {
      return { output: decodeURIComponent(input), error: null };
    }
    if (operation === "hex-encode") {
      return { output: bytesToHex(textEncoder.encode(input)), error: null };
    }
    return { output: textDecoder.decode(hexToBytes(input)), error: null };
  } catch (error) {
    const message = error instanceof Error ? error.message : "";
    if (message === "invalid-hex-length") {
      return { output: "", error: "invalid-hex-length" };
    }
    if (message === "invalid-hex") {
      return { output: "", error: "invalid-hex" };
    }
    if (operation === "base64-decode") {
      return { output: "", error: "invalid-base64" };
    }
    if (operation === "url-decode") {
      return { output: "", error: "invalid-url" };
    }
    return { output: "", error: "invalid-hex" };
  }
}
```

- [ ] **Step 4: Add crypto i18n strings**

Modify `src/lib/i18n.tsx` by adding these keys to `en` and matching Simplified Chinese strings to `zh`:

```ts
"nav.cryptoTools": "Crypto Tools",
"crypto.kicker": "System utility",
"crypto.title": "Crypto Tools",
"crypto.subtitle": "Encode and decode reversible text formats locally.",
"crypto.input": "Input text",
"crypto.inputPlaceholder": "Paste text to transform",
"crypto.operation": "Operation",
"crypto.output": "Output text",
"crypto.copy": "Copy output",
"crypto.copied": "Copied",
"crypto.base64Encode": "Base64 encode",
"crypto.base64Decode": "Base64 decode",
"crypto.urlEncode": "URL encode",
"crypto.urlDecode": "URL decode",
"crypto.hexEncode": "Hex encode",
"crypto.hexDecode": "Hex decode",
"crypto.error.invalidBase64": "Base64 input is invalid.",
"crypto.error.invalidUrl": "URL escapes are invalid.",
"crypto.error.invalidHexLength": "Hex content must have an even length.",
"crypto.error.invalidHex": "Hex content can only contain 0-9 and A-F.",
```

Chinese values:

```ts
"nav.cryptoTools": "加解密",
"crypto.kicker": "系统工具",
"crypto.title": "加解密",
"crypto.subtitle": "在本地进行可逆文本编码和解码。",
"crypto.input": "输入文本",
"crypto.inputPlaceholder": "粘贴要转换的文本",
"crypto.operation": "转换方式",
"crypto.output": "输出文本",
"crypto.copy": "复制输出",
"crypto.copied": "已复制",
"crypto.base64Encode": "Base64 编码",
"crypto.base64Decode": "Base64 解码",
"crypto.urlEncode": "URL 编码",
"crypto.urlDecode": "URL 解码",
"crypto.hexEncode": "Hex 编码",
"crypto.hexDecode": "Hex 解码",
"crypto.error.invalidBase64": "Base64 内容无效。",
"crypto.error.invalidUrl": "URL 转义内容无效。",
"crypto.error.invalidHexLength": "Hex 内容必须是偶数长度。",
"crypto.error.invalidHex": "Hex 内容只能包含 0-9 和 A-F。",
```

- [ ] **Step 5: Implement `CryptoToolsScreen`**

Create `src/screens/CryptoToolsScreen.tsx`:

```tsx
import { useMemo, useState } from "react";
import { Copy } from "lucide-react";
import { transformCryptoText, type CryptoOperation } from "../lib/cryptoTransforms";
import { useI18n } from "../lib/i18n";

const operations: Array<{ value: CryptoOperation; labelKey: Parameters<ReturnType<typeof useI18n>["t"]>[0] }> = [
  { value: "base64-encode", labelKey: "crypto.base64Encode" },
  { value: "base64-decode", labelKey: "crypto.base64Decode" },
  { value: "url-encode", labelKey: "crypto.urlEncode" },
  { value: "url-decode", labelKey: "crypto.urlDecode" },
  { value: "hex-encode", labelKey: "crypto.hexEncode" },
  { value: "hex-decode", labelKey: "crypto.hexDecode" },
];

const errorKeys = {
  "invalid-base64": "crypto.error.invalidBase64",
  "invalid-url": "crypto.error.invalidUrl",
  "invalid-hex-length": "crypto.error.invalidHexLength",
  "invalid-hex": "crypto.error.invalidHex",
} as const;

export function CryptoToolsScreen() {
  const { t } = useI18n();
  const [input, setInput] = useState("");
  const [operation, setOperation] = useState<CryptoOperation>("base64-encode");
  const [copied, setCopied] = useState(false);
  const result = useMemo(() => transformCryptoText(input, operation), [input, operation]);

  const copyOutput = async () => {
    if (!result.output || typeof navigator === "undefined" || !navigator.clipboard) {
      return;
    }
    await navigator.clipboard.writeText(result.output);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1400);
  };

  return (
    <section className="space-y-3">
      <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">{t("crypto.kicker")}</p>
        <h1 className="mt-0.5 text-lg font-semibold tracking-tight text-stone-950">{t("crypto.title")}</h1>
        <p className="mt-1 text-[13px] text-stone-500">{t("crypto.subtitle")}</p>
      </div>

      <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_260px]">
        <label className="flex flex-col gap-1.5 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
          <span className="text-[12px] font-semibold text-stone-600">{t("crypto.input")}</span>
          <textarea
            aria-label={t("crypto.input")}
            className="min-h-64 resize-y rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
            onChange={(event) => setInput(event.target.value)}
            placeholder={t("crypto.inputPlaceholder")}
            value={input}
          />
        </label>

        <label className="flex h-fit flex-col gap-1.5 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
          <span className="text-[12px] font-semibold text-stone-600">{t("crypto.operation")}</span>
          <select
            aria-label={t("crypto.operation")}
            className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-medium text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
            onChange={(event) => setOperation(event.target.value as CryptoOperation)}
            value={operation}
          >
            {operations.map((item) => (
              <option key={item.value} value={item.value}>
                {t(item.labelKey)}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
        <div className="mb-2 flex items-center justify-between gap-2">
          <label className="text-[12px] font-semibold text-stone-600" htmlFor="crypto-output">
            {t("crypto.output")}
          </label>
          <button
            className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50 disabled:opacity-50"
            disabled={!result.output}
            onClick={copyOutput}
            type="button"
          >
            <Copy className="h-3.5 w-3.5" />
            {copied ? t("crypto.copied") : t("crypto.copy")}
          </button>
        </div>
        <textarea
          aria-label={t("crypto.output")}
          className="min-h-48 w-full resize-y rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-900 outline-none"
          id="crypto-output"
          readOnly
          value={result.output}
        />
        {result.error && (
          <p className="mt-2 text-[13px] font-medium text-red-700">{t(errorKeys[result.error])}</p>
        )}
      </div>
    </section>
  );
}
```

- [ ] **Step 6: Run crypto test and typecheck for this task**

Run: `pnpm vitest run tests/CryptoToolsScreen.test.tsx`

Expected: pass.

Run: `pnpm typecheck`

Expected: pass or only fail on unrelated pre-existing files; if it fails on files touched in this task, fix them before continuing.

- [ ] **Step 7: Commit crypto task**

Run:

```powershell
git add src/lib/cryptoTransforms.ts src/screens/CryptoToolsScreen.tsx tests/CryptoToolsScreen.test.tsx src/lib/i18n.tsx
git commit -m "feat: add local crypto tools screen"
```

Expected: commit includes only crypto task files.

---

### Task 2: OCR Dependency, Wrapper, And Screen

**Files:**
- Modify: `package.json`
- Modify: `pnpm-lock.yaml`
- Create: `src/types/ocrad-js.d.ts`
- Create: `src/lib/ocr/recognizeText.ts`
- Create: `src/screens/OcrScreen.tsx`
- Create: `tests/OcrScreen.test.tsx`
- Modify: `src/lib/i18n.tsx`

**Interfaces:**
- Produces: `async function recognizeImageText(source: HTMLImageElement | HTMLCanvasElement): Promise<string>`
- Consumes: `ocrad.js` default export called as `OCRAD(source)`.
- Consumes: existing `useI18n()`.

- [ ] **Step 1: Add the OCR dependency**

Run: `pnpm add ocrad.js`

Expected: `package.json` gains `"ocrad.js": "^0.0.1"` and `pnpm-lock.yaml` updates.

- [ ] **Step 2: Write the failing OCR screen test**

Create `tests/OcrScreen.test.tsx`:

```tsx
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../src/lib/i18n";
import { recognizeImageText } from "../src/lib/ocr/recognizeText";
import { OcrScreen } from "../src/screens/OcrScreen";

vi.mock("../src/lib/ocr/recognizeText", () => ({
  recognizeImageText: vi.fn(),
}));

function renderScreen() {
  return render(
    <I18nProvider initialLanguage="zh-CN">
      <OcrScreen />
    </I18nProvider>,
  );
}

describe("OcrScreen", () => {
  it("loads an image file and shows mocked recognition text", async () => {
    vi.mocked(recognizeImageText).mockResolvedValue("ABCD 1234");
    renderScreen();

    const file = new File(["fake"], "sample.png", { type: "image/png" });
    await userEvent.upload(screen.getByLabelText("选择图片"), file);

    expect(screen.getByText("sample.png")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "开始识别" }));

    await waitFor(() => expect(recognizeImageText).toHaveBeenCalled());
    expect(await screen.findByLabelText("识别结果")).toHaveValue("ABCD 1234");
  });

  it("rejects non-image files", async () => {
    renderScreen();

    const file = new File(["hello"], "notes.txt", { type: "text/plain" });
    await userEvent.upload(screen.getByLabelText("选择图片"), file);

    expect(screen.getByText("请选择图片文件。")).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run the OCR test and verify it fails**

Run: `pnpm vitest run tests/OcrScreen.test.tsx`

Expected: fail because `OcrScreen` and the OCR wrapper do not exist.

- [ ] **Step 4: Add the OCR type declaration and wrapper**

Create `src/types/ocrad-js.d.ts`:

```ts
declare module "ocrad.js" {
  export default function OCRAD(source: HTMLImageElement | HTMLCanvasElement): string;
}
```

Create `src/lib/ocr/recognizeText.ts`:

```ts
import OCRAD from "ocrad.js";

export async function recognizeImageText(source: HTMLImageElement | HTMLCanvasElement): Promise<string> {
  return OCRAD(source).trim();
}
```

- [ ] **Step 5: Add OCR i18n strings**

Modify `src/lib/i18n.tsx` by adding these keys to `en`:

```ts
"nav.ocr": "OCR",
"ocr.kicker": "System utility",
"ocr.title": "OCR Recognition",
"ocr.subtitle": "Recognize English letters and numbers from local images without network access.",
"ocr.limit": "Best for English, numbers, and simple screenshots.",
"ocr.file": "Choose image",
"ocr.fileHint": "PNG, JPEG, GIF, BMP, or WebP image.",
"ocr.noFile": "No image selected.",
"ocr.recognize": "Recognize",
"ocr.recognizing": "Recognizing...",
"ocr.result": "Recognition result",
"ocr.resultPlaceholder": "Recognized text appears here.",
"ocr.error.noFile": "Choose an image before recognition.",
"ocr.error.invalidFile": "Please choose an image file.",
"ocr.error.failed": "OCR recognition failed.",
```

Add these matching Simplified Chinese strings to `zh`:

```ts
"nav.ocr": "OCR识别",
"ocr.kicker": "系统工具",
"ocr.title": "OCR识别",
"ocr.subtitle": "本地识别图片中的英文和数字，不访问网络。",
"ocr.limit": "更适合英文、数字和简单截图。",
"ocr.file": "选择图片",
"ocr.fileHint": "支持 PNG、JPEG、GIF、BMP 或 WebP 图片。",
"ocr.noFile": "尚未选择图片。",
"ocr.recognize": "开始识别",
"ocr.recognizing": "识别中...",
"ocr.result": "识别结果",
"ocr.resultPlaceholder": "识别出的文本会显示在这里。",
"ocr.error.noFile": "请先选择图片再识别。",
"ocr.error.invalidFile": "请选择图片文件。",
"ocr.error.failed": "OCR 识别失败。",
```

- [ ] **Step 6: Implement `OcrScreen`**

Create `src/screens/OcrScreen.tsx`:

```tsx
import { ChangeEvent, useEffect, useRef, useState } from "react";
import { ScanText } from "lucide-react";
import { recognizeImageText } from "../lib/ocr/recognizeText";
import { useI18n } from "../lib/i18n";

type OcrError = "no-file" | "invalid-file" | "failed" | null;

const errorKeys = {
  "no-file": "ocr.error.noFile",
  "invalid-file": "ocr.error.invalidFile",
  failed: "ocr.error.failed",
} as const;

export function OcrScreen() {
  const { t } = useI18n();
  const imageRef = useRef<HTMLImageElement | null>(null);
  const [fileName, setFileName] = useState<string | null>(null);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [result, setResult] = useState("");
  const [error, setError] = useState<OcrError>(null);
  const [recognizing, setRecognizing] = useState(false);

  useEffect(() => {
    return () => {
      if (previewUrl) {
        URL.revokeObjectURL(previewUrl);
      }
    };
  }, [previewUrl]);

  const handleFileChange = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    setResult("");
    if (!file) {
      setFileName(null);
      setPreviewUrl(null);
      setError(null);
      return;
    }
    if (!file.type.startsWith("image/")) {
      setFileName(null);
      setPreviewUrl(null);
      setError("invalid-file");
      return;
    }
    const nextUrl = URL.createObjectURL(file);
    if (previewUrl) {
      URL.revokeObjectURL(previewUrl);
    }
    setFileName(file.name);
    setPreviewUrl(nextUrl);
    setError(null);
  };

  const runRecognition = async () => {
    if (!imageRef.current) {
      setError("no-file");
      return;
    }
    setRecognizing(true);
    setError(null);
    try {
      setResult(await recognizeImageText(imageRef.current));
    } catch {
      setError("failed");
    } finally {
      setRecognizing(false);
    }
  };

  return (
    <section className="space-y-3">
      <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">{t("ocr.kicker")}</p>
        <h1 className="mt-0.5 text-lg font-semibold tracking-tight text-stone-950">{t("ocr.title")}</h1>
        <p className="mt-1 text-[13px] text-stone-500">{t("ocr.subtitle")}</p>
        <p className="mt-2 rounded-xl border border-amber-200 bg-amber-50 px-3 py-2 text-[12px] font-medium text-amber-800">
          {t("ocr.limit")}
        </p>
      </div>

      <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_360px]">
        <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
          <label className="flex flex-col gap-1.5 text-[12px] font-semibold text-stone-600">
            <span>{t("ocr.file")}</span>
            <input
              accept="image/*"
              aria-label={t("ocr.file")}
              className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-medium text-stone-900 file:mr-3 file:rounded-lg file:border-0 file:bg-stone-900 file:px-3 file:py-1.5 file:text-[12px] file:font-semibold file:text-white"
              onChange={handleFileChange}
              type="file"
            />
          </label>
          <p className="mt-2 text-[12px] text-stone-500">{t("ocr.fileHint")}</p>
          <p className="mt-3 text-[13px] font-medium text-stone-700">{fileName ?? t("ocr.noFile")}</p>
          {error && <p className="mt-2 text-[13px] font-medium text-red-700">{t(errorKeys[error])}</p>}
          <button
            className="mt-4 inline-flex items-center gap-2 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800 disabled:opacity-50"
            disabled={recognizing}
            onClick={runRecognition}
            type="button"
          >
            <ScanText className="h-4 w-4" />
            {recognizing ? t("ocr.recognizing") : t("ocr.recognize")}
          </button>
        </div>

        <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
          {previewUrl ? (
            <img
              alt={fileName ?? t("ocr.file")}
              className="max-h-80 w-full rounded-xl border border-stone-200 object-contain"
              ref={imageRef}
              src={previewUrl}
            />
          ) : (
            <div className="grid min-h-60 place-items-center rounded-xl border border-dashed border-stone-300 bg-stone-50 text-[13px] text-stone-500">
              {t("ocr.noFile")}
            </div>
          )}
        </div>
      </div>

      <label className="flex flex-col gap-1.5 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
        <span className="text-[12px] font-semibold text-stone-600">{t("ocr.result")}</span>
        <textarea
          aria-label={t("ocr.result")}
          className="min-h-48 resize-y rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-900 outline-none"
          placeholder={t("ocr.resultPlaceholder")}
          readOnly
          value={result}
        />
      </label>
    </section>
  );
}
```

- [ ] **Step 7: Run OCR test and typecheck for this task**

Run: `pnpm vitest run tests/OcrScreen.test.tsx`

Expected: pass.

Run: `pnpm typecheck`

Expected: pass or only fail on unrelated pre-existing files; if it fails on files touched in this task, fix them before continuing.

- [ ] **Step 8: Commit OCR task**

Run:

```powershell
git add package.json pnpm-lock.yaml src/types/ocrad-js.d.ts src/lib/ocr/recognizeText.ts src/screens/OcrScreen.tsx tests/OcrScreen.test.tsx src/lib/i18n.tsx
git commit -m "feat: add offline OCR screen"
```

Expected: commit includes OCR files and the dependency update.

---

### Task 3: System Navigation And App Registration

**Files:**
- Modify: `src/components/layout/AppLayout.tsx`
- Modify: `src/App.tsx`
- Create: `tests/AppLayout.test.tsx`

**Interfaces:**
- Consumes: `CryptoToolsScreen` from `src/screens/CryptoToolsScreen.tsx`.
- Consumes: `OcrScreen` from `src/screens/OcrScreen.tsx`.
- Produces: system screen names `"CryptoTools"` and `"OCR"`.

- [ ] **Step 1: Write the failing navigation test**

Create `tests/AppLayout.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AppLayout } from "../src/components/layout/AppLayout";
import { I18nProvider } from "../src/lib/i18n";

describe("AppLayout", () => {
  it("renders system utility nav entries and navigates to their screens", async () => {
    const onNavigate = vi.fn();

    render(
      <I18nProvider initialLanguage="zh-CN">
        <AppLayout activeScreen="Codex" onNavigate={onNavigate}>
          <div>content</div>
        </AppLayout>
      </I18nProvider>,
    );

    await userEvent.click(screen.getByRole("button", { name: /加解密/ }));
    await userEvent.click(screen.getByRole("button", { name: /OCR识别/ }));

    expect(onNavigate).toHaveBeenCalledWith("CryptoTools");
    expect(onNavigate).toHaveBeenCalledWith("OCR");
  });
});
```

- [ ] **Step 2: Run the navigation test and verify it fails**

Run: `pnpm vitest run tests/AppLayout.test.tsx`

Expected: fail because the nav entries do not exist.

- [ ] **Step 3: Add system nav entries**

Modify `src/components/layout/AppLayout.tsx`:

```ts
export const settingsFeatureScreens = [
  "Sessions",
  "Updates",
  "Log",
  "CryptoTools",
  "OCR",
] as const;
```

In the system section, render two more `NavButton` components before `Settings`:

```tsx
<NavButton
  active={activeScreen === "CryptoTools"}
  label={t("nav.cryptoTools")}
  onClick={() => onNavigate("CryptoTools")}
/>
<NavButton
  active={activeScreen === "OCR"}
  label={t("nav.ocr")}
  onClick={() => onNavigate("OCR")}
/>
```

- [ ] **Step 4: Register screens in `App.tsx`**

Modify `src/App.tsx` imports:

```ts
import { CryptoToolsScreen } from "./screens/CryptoToolsScreen";
import { OcrScreen } from "./screens/OcrScreen";
```

Add to `implementedScreens`:

```ts
"CryptoTools",
"OCR",
```

Add screen rendering inside `AppLayout`:

```tsx
{screen === "CryptoTools" && <CryptoToolsScreen />}
{screen === "OCR" && <OcrScreen />}
```

- [ ] **Step 5: Run navigation test**

Run: `pnpm vitest run tests/AppLayout.test.tsx`

Expected: pass.

- [ ] **Step 6: Run focused utility tests**

Run:

```powershell
pnpm vitest run tests/CryptoToolsScreen.test.tsx tests/OcrScreen.test.tsx tests/AppLayout.test.tsx
```

Expected: all three pass.

- [ ] **Step 7: Commit navigation task**

Run:

```powershell
git add src/components/layout/AppLayout.tsx src/App.tsx tests/AppLayout.test.tsx
git commit -m "feat: add system utility navigation"
```

Expected: commit includes navigation and app registration only.

---

### Task 4: Final Verification

**Files:**
- No new files.
- Read: `git status --short`

**Interfaces:**
- Consumes: all previous task outputs.
- Produces: verified utility feature.

- [ ] **Step 1: Run full front-end tests**

Run: `pnpm test:run`

Expected: pass. If failures occur only in unrelated pre-existing dirty files, record the exact test names and error lines in the final response.

- [ ] **Step 2: Run TypeScript typecheck**

Run: `pnpm typecheck`

Expected: pass. If failures occur only in unrelated pre-existing dirty files, record the exact file and line in the final response.

- [ ] **Step 3: Inspect worktree**

Run: `git status --short`

Expected: only unrelated pre-existing user changes remain after task commits. If utility files are still unstaged, either commit them or explain why they were intentionally left unstaged.

---

## Self-Review

- Spec coverage: navigation, crypto transforms, offline OCR, i18n, error handling, and tests are each mapped to a task.
- Placeholder scan: no `TBD`, `TODO`, or deferred implementation steps remain.
- Type consistency: `CryptoTools`, `OCR`, `CryptoOperation`, `transformCryptoText`, and `recognizeImageText` names are consistent across tasks.
