import type { TranslationKey } from "../../lib/i18n";

export const OFFICIAL_SITE_URL = "https://ijry.github.io/ai-switch/";
export const REPOSITORY_URL = "https://github.com/ijry/ai-switch";

export type AboutLink = {
  labelKey: TranslationKey;
  url: string;
};

export const ABOUT_LINKS: AboutLink[] = [
  { labelKey: "about.linkWebsite", url: OFFICIAL_SITE_URL },
  { labelKey: "about.linkRepository", url: REPOSITORY_URL },
  { labelKey: "about.linkReleases", url: `${REPOSITORY_URL}/releases` },
  { labelKey: "about.linkIssues", url: `${REPOSITORY_URL}/issues` },
];

export type OpenSourceCredit = {
  name: string;
  /** SPDX identifiers as published by the project; `/` marks a dual license. */
  license: string;
  url: string;
};

export type OpenSourceCreditGroup = {
  titleKey: TranslationKey;
  items: OpenSourceCredit[];
};

/**
 * Direct dependencies worth naming, grouped the way they are used here. Kept in
 * sync with `package.json`, `src-tauri/Cargo.toml`, `sidecar/ai-switch-tsnet`,
 * and `THIRD_PARTY_NOTICES.md` by hand: transitive crates and packages are left
 * to the lockfiles.
 */
export const OPEN_SOURCE_CREDITS: OpenSourceCreditGroup[] = [
  {
    titleKey: "about.credits.groupApp",
    items: [
      { name: "Tauri", license: "MIT / Apache-2.0", url: "https://tauri.app" },
      { name: "React", license: "MIT", url: "https://react.dev" },
      { name: "TypeScript", license: "Apache-2.0", url: "https://www.typescriptlang.org" },
      { name: "Vite", license: "MIT", url: "https://vite.dev" },
      { name: "UnoCSS", license: "MIT", url: "https://unocss.dev" },
      { name: "TanStack Query", license: "MIT", url: "https://tanstack.com/query" },
      { name: "Motion", license: "MIT", url: "https://motion.dev" },
      { name: "Lucide", license: "ISC", url: "https://lucide.dev" },
      { name: "xterm.js", license: "MIT", url: "https://xtermjs.org" },
      { name: "three.js", license: "MIT", url: "https://threejs.org" },
      { name: "react-markdown", license: "MIT", url: "https://github.com/remarkjs/react-markdown" },
      { name: "remark-gfm", license: "MIT", url: "https://github.com/remarkjs/remark-gfm" },
      { name: "JSZip", license: "MIT / GPL-3.0", url: "https://stuk.github.io/jszip/" },
      { name: "node-qrcode", license: "MIT", url: "https://github.com/soldair/node-qrcode" },
      { name: "Ocrad.js", license: "GPL-3.0", url: "https://github.com/antimatter15/ocrad.js" },
      { name: "Vitest", license: "MIT", url: "https://vitest.dev" },
    ],
  },
  {
    titleKey: "about.credits.groupCore",
    items: [
      { name: "Rust", license: "MIT / Apache-2.0", url: "https://www.rust-lang.org" },
      { name: "Tokio", license: "MIT", url: "https://tokio.rs" },
      { name: "axum", license: "MIT", url: "https://github.com/tokio-rs/axum" },
      { name: "SQLx", license: "MIT / Apache-2.0", url: "https://github.com/launchbadge/sqlx" },
      { name: "SQLite", license: "Public Domain", url: "https://www.sqlite.org" },
      { name: "Serde", license: "MIT / Apache-2.0", url: "https://serde.rs" },
      { name: "reqwest", license: "MIT / Apache-2.0", url: "https://github.com/seanmonstar/reqwest" },
      { name: "portable-pty", license: "MIT", url: "https://github.com/wezterm/wezterm" },
      { name: "keyring-rs", license: "MIT / Apache-2.0", url: "https://github.com/hwchen/keyring-rs" },
    ],
  },
  {
    titleKey: "about.credits.groupNetwork",
    items: [
      { name: "Tailscale (tsnet)", license: "BSD-3-Clause", url: "https://tailscale.com" },
      { name: "rustls", license: "Apache-2.0 / MIT / ISC", url: "https://github.com/rustls/rustls" },
      { name: "rcgen", license: "MIT / Apache-2.0", url: "https://github.com/rustls/rcgen" },
    ],
  },
  {
    titleKey: "about.credits.groupDerived",
    items: [{ name: "codeg", license: "Apache-2.0", url: "https://github.com/xintaofei/codeg" }],
  },
];

export type FriendlyLink = {
  name: string;
  url: string;
};

export const FRIENDLY_LINKS: FriendlyLink[] = [
  { name: "MCode", url: "https://getmcode.lingyun.net" },
  {
    name: "DeepSeek Harness Desktop Ultra",
    url: "https://ijry.github.io/DeepSeek-Harness-Desktop-Ultra/",
  },
  { name: "uview-plus", url: "https://uview-plus.jiangruyi.com/" },
  { name: "Lingyun App", url: "https://app.lingyun.net/" },
  { name: "FastView", url: "https://fastview.lingyun.net/" },
  { name: "ShareZone", url: "https://sharezone.lingyun.net/" },
  { name: "AirDB", url: "https://airdb.lingyun.net/" },
];
