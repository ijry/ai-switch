export function downloadRouteCredentialJson(
  jsonText: string,
  fileName: string,
): void {
  const blob = new Blob([jsonText], { type: "application/json" });
  const objectUrl = URL.createObjectURL(blob);
  let anchor: HTMLAnchorElement | null = null;

  try {
    anchor = document.createElement("a");
    anchor.href = objectUrl;
    anchor.download = fileName;
    document.body.appendChild(anchor);
    anchor.click();
  } finally {
    try {
      anchor?.remove();
    } finally {
      URL.revokeObjectURL(objectUrl);
    }
  }
}

export async function copySensitiveText(text: string): Promise<void> {
  if (typeof navigator === "undefined" || !navigator.clipboard?.writeText) {
    throw new Error("Clipboard access is unavailable.");
  }

  await navigator.clipboard.writeText(text);
}
