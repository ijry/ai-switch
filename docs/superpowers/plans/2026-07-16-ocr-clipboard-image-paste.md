# OCR Clipboard Image Paste Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users paste a clipboard image into the existing OCR screen, preview it, and keep OCR manual.

**Architecture:** Keep clipboard parsing in a tiny helper under `src/lib/ocr`, and let `OcrScreen` reuse its current preview and recognition flow. The screen stays front-end only: paste events are handled in React, image blobs become object URLs, and OCR still starts only when the user clicks `Recognize`.

**Tech Stack:** React 18, TypeScript, Vitest, Testing Library, browser `ClipboardEvent` / `DataTransfer` APIs.

## Global Constraints

- The feature is front-end only. It does not add Tauri commands, clipboard plugins, network calls, or backend APIs.
- When the OCR screen is focused and the user presses `Ctrl+V`, the screen reads `event.clipboardData.items` from the paste event.
- If the clipboard contains an `image/*` item, use the first image item, convert it to a `File` or `Blob`, reuse the existing preview URL state, clear the previous OCR result, show a source label such as `Pasted image`, and do not start OCR automatically.
- If the clipboard does not contain an image item, keep the current selected image, if any, and show an inline error: `Clipboard does not contain an image.`
- Add English and Simplified Chinese strings for the paste hint, pasted image source label, and clipboard error.

---

## File Structure

- Create `src/lib/ocr/clipboardImage.ts`: clipboard item extraction helper for image pastes.
- Modify `src/screens/OcrScreen.tsx`: paste handler, focusable screen root, preview reuse, and clipboard error state.
- Modify `src/lib/i18n.tsx`: add paste hint, pasted image label, and clipboard error strings.
- Modify `tests/OcrScreen.test.tsx`: add clipboard paste coverage for image and non-image cases.

---

### Task 1: Clipboard Paste Support

**Files:**
- Create: `src/lib/ocr/clipboardImage.ts`
- Modify: `src/screens/OcrScreen.tsx`
- Modify: `src/lib/i18n.tsx`
- Modify: `tests/OcrScreen.test.tsx`

**Interfaces:**
- Produces: `function extractClipboardImage(clipboardData: DataTransfer): File | null`
- Consumes: existing `recognizeImageText(source: HTMLImageElement | HTMLCanvasElement): Promise<string>`
- Consumes: existing `useI18n()`

- [ ] **Step 1: Write the failing clipboard tests**

Update `tests/OcrScreen.test.tsx` so it covers both paste paths:

```tsx
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../src/lib/i18n";
import { recognizeImageText } from "../src/lib/ocr/recognizeText";
import { OcrScreen } from "../src/screens/OcrScreen";

vi.mock("../src/lib/ocr/recognizeText", () => ({
  recognizeImageText: vi.fn(),
}));

beforeEach(() => {
  Object.defineProperty(URL, "createObjectURL", {
    configurable: true,
    value: vi.fn(() => "blob:sample"),
  });
  Object.defineProperty(URL, "revokeObjectURL", {
    configurable: true,
    value: vi.fn(),
  });
});

function renderScreen() {
  return render(
    <I18nProvider initialLanguage="zh-CN">
      <OcrScreen />
    </I18nProvider>,
  );
}

describe("OcrScreen", () => {
  it("loads a pasted image from the clipboard and keeps recognition manual", async () => {
    vi.mocked(recognizeImageText).mockResolvedValue("ABCD 1234");
    renderScreen();

    expect(screen.getByText("也可以按 Ctrl+V 粘贴图片。")).toBeInTheDocument();

    const root = screen.getByText("OCR识别").closest("section");
    const file = new File(["fake"], "clipboard.png", { type: "image/png" });

    fireEvent.paste(root!, {
      clipboardData: {
        items: [
          {
            kind: "file",
            type: "image/png",
            getAsFile: () => file,
          },
        ],
        files: [file],
        types: ["Files"],
        getData: () => "",
      },
    });

    expect(screen.getByText("粘贴的图片")).toBeInTheDocument();
    expect(recognizeImageText).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "开始识别" }));
    await waitFor(() => expect(recognizeImageText).toHaveBeenCalled());
    expect(await screen.findByLabelText("识别结果")).toHaveValue("ABCD 1234");
  });

  it("shows a clipboard error and keeps the current image when pasted content is not an image", async () => {
    renderScreen();

    const file = new File(["fake"], "sample.png", { type: "image/png" });
    await userEvent.upload(screen.getByLabelText("选择图片"), file);

    const root = screen.getByText("OCR识别").closest("section");
    fireEvent.paste(root!, {
      clipboardData: {
        items: [
          {
            kind: "string",
            type: "text/plain",
            getAsFile: () => null,
          },
        ],
        files: [],
        types: ["text/plain"],
        getData: () => "hello",
      },
    });

    expect(screen.getByText("sample.png")).toBeInTheDocument();
    expect(screen.getByText("剪切板中没有图片。")).toBeInTheDocument();
    expect(screen.getByLabelText("识别结果")).toHaveValue("");
  });
});
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `pnpm vitest run tests/OcrScreen.test.tsx`

Expected: FAIL because the OCR screen does not yet handle clipboard image pastes or the new strings.

- [ ] **Step 3: Implement the clipboard helper and screen behavior**

Create `src/lib/ocr/clipboardImage.ts`:

```ts
export function extractClipboardImage(clipboardData: DataTransfer): File | null {
  for (const item of Array.from(clipboardData.items)) {
    if (item.kind !== "file" || !item.type.startsWith("image/")) {
      continue;
    }

    const file = item.getAsFile();
    if (file) {
      return file;
    }
  }

  for (const file of Array.from(clipboardData.files ?? [])) {
    if (file.type.startsWith("image/")) {
      return file;
    }
  }

  return null;
}
```

Update `src/screens/OcrScreen.tsx` to:

```tsx
import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { ScanText } from "lucide-react";
import { extractClipboardImage } from "../lib/ocr/clipboardImage";
import { recognizeImageText } from "../lib/ocr/recognizeText";
import { useI18n } from "../lib/i18n";

