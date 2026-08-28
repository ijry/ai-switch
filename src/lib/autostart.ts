import {
  disable,
  enable,
  isEnabled,
} from "@tauri-apps/plugin-autostart";

export function isAutostartEnabled(): Promise<boolean> {
  return isEnabled();
}

export function enableAutostart(): Promise<void> {
  return enable();
}

export function disableAutostart(): Promise<void> {
  return disable();
}
