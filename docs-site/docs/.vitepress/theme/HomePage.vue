<script setup lang="ts">
import { computed } from "vue";
import { useData, withBase } from "vitepress";

const { localeIndex } = useData();

const isEn = computed(() => localeIndex.value === "en");
// cleanUrls is false, so custom-component links must carry .html themselves.
// withBase only prepends base when the path starts with a slash.
const link = (path: string) => withBase(`${isEn.value ? "/en" : ""}${path}.html`);

const REPO = "https://github.com/ijry/ai-switch";
const RELEASES = `${REPO}/releases/latest`;
const SHOT =
  "https://github.com/user-attachments/assets/fbd3932e-29a7-4e3f-a980-e93fb093b643";

const PLATFORM_ICONS: Record<string, string> = {
  codex: `<svg viewBox="2 2 20 20" xmlns="http://www.w3.org/2000/svg"><path d="M9.064 3.344a4.578 4.578 0 012.285-.312c1 .115 1.891.54 2.673 1.275.01.01.024.017.037.021a.09.09 0 00.043 0 4.55 4.55 0 013.046.275l.047.022.116.057a4.581 4.581 0 012.188 2.399c.209.51.313 1.041.315 1.595a4.24 4.24 0 01-.134 1.223.123.123 0 00.03.115c.594.607.988 1.33 1.183 2.17.289 1.425-.007 2.71-.887 3.854l-.136.166a4.548 4.548 0 01-2.201 1.388.123.123 0 00-.081.076c-.191.551-.383 1.023-.74 1.494-.9 1.187-2.222 1.846-3.711 1.838-1.187-.006-2.239-.44-3.157-1.302a.107.107 0 00-.105-.024c-.388.125-.78.143-1.204.138a4.441 4.441 0 01-1.945-.466 4.544 4.544 0 01-1.61-1.335c-.152-.202-.303-.392-.414-.617a5.81 5.81 0 01-.37-.961 4.582 4.582 0 01-.014-2.298.124.124 0 00.006-.056.085.085 0 00-.027-.048 4.467 4.467 0 01-1.034-1.651 3.896 3.896 0 01-.251-1.192 5.189 5.189 0 01.141-1.6c.337-1.112.982-1.985 1.933-2.618.212-.141.413-.251.601-.33.215-.089.43-.164.646-.227a.098.098 0 00.065-.066 4.51 4.51 0 01.829-1.615 4.535 4.535 0 011.837-1.388zm3.482 10.565a.637.637 0 000 1.272h3.636a.637.637 0 100-1.272h-3.636zM8.462 9.23a.637.637 0 00-1.106.631l1.272 2.224-1.266 2.136a.636.636 0 101.095.649l1.454-2.455a.636.636 0 00.005-.64L8.462 9.23z" fill="url(#as-codex-g)"/><defs><linearGradient gradientUnits="userSpaceOnUse" id="as-codex-g" x1="12" x2="12" y1="3" y2="21"><stop stop-color="#B1A7FF"/><stop offset=".5" stop-color="#7A9DFF"/><stop offset="1" stop-color="#3941FF"/></linearGradient></defs></svg>`,
  claude: `<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path clip-rule="evenodd" d="M20.998 10.949H24v3.102h-3v3.028h-1.487V20H18v-2.921h-1.487V20H15v-2.921H9V20H7.488v-2.921H6V20H4.487v-2.921H3V14.05H0V10.95H3V5h17.998v5.949zM6 10.949h1.488V8.102H6v2.847zm10.51 0H18V8.102h-1.49v2.847z" fill="#D97757" fill-rule="evenodd"/></svg>`,
  gemini: `<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M0 4.391A4.391 4.391 0 014.391 0h15.217A4.391 4.391 0 0124 4.391v15.217A4.391 4.391 0 0119.608 24H4.391A4.391 4.391 0 010 19.608V4.391z" fill="url(#as-gemini-g)"/><path clip-rule="evenodd" d="M19.74 1.444a2.816 2.816 0 012.816 2.816v15.48a2.816 2.816 0 01-2.816 2.816H4.26a2.816 2.816 0 01-2.816-2.816V4.26A2.816 2.816 0 014.26 1.444h15.48zM7.236 8.564l7.752 3.728-7.752 3.727v2.802l9.557-4.596v-3.866L7.236 5.763v2.801z" fill="#1E1E2E" fill-rule="evenodd"/><defs><linearGradient gradientUnits="userSpaceOnUse" id="as-gemini-g" x1="24" x2="0" y1="6.587" y2="16.494"><stop stop-color="#EE4D5D"/><stop offset=".328" stop-color="#B381DD"/><stop offset=".476" stop-color="#207CFE"/></linearGradient></defs></svg>`,
  grok: `<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><rect fill="#0B0F14" height="24" rx="5" width="24"/><path d="M6.2 7.4h2.1l2.1 5.2 2.1-5.2h2.1l-3.4 8.1H9.6L6.2 7.4zm10.1 0H18v8.1h-1.7V7.4z" fill="#F8FAFC"/><circle cx="18.4" cy="16.8" fill="#22D3EE" r="1.1"/></svg>`,
  opencode: `<svg fill="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M16 6H8v12h8V6zm4 16H4V2h16v20z"/></svg>`,
  openclaw: `<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M12 2.568c-6.33 0-9.495 5.275-9.495 9.495 0 4.22 3.165 8.44 6.33 9.494v2.11h2.11v-2.11s1.055.422 2.11 0v2.11h2.11v-2.11c3.165-1.055 6.33-5.274 6.33-9.494S18.33 2.568 12 2.568z" fill="url(#as-claw-s)"/><path d="M3.56 9.953C.396 8.898-.66 11.008.396 13.118c1.055 2.11 3.164 1.055 4.22-1.055.632-1.477 0-2.11-1.056-2.11z" fill="url(#as-claw-l)"/><path d="M20.44 9.953c3.164-1.055 4.22 1.055 3.164 3.165-1.055 2.11-3.164 1.055-4.22-1.055-.632-1.477 0-2.11 1.056-2.11z" fill="url(#as-claw-r)"/><path d="M8.835 9.109a1.266 1.266 0 100-2.532 1.266 1.266 0 000 2.532zM15.165 9.109a1.266 1.266 0 100-2.532 1.266 1.266 0 000 2.532z" fill="#050810"/><path d="M9.046 8.16a.527.527 0 100-1.056.527.527 0 100 1.055zM15.376 8.16a.527.527 0 100-1.055.527.527 0 100 1.054z" fill="#00E5CC"/><defs><linearGradient gradientUnits="userSpaceOnUse" id="as-claw-s" x1="-.659" x2="27.023" y1=".458" y2="22.855"><stop stop-color="#FF4D4D"/><stop offset="1" stop-color="#991B1B"/></linearGradient><linearGradient gradientUnits="userSpaceOnUse" id="as-claw-l" x1="0" x2="4.311" y1="9.672" y2="14.949"><stop stop-color="#FF4D4D"/><stop offset="1" stop-color="#991B1B"/></linearGradient><linearGradient gradientUnits="userSpaceOnUse" id="as-claw-r" x1="19.385" x2="24.399" y1="9.953" y2="14.462"><stop stop-color="#FF4D4D"/><stop offset="1" stop-color="#991B1B"/></linearGradient></defs></svg>`,
  hermes: `<svg fill="none" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><rect height="20" rx="6" stroke="currentColor" stroke-width="2" width="20" x="2" y="2"/><path d="M7 16V8M17 8v8M8 12h8M5 7.2l2.4-2.1M19 7.2l-2.4-2.1" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"/></svg>`,
};