type OcrError = "no-file" | "invalid-file" | "clipboard-no-image" | "failed" | null;

const errorKeys = {
  "no-file": "ocr.error.noFile",
  "invalid-file": "ocr.error.invalidFile",
  "clipboard-no-image": "ocr.error.clipboardNoImage",
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
      if (previewUrl && typeof URL.revokeObjectURL === "function") {
        URL.revokeObjectURL(previewUrl);
      }
    };
  }, [previewUrl]);

  const setImagePreview = (file: Blob, displayName: string) => {
    const nextUrl = URL.createObjectURL(file);
    setPreviewUrl((current) => {
      if (current && typeof URL.revokeObjectURL === "function") {
        URL.revokeObjectURL(current);
      }
      return nextUrl;
    });
    setFileName(displayName);
    setResult("");
    setError(null);
  };

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
    setImagePreview(file, file.name);
  };

  const handlePaste = (event: React.ClipboardEvent<HTMLElement>) => {
    event.preventDefault();
    const pastedImage = extractClipboardImage(event.clipboardData);
    if (!pastedImage) {
      setError("clipboard-no-image");
      return;
    }
    setImagePreview(pastedImage, t("ocr.pastedImage"));
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
    <section className="space-y-3" tabIndex={0} onPaste={handlePaste}>
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
          <p className="mt-1 text-[12px] text-stone-500">{t("ocr.pasteHint")}</p>
          <p className="mt-3 text-[13px] font-medium text-stone-700">{fileName ?? t("ocr.noFile")}</p>
          {error && (
            <p className="mt-2 text-[13px] font-medium text-red-700">
              {t(errorKeys[error] as Parameters<typeof t>[0])}
            </p>
          )}
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

Add these keys to `src/lib/i18n.tsx`:

```ts
"ocr.pasteHint": "You can also paste an image with Ctrl+V.",
"ocr.pastedImage": "Pasted image",
"ocr.error.clipboardNoImage": "Clipboard does not contain an image.",
```

And in `zh`:

```ts
"ocr.pasteHint": "也可以按 Ctrl+V 粘贴图片。",
"ocr.pastedImage": "粘贴的图片",
"ocr.error.clipboardNoImage": "剪切板中没有图片。",
```

- [ ] **Step 4: Run the focused test and typecheck**

Run: `pnpm vitest run tests/OcrScreen.test.tsx`

Expected: pass with the clipboard paste cases and the existing OCR recognition coverage.

Run: `pnpm typecheck`

Expected: pass.

- [ ] **Step 5: Commit the feature**

Run:

```powershell
git add src/lib/ocr/clipboardImage.ts src/screens/OcrScreen.tsx src/lib/i18n.tsx tests/OcrScreen.test.tsx
git commit -m "feat: paste clipboard images into OCR"
```

Expected: commit includes only clipboard paste support files.

---

## Self-Review

- Spec coverage: paste hint, pasted image label, clipboard error, and manual-only OCR all map to the single task.
- Placeholder scan: no `TBD`, `TODO`, or deferred implementation steps remain.
- Type consistency: `extractClipboardImage`, `clipboard-no-image`, `ocr.pasteHint`, and `ocr.pastedImage` are used consistently across tests and implementation.
