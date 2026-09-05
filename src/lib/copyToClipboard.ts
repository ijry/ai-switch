/**
 * Copy non-sensitive plain text to the clipboard.
 *
 * `navigator.clipboard` only exists in a secure context, and the self-hosted Web
 * app is usually reached over plain HTTP on a private network address, so the
 * async API is missing exactly where sharing is most likely to happen. The
 * deprecated `execCommand` path still works there. Only use this for text the
 * user is about to publish anyway; secrets keep going through
 * `copySensitiveText`, which fails loudly instead of falling back.
 *
 * Returns whether the text reached the clipboard so callers can offer a manual
 * copy instead of claiming success.
 */
export async function copyPlainText(text: string): Promise<boolean> {
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // A denied permission or a non-focused document still leaves the
      // textarea path below.
    }
  }

  return copyWithHiddenTextarea(text);
}

function copyWithHiddenTextarea(text: string): boolean {
  if (typeof document === "undefined" || !document.body || !document.execCommand) {
    return false;
  }

  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "true");
  // Keep it out of view without `display: none`, which would make the selection
  // and the copy command no-ops.
  textarea.style.position = "fixed";
  textarea.style.top = "-1000px";
  textarea.style.left = "-1000px";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);

  try {
    textarea.select();
    textarea.setSelectionRange(0, text.length);
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    textarea.remove();
  }
}