const COPY = {
  root: {
    heroEyebrow: "开源 · MIT 许可",
    heroTitle: "一个壳里统一管理所有 AI API",
    heroLead:
      "AI Switch 把 Codex、Claude Code、Gemini CLI、Grok 等命令行工具的账号、协议和算力集中到一处：本地代理统一入口，算力池自动调度，上游协议按需桥接。桌面应用与自托管 Web 服务共用同一份数据。",
    ctaPrimary: "快速开始",
    ctaSecondary: "下载最新版",
    metrics: [
      { value: "7", label: "个智能体平台" },
      { value: "4", label: "种上游协议" },
      { value: "7", label: "条桥接链路" },
      { value: "MIT", label: "开源许可" },
    ],
    shotAlt: "AI Switch 界面截图",

    platformsTitle: "支持的平台",
    platformsLead:
      "Codex、Claude Code、Gemini CLI 和 Grok 拥有原生配置写入与官方账号导入；OpenCode、OpenClaw 和 Hermes 通过通用 API 路由接入，可用于路由、终端启动和会话流程。",
    tierFull: "原生支持",
    tierGeneric: "通用 API 路由",
    platformsNote:
      "原生配置写入采用安全直写：变更前建立快照、原子写入、检测并发修改，并支持带守卫的回滚。",
    capMatrixLink: "查看完整能力矩阵",

    bridgeTitle: "协议桥接，本地入口不用改",
    bridgeLead:
      "CLI 仍然按它自己的协议说话，AI Switch 在本地代理里把请求翻译成上游账号实际支持的协议。换供应商不需要改 CLI 配置。",
    bridgeLocal: "本地入口",
    bridgeProxy: "AI Switch 本地代理",
    bridgeUpstream: "上游协议",
    bridgeEntries: [
      { name: "Codex", path: "/responses", note: "OpenAI Responses" },
      { name: "Claude Code", path: "/v1/messages", note: "Anthropic Messages" },
      { name: "Gemini CLI", path: "Gemini native", note: "仅路由 Gemini 账号" },
    ],
    bridgeDialects: [
      { id: "openai", note: "Chat Completions" },
      { id: "openai-responses", note: "Responses API" },
      { id: "anthropic", note: "Messages API" },
      { id: "gemini", note: "generateContent" },
    ],
    bridgeListTitle: "七条桥接链路",
    bridgeList: [
      "Responses → Chat Completions",
      "Responses → Responses",
      "Responses → Anthropic",
      "Responses → Gemini",
      "Claude → Chat Completions",
      "Claude → Responses",
      "Claude → Gemini",
    ],
    bridgeMore: "了解协议路由",

    capTitle: "核心能力",
    caps: [
      {
        title: "算力池调度",
        body: "每个账号可设 1–5 级优先级和并发上限，按平台分池。高优先级先用，占满自动落到下一级。",
        to: "/guide/accounts",
      },
      {
        title: "协议路由与桥接",
        body: "四种上游协议任选，本地入口协议与上游不一致时自动转换。默认监听 127.0.0.1:19527。",
        to: "/guide/protocol-routing",
      },
      {
        title: "失败处理与恢复",
        body: "记录瞬时失败次数与失败类型，按冷却窗口暂时摘出问题账号，到点自动重试回池。",
        to: "/guide/reliability",
      },
      {
        title: "用量可观测",
        body: "按请求记录输入、输出与缓存 token，支持美元和人民币双币种计价，可按账号和平台拆分。",
        to: "/guide/usage-stats",
      },
      {
        title: "终端与会话",
        body: "内置终端直接拉起 CLI，会话可恢复，另有三套 Vibe 皮肤换个心情。",
        to: "/features/vibe",
      },
      {
        title: "生态集成",
        body: "统一管理 11 个 MCP 客户端的服务器配置，内置两个技能包共 27 个技能。",
        to: "/features/mcp",
      },
    ],

    formTitle: "一套界面，四种形态",
    formLead:
      "桌面端与浏览器端共用同一份 React 界面和同一个数据库，代理行为完全一致。",
    forms: [
      { name: "桌面应用", body: "Tauri 2 原生外壳，通过 IPC 直连 Rust 核心。" },
      { name: "局域网浏览器", body: "开启 Web 服务，用 token 鉴权，手机平板都能开。" },
      { name: "Tailscale 私网", body: "手动登录 Tailscale，在私有网络内远程访问。" },
      { name: "Funnel 公网", body: "需要对外时启用 Funnel，token 仍然必需。" },
    ],
    formNote:
      "默认绑定 127.0.0.1:3090。绑定到 0.0.0.0 必须显式设置。所有 /api/* 与 /ws/events 请求都需要访问令牌，即使走 Tailscale 也一样。",
    formMore: "查看部署方式",

    archTitle: "技术架构",
    archLead:
      "前端一份，传输两套，核心共享。Rust 核心同时服务桌面 IPC 和 HTTP 传输，因此两端行为不会漂移。",
    archLayers: [
      { name: "界面层", body: "React 18 + TypeScript，桌面与浏览器同一份代码" },
      { name: "核心层", body: "ai_switch_lib — 账号、路由、会话、终端、MCP" },
      { name: "传输层", body: "Tauri IPC / axum HTTP + WebSocket" },
      { name: "存储层", body: "SQLite，23 个迁移；数据与凭据存于本地数据目录" },
    ],
    archStack: "技术栈",
    archMore: "深入架构",

    finalTitle: "开始使用 AI Switch",
    finalLead: "桌面端装完即用，也可以自己跑一份服务给团队。",
    finalPrimary: "安装指南",
    finalSecondary: "在 GitHub 上查看",
  },

  en: {
    heroEyebrow: "Open source · MIT",
    heroTitle: "One shell for every AI API",
    heroLead:
      "AI Switch brings the accounts, protocols, and capacity behind Codex, Claude Code, Gemini CLI, Grok and more into one place: a single local proxy entry, automatic pool scheduling, and on-demand upstream protocol bridging. The desktop app and the self-hosted web service share the same data.",
    ctaPrimary: "Quick Start",
    ctaSecondary: "Download",
    metrics: [
      { value: "7", label: "agent platforms" },
      { value: "4", label: "upstream protocols" },
      { value: "7", label: "bridge routes" },
      { value: "MIT", label: "licensed" },
    ],
    shotAlt: "AI Switch screenshot",

    platformsTitle: "Supported platforms",
    platformsLead:
      "Codex, Claude Code, Gemini CLI and Grok get native config writing and official account import. OpenCode, OpenClaw and Hermes connect through generic API routing, which still covers routing, terminal launch and session workflows.",
    tierFull: "Native support",
    tierGeneric: "Generic API routing",
    platformsNote:
      "Native config writing uses safe direct writes: a snapshot is taken before mutation, the write is atomic, concurrent changes are detected, and guarded rollback is supported.",
    capMatrixLink: "See the full capability matrix",

    bridgeTitle: "Protocol bridging, no CLI reconfiguration",
    bridgeLead:
      "Each CLI keeps speaking its own protocol. AI Switch translates the request inside the local proxy to whatever the upstream account actually supports, so switching providers never means editing CLI config.",
    bridgeLocal: "Local entry",
    bridgeProxy: "AI Switch local proxy",
    bridgeUpstream: "Upstream protocol",
    bridgeEntries: [
      { name: "Codex", path: "/responses", note: "OpenAI Responses" },
      { name: "Claude Code", path: "/v1/messages", note: "Anthropic Messages" },
      { name: "Gemini CLI", path: "Gemini native", note: "Gemini accounts only" },
    ],
    bridgeDialects: [
      { id: "openai", note: "Chat Completions" },
      { id: "openai-responses", note: "Responses API" },
      { id: "anthropic", note: "Messages API" },
      { id: "gemini", note: "generateContent" },
    ],
    bridgeListTitle: "Seven bridge routes",
    bridgeList: [
      "Responses → Chat Completions",
      "Responses → Responses",
      "Responses → Anthropic",
      "Responses → Gemini",
      "Claude → Chat Completions",
      "Claude → Responses",
      "Claude → Gemini",
    ],
    bridgeMore: "Learn about protocol routing",

    capTitle: "Core capabilities",
    caps: [
      {
        title: "Pool scheduling",
        body: "Give every account a priority from 1 to 5 and a concurrency cap, pooled per platform. High priority goes first; when it saturates, traffic falls through to the next tier.",
        to: "/guide/accounts",
      },
      {
        title: "Routing and bridging",
        body: "Pick any of four upstream protocols. When the local entry protocol differs from the upstream, the proxy converts it. Listens on 127.0.0.1:19527 by default.",
        to: "/guide/protocol-routing",
      },
      {
        title: "Failure handling",
        body: "Transient failure counts and failure kinds are recorded, bad accounts are pulled out for a cooldown window, and retried back into the pool when it expires.",
        to: "/guide/reliability",
      },
      {
        title: "Usage visibility",
        body: "Input, output and cache tokens are recorded per request, priced in both USD and CNY, and breakable down by account and platform.",
        to: "/guide/usage-stats",
      },
      {
        title: "Terminal and sessions",
        body: "Launch CLIs straight from the built-in terminal, resume sessions, and swap between three Vibe skins when the mood calls for it.",
        to: "/features/vibe",
      },
      {
        title: "Ecosystem",
        body: "Manage MCP server config across 11 clients from one place, plus two bundled skill packages totalling 27 skills.",
        to: "/features/mcp",
      },
    ],

    formTitle: "One UI, four shapes",
    formLead:
      "Desktop and browser share the same React UI and the same database, so proxy behaviour is identical either way.",
    forms: [
      { name: "Desktop app", body: "A native Tauri 2 shell talking to the Rust core over IPC." },
      { name: "LAN browser", body: "Start the web service with token auth and open it from a phone or tablet." },
      { name: "Tailscale private", body: "Log in to Tailscale manually and reach it across your private network." },
      { name: "Funnel public", body: "Enable Funnel when you need public reach — the token is still required." },
    ],
    formNote:
      "Default bind is 127.0.0.1:3090; binding to 0.0.0.0 must be explicit. Every /api/* and /ws/events request needs the access token, Tailscale included.",
    formMore: "See deployment options",

    archTitle: "Architecture",
    archLead:
      "One frontend, two transports, a shared core. The Rust core serves both desktop IPC and HTTP, so the two ends cannot drift apart.",
    archLayers: [
      { name: "UI", body: "React 18 + TypeScript, one codebase for desktop and browser" },
      { name: "Core", body: "ai_switch_lib — accounts, routing, sessions, terminal, MCP" },
      { name: "Transport", body: "Tauri IPC / axum HTTP + WebSocket" },
      { name: "Storage", body: "SQLite across 23 migrations; data and credentials in the local data directory" },
    ],
    archStack: "Stack",
    archMore: "Read the architecture",

    finalTitle: "Get started with AI Switch",
    finalLead:
      "Install the desktop build and go, or run the service yourself for a team.",
    finalPrimary: "Installation guide",
    finalSecondary: "View on GitHub",
  },
} as const;

