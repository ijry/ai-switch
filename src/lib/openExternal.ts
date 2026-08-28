import { openUrl } from "@tauri-apps/plugin-opener";
import { isDesktop } from "./transport";

/**
 * Open an http(s) url outside the app.
 *
 * The desktop webview drops `window.open` and `target="_blank"` navigations, so
 * the URL has to be handed to the system browser through the opener plugin.
 * Failures are propagated instead of swallowed: a silently dead link is worse
 * than a visible error.
 */
export async function openExternal(url: string): Promise<void> {
  if (isDesktop()) {
    await openUrl(url);
    return;
  }

  window.open(url, "_blank", "noopener,noreferrer");
}
