import { version } from "../../package.json";

/**
 * Application version shown in the About screen.
 *
 * Read from `package.json` at build time rather than from the Tauri `app`
 * plugin: the same bundle is served to browsers by `ai-switch-server`, where no
 * Tauri API exists and `getVersion()` would throw. The release flow keeps
 * `package.json`, `tauri.conf.json`, and `Cargo.toml` on the same version, so
 * this is the shipped version in both runtimes.
 */
export const appVersion: string = version;