const t = computed(() => (isEn.value ? COPY.en : COPY.root));

const PLATFORMS = [
  { key: "codex", name: "Codex", tier: "full" },
  { key: "claude", name: "Claude Code", tier: "full" },
  { key: "gemini", name: "Gemini CLI", tier: "full" },
  { key: "grok", name: "Grok", tier: "full" },
  { key: "opencode", name: "OpenCode", tier: "generic" },
  { key: "openclaw", name: "OpenClaw", tier: "generic" },
  { key: "hermes", name: "Hermes", tier: "generic" },
] as const;

const STACK = [
  "Tauri 2",
  "Rust",
  "axum",
  "sqlx",
  "SQLite",
  "React 18",
  "TypeScript",
  "Go sidecar",
  "Tailscale tsnet",
  "rustls",
];
</script>

<template>
  <div class="as-home">
    <!-- 1 · Hero -->
    <section class="as-hero">
      <div class="as-wrap as-hero-grid">
        <div class="as-hero-copy">
          <p class="as-eyebrow">{{ t.heroEyebrow }}</p>
          <h1 class="as-hero-title">{{ t.heroTitle }}</h1>
          <p class="as-hero-lead">{{ t.heroLead }}</p>
          <div class="as-cta-row">
            <a class="as-btn as-btn-primary" :href="link('/guide/quick-start')">
              {{ t.ctaPrimary }}
            </a>
            <a class="as-btn as-btn-ghost" :href="RELEASES" target="_blank" rel="noreferrer">
              {{ t.ctaSecondary }}
            </a>
          </div>
          <dl class="as-metrics">
            <div v-for="m in t.metrics" :key="m.label" class="as-metric">
              <dt class="as-metric-value">{{ m.value }}</dt>
              <dd class="as-metric-label">{{ m.label }}</dd>
            </div>
          </dl>
        </div>

        <div class="as-shot-frame">
          <div class="as-shot-bar">
            <span class="as-dot" /><span class="as-dot" /><span class="as-dot" />
          </div>
          <img class="as-shot" :src="SHOT" :alt="t.shotAlt" width="2360" height="1520" loading="lazy" />
        </div>
      </div>
    </section>

    <!-- 2 · Platforms -->
    <section class="as-band">
      <div class="as-wrap">
        <h2 class="as-h2">{{ t.platformsTitle }}</h2>
        <p class="as-lead">{{ t.platformsLead }}</p>
        <ul class="as-platform-grid">
          <li v-for="p in PLATFORMS" :key="p.key" class="as-platform">
            <span class="as-platform-icon" v-html="PLATFORM_ICONS[p.key]" />
            <span class="as-platform-name">{{ p.name }}</span>
            <span class="as-tag" :class="p.tier === 'full' ? 'as-tag-full' : 'as-tag-generic'">
              {{ p.tier === "full" ? t.tierFull : t.tierGeneric }}
            </span>
          </li>
        </ul>
        <p class="as-note">{{ t.platformsNote }}</p>
        <a class="as-textlink" :href="link('/guide/platform-support')">{{ t.capMatrixLink }} →</a>
      </div>
    </section>

    <!-- 3 · Protocol bridging -->
    <section class="as-band as-band-alt">
      <div class="as-wrap">
        <h2 class="as-h2">{{ t.bridgeTitle }}</h2>
        <p class="as-lead">{{ t.bridgeLead }}</p>

        <div class="as-flow">
          <div class="as-flow-col">
            <p class="as-flow-head">{{ t.bridgeLocal }}</p>
            <div v-for="e in t.bridgeEntries" :key="e.name" class="as-node">
              <span class="as-node-name">{{ e.name }}</span>
              <code class="as-node-path">{{ e.path }}</code>
              <span class="as-node-note">{{ e.note }}</span>
            </div>
          </div>

          <div class="as-flow-mid">
            <div class="as-proxy">
              <span class="as-proxy-label">{{ t.bridgeProxy }}</span>
              <code class="as-proxy-addr">127.0.0.1:19527</code>
            </div>
          </div>

          <div class="as-flow-col">
            <p class="as-flow-head">{{ t.bridgeUpstream }}</p>
            <div v-for="d in t.bridgeDialects" :key="d.id" class="as-node as-node-up">
              <code class="as-node-path as-node-dialect">{{ d.id }}</code>
              <span class="as-node-note">{{ d.note }}</span>
            </div>
          </div>
        </div>

        <div class="as-bridgelist">
          <p class="as-flow-head">{{ t.bridgeListTitle }}</p>
          <ul>
            <li v-for="b in t.bridgeList" :key="b"><code>{{ b }}</code></li>
          </ul>
        </div>

        <a class="as-textlink" :href="link('/guide/protocol-routing')">{{ t.bridgeMore }} →</a>
      </div>
    </section>

    <!-- 4 · Capabilities -->
    <section class="as-band">
      <div class="as-wrap">
        <h2 class="as-h2">{{ t.capTitle }}</h2>
        <div class="as-cap-grid">
          <a v-for="c in t.caps" :key="c.title" class="as-cap" :href="link(c.to)">
            <h3 class="as-cap-title">{{ c.title }}</h3>
            <p class="as-cap-body">{{ c.body }}</p>
          </a>
        </div>
      </div>
    </section>

    <!-- 5 · Four shapes -->
    <section class="as-band as-band-alt">
      <div class="as-wrap">
        <h2 class="as-h2">{{ t.formTitle }}</h2>
        <p class="as-lead">{{ t.formLead }}</p>
        <ol class="as-form-grid">
          <li v-for="(f, i) in t.forms" :key="f.name" class="as-form">
            <span class="as-form-idx">{{ String(i + 1).padStart(2, "0") }}</span>
            <h3 class="as-form-name">{{ f.name }}</h3>
            <p class="as-form-body">{{ f.body }}</p>
          </li>
        </ol>
        <p class="as-note">{{ t.formNote }}</p>
        <a class="as-textlink" :href="link('/deploy/desktop')">{{ t.formMore }} →</a>
      </div>
    </section>

    <!-- 6 · Architecture -->
    <section class="as-band">
      <div class="as-wrap">
        <h2 class="as-h2">{{ t.archTitle }}</h2>
        <p class="as-lead">{{ t.archLead }}</p>
        <div class="as-arch">
          <div v-for="l in t.archLayers" :key="l.name" class="as-layer">
            <span class="as-layer-name">{{ l.name }}</span>
            <span class="as-layer-body">{{ l.body }}</span>
          </div>
        </div>
        <p class="as-flow-head as-stack-head">{{ t.archStack }}</p>
        <ul class="as-stack">
          <li v-for="s in STACK" :key="s">{{ s }}</li>
        </ul>
        <a class="as-textlink" :href="link('/dev/architecture')">{{ t.archMore }} →</a>
      </div>
    </section>

    <!-- 7 · Final CTA -->
    <section class="as-final">
      <div class="as-wrap as-final-inner">
        <h2 class="as-final-title">{{ t.finalTitle }}</h2>
        <p class="as-final-lead">{{ t.finalLead }}</p>
        <div class="as-cta-row as-cta-center">
          <a class="as-btn as-btn-primary" :href="link('/guide/installation')">
            {{ t.finalPrimary }}
          </a>
          <a class="as-btn as-btn-ghost" :href="REPO" target="_blank" rel="noreferrer">
            {{ t.finalSecondary }}
          </a>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.as-home {
  --as-radius: 14px;
}

