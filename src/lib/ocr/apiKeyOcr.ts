import { recognizeImageText } from "./recognizeText";

type ClipboardWithRead = Clipboard & {
  read?: () => Promise<ClipboardItem[]>;
};

export class ClipboardImageReadError extends Error {
  constructor(readonly code: "unsupported" | "no-image") {
    super(code);
    this.name = "ClipboardImageReadError";
  }
}

export async function readClipboardImageBlob(): Promise<Blob> {
  const clipboard = typeof navigator === "undefined" ? null : (navigator.clipboard as ClipboardWithRead | undefined);
  if (!clipboard?.read) {
    throw new ClipboardImageReadError("unsupported");
  }

  const items = await clipboard.read();
  for (const item of items) {
    const imageType = item.types.find((type) => type.startsWith("image/"));
    if (imageType) {
      return item.getType(imageType);
    }
  }

  throw new ClipboardImageReadError("no-image");
}

export async function recognizeApiKeysFromImageBlob(blob: Blob): Promise<string> {
  const image = await loadImageFromBlob(blob);
  return extractApiKeysFromOcrText(await recognizeImageText(image));
}

export function extractApiKeysFromOcrText(text: string): string {
  const candidates = new Set<string>();
  const normalized = text.replace(/\u00a0/g, " ").replace(/[ \t]+/g, " ");
  const compactedLines = text
    .split(/\r?\n/)
    .map((line) => line.replace(/\s+/g, ""))
    .join("\n");

  collectApiKeyCandidates(normalized, candidates, true);
  collectApiKeyCandidates(compactedLines, candidates, false);

  if (candidates.size > 0) {
    return Array.from(candidates).join("\n");
  }

  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .join("\n");
}

function collectApiKeyCandidates(value: string, candidates: Set<string>, includeJwt: boolean) {
  const patterns = [
    /\bsk-[A-Za-z0-9][A-Za-z0-9._-]{8,}\b/g,
    /\bAIza[0-9A-Za-z_-]{20,}\b/g,
    ...(includeJwt ? [/\b[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{16,}\b/g] : []),
  ];

  for (const pattern of patterns) {
    for (const match of value.matchAll(pattern)) {
      candidates.add(match[0]);
    }
  }
}

async function loadImageFromBlob(blob: Blob): Promise<HTMLImageElement> {
  const url = URL.createObjectURL(blob);
  const image = new Image();

  try {
    await new Promise<void>((resolve, reject) => {
      image.onload = () => resolve();
      image.onerror = () => reject(new Error("Failed to load image"));
      image.src = url;
    });
    return image;
  } finally {
    URL.revokeObjectURL(url);
  }
}