.as-wrap {
  max-width: 1152px;
  margin: 0 auto;
  padding: 0 24px;
}

/* ---------- shared type ---------- */

.as-h2 {
  margin: 0;
  font-size: 30px;
  line-height: 1.25;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--as-ink);
}

.as-lead {
  margin: 14px 0 0;
  max-width: 68ch;
  font-size: 16px;
  line-height: 1.7;
  color: var(--as-muted);
}

.as-note {
  margin: 28px 0 0;
  max-width: 76ch;
  padding-left: 14px;
  border-left: 2px solid var(--as-line);
  font-size: 13.5px;
  line-height: 1.7;
  color: var(--as-muted);
}

.as-textlink {
  display: inline-block;
  margin-top: 24px;
  font-size: 14px;
  font-weight: 600;
  color: var(--vp-c-brand-1);
  text-decoration: none;
}

.as-textlink:hover {
  text-decoration: underline;
}

.as-band {
  padding: 72px 0;
  background: var(--as-bg);
  border-top: 1px solid var(--as-line);
}

.as-band-alt {
  background: var(--as-bg-alt);
}

/* ---------- 1 · hero ---------- */

.as-hero {
  padding: 76px 0 84px;
  background:
    radial-gradient(900px 420px at 12% -12%, rgba(16, 185, 129, 0.22), transparent 62%),
    radial-gradient(760px 380px at 92% 8%, rgba(245, 158, 11, 0.16), transparent 60%),
    linear-gradient(160deg, #1c1917 0%, #0c0a09 100%);
  color: var(--as-dark-ink);
}

.as-hero-grid {
  display: grid;
  grid-template-columns: minmax(0, 0.92fr) minmax(0, 1.08fr);
  gap: 52px;
  align-items: center;
}

.as-eyebrow {
  margin: 0 0 18px;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  color: #6ee7b7;
}

.as-hero-title {
  margin: 0;
  font-size: 46px;
  line-height: 1.14;
  font-weight: 800;
  letter-spacing: -0.025em;
  background: linear-gradient(120deg, #ffffff 34%, #6ee7b7 72%, #fcd34d);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
  color: transparent;
}

.as-hero-lead {
  margin: 22px 0 0;
  max-width: 54ch;
  font-size: 16px;
  line-height: 1.75;
  color: var(--as-dark-muted);
}

.as-cta-row {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 30px;
}

.as-cta-center {
  justify-content: center;
}

.as-btn {
  display: inline-flex;
  align-items: center;
  height: 44px;
  padding: 0 24px;
  border-radius: 22px;
  border: 1px solid transparent;
  font-size: 14.5px;
  font-weight: 600;
  text-decoration: none;
  transition: transform 0.15s ease, background-color 0.15s ease, border-color 0.15s ease;
}

.as-btn:hover {
  transform: translateY(-1px);
}

.as-btn-primary {
  background: #10b981;
  color: #052e21;
}

.as-btn-primary:hover {
  background: #34d399;
}

.as-btn-ghost {
  border-color: rgba(250, 250, 249, 0.28);
  color: #fafaf9;
}

.as-btn-ghost:hover {
  border-color: #6ee7b7;
  background: rgba(16, 185, 129, 0.12);
}

.as-metrics {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 18px;
  margin: 42px 0 0;
  padding-top: 26px;
  border-top: 1px solid var(--as-dark-line);
}

.as-metric {
  min-width: 0;
}

.as-metric-value {
  font-size: 26px;
  font-weight: 700;
  line-height: 1.1;
  color: #fafaf9;
}

.as-metric-label {
  margin: 6px 0 0;
  font-size: 12.5px;
  line-height: 1.45;
  color: var(--as-dark-muted);
}

.as-shot-frame {
  border-radius: var(--as-radius);
  border: 1px solid rgba(250, 250, 249, 0.16);
  background: #171412;
  box-shadow: 0 30px 70px rgba(0, 0, 0, 0.5);
  overflow: hidden;
  /* The screenshot is hotlinked, so the frame must look intentional even if
     the image never loads. */
  min-height: 200px;
}

.as-shot-bar {
  display: flex;
  gap: 7px;
  align-items: center;
  height: 32px;
  padding: 0 14px;
  background: #0c0a09;
  border-bottom: 1px solid rgba(250, 250, 249, 0.1);
}

.as-dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: rgba(250, 250, 249, 0.22);
}

.as-shot {
  display: block;
  width: 100%;
  height: auto;
}

/* ---------- 2 · platforms ---------- */

.as-platform-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 14px;
  margin: 34px 0 0;
  padding: 0;
  list-style: none;
}

.as-platform {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 10px;
  padding: 20px 18px;
  border: 1px solid var(--as-line);
  border-radius: var(--as-radius);
  background: var(--as-panel);
}

.as-platform-icon {
  display: inline-flex;
  width: 30px;
  height: 30px;
  color: var(--as-ink);
}

.as-platform-icon :deep(svg) {
  width: 100%;
  height: 100%;
}

.as-platform-name {
  font-size: 15px;
  font-weight: 650;
  color: var(--as-ink);
}

.as-tag {
  padding: 3px 9px;
  border-radius: 999px;
  font-size: 11.5px;
  font-weight: 600;
  line-height: 1.5;
}

.as-tag-full {
  background: rgba(16, 185, 129, 0.14);
  color: #047857;
}

.as-tag-generic {
  background: rgba(245, 158, 11, 0.16);
  color: #b45309;
}

.dark .as-tag-full {
  color: #6ee7b7;
}

.dark .as-tag-generic {
  color: #fcd34d;
}

/* ---------- 3 · bridging ---------- */

.as-flow {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  gap: 22px;
  align-items: center;
  margin-top: 36px;
}

.as-flow-col {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}

.as-flow-head {
  margin: 0 0 4px;
  font-size: 11.5px;
  font-weight: 700;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--as-muted);
}

.as-node {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 14px 16px;
  border: 1px solid var(--as-line);
  border-radius: 11px;
  background: var(--as-panel);
}

.as-node-up {
  border-left: 3px solid var(--as-amber);
}

.as-node-name {
  font-size: 14.5px;
  font-weight: 650;
  color: var(--as-ink);
}

.as-node-path {
  font-size: 12.5px;
  color: var(--vp-c-brand-1);
  background: none;
  padding: 0;
}

.as-node-dialect {
  font-size: 13.5px;
  font-weight: 600;
  color: #b45309;
}

.dark .as-node-dialect {
  color: #fcd34d;
}

.as-node-note {
  font-size: 12px;
  color: var(--as-muted);
}

.as-flow-mid {
  display: flex;
  justify-content: center;
}

.as-proxy {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 22px 20px;
  border-radius: var(--as-radius);
  background: linear-gradient(150deg, #1c1917, #0c0a09);
  border: 1px solid rgba(16, 185, 129, 0.4);
  box-shadow: 0 0 0 5px rgba(16, 185, 129, 0.08);
  text-align: center;
}

.as-proxy-label {
  font-size: 13px;
  font-weight: 650;
  color: #fafaf9;
  max-width: 15ch;
}

.as-proxy-addr {
  font-size: 12px;
  color: #6ee7b7;
  background: none;
  padding: 0;
}

.as-bridgelist {
  margin-top: 34px;
  padding: 22px 24px;
  border: 1px solid var(--as-line);
  border-radius: var(--as-radius);
  background: var(--as-panel);
}

.as-bridgelist ul {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(238px, 1fr));
  gap: 8px 22px;
  margin: 10px 0 0;
  padding: 0;
  list-style: none;
}

.as-bridgelist code {
  font-size: 12.5px;
  color: var(--as-ink);
  background: none;
  padding: 0;
}

/* ---------- 4 · capabilities ---------- */

.as-cap-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
  margin-top: 34px;
}

.as-cap {
  display: block;
  padding: 24px 22px;
  border: 1px solid var(--as-line);
  border-radius: var(--as-radius);
  background: var(--as-panel);
  text-decoration: none;
  transition: border-color 0.15s ease, transform 0.15s ease;
}

.as-cap:hover {
  border-color: var(--vp-c-brand-1);
  transform: translateY(-2px);
}

.as-cap-title {
  margin: 0;
  font-size: 16.5px;
  font-weight: 650;
  color: var(--as-ink);
}

.as-cap-body {
  margin: 10px 0 0;
  font-size: 14px;
  line-height: 1.68;
  color: var(--as-muted);
}

/* ---------- 5 · four shapes ---------- */

.as-form-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 16px;
  margin: 34px 0 0;
  padding: 0;
  list-style: none;
  counter-reset: none;
}

.as-form {
  padding: 22px 20px;
  border: 1px solid var(--as-line);
  border-radius: var(--as-radius);
  background: var(--as-panel);
}

.as-form-idx {
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.08em;
  color: var(--vp-c-brand-1);
}

.as-form-name {
  margin: 10px 0 0;
  font-size: 15.5px;
  font-weight: 650;
  color: var(--as-ink);
}

.as-form-body {
  margin: 8px 0 0;
  font-size: 13.5px;
  line-height: 1.66;
  color: var(--as-muted);
}

/* ---------- 6 · architecture ---------- */

.as-arch {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 34px;
}

.as-layer {
  display: grid;
  grid-template-columns: 150px minmax(0, 1fr);
  gap: 18px;
  align-items: center;
  padding: 16px 20px;
  border: 1px solid var(--as-line);
  border-radius: 11px;
  background: var(--as-panel);
}

.as-layer-name {
  font-size: 14px;
  font-weight: 700;
  color: var(--as-ink);
}

.as-layer-body {
  font-size: 13.5px;
  line-height: 1.6;
  color: var(--as-muted);
}

.as-stack-head {
  margin-top: 34px;
}

.as-stack {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 12px 0 0;
  padding: 0;
  list-style: none;
}

.as-stack li {
  padding: 5px 13px;
  border: 1px solid var(--as-line);
  border-radius: 999px;
  background: var(--as-panel);
  font-size: 12.5px;
  font-weight: 550;
  color: var(--as-muted);
}

/* ---------- 7 · final ---------- */

.as-final {
  padding: 84px 0 92px;
  background:
    radial-gradient(700px 320px at 50% -30%, rgba(16, 185, 129, 0.2), transparent 65%),
    linear-gradient(160deg, #1c1917 0%, #0c0a09 100%);
  color: var(--as-dark-ink);
}

.as-final-inner {
  text-align: center;
}

.as-final-title {
  margin: 0;
  font-size: 32px;
  font-weight: 750;
  letter-spacing: -0.015em;
  color: #fafaf9;
}

.as-final-lead {
  margin: 14px auto 0;
  max-width: 56ch;
  font-size: 15.5px;
  line-height: 1.7;
  color: var(--as-dark-muted);
}

/* ---------- responsive ---------- */

@media (max-width: 1040px) {
  .as-hero-grid {
    grid-template-columns: minmax(0, 1fr);
    gap: 40px;
  }

  .as-hero-title {
    font-size: 40px;
  }

  .as-platform-grid,
  .as-form-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .as-cap-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .as-flow {
    grid-template-columns: minmax(0, 1fr);
    gap: 16px;
  }

  .as-proxy {
    width: 100%;
  }

  .as-proxy-label {
    max-width: none;
  }
}

@media (max-width: 720px) {
  .as-hero {
    padding: 56px 0 62px;
  }

  .as-band {
    padding: 56px 0;
  }

  .as-hero-title {
    font-size: 33px;
  }

  .as-h2 {
    font-size: 25px;
  }

  .as-metrics {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 22px 18px;
  }

  .as-cap-grid {
    grid-template-columns: minmax(0, 1fr);
  }

  .as-layer {
    grid-template-columns: minmax(0, 1fr);
    gap: 6px;
  }

  .as-final-title {
    font-size: 26px;
  }
}

@media (max-width: 460px) {
  .as-wrap {
    padding: 0 18px;
  }

  .as-hero-title {
    font-size: 29px;
  }

  .as-platform-grid,
  .as-form-grid {
    grid-template-columns: minmax(0, 1fr);
  }

  .as-btn {
    width: 100%;
    justify-content: center;
  }
}
</style>
