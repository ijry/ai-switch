import { defineConfig, type DefaultTheme } from "vitepress";

const REPO = "https://github.com/ijry/ai-switch";
const RELEASES = `${REPO}/releases/latest`;
// Trailing slash is required: generateSitemap feeds SitemapStream relative paths,
// so a hostname without it silently drops the /ai-switch/ segment from every <loc>.
const HOSTNAME = "https://ijry.github.io/ai-switch/";

/* ----------------------------- 简体中文 ----------------------------- */

const zhNav: DefaultTheme.NavItem[] = [
  { text: "指南", link: "/guide/introduction", activeMatch: "^/guide/" },
  { text: "功能", link: "/features/vibe", activeMatch: "^/features/" },
  { text: "部署", link: "/deploy/desktop", activeMatch: "^/deploy/" },
  { text: "开发", link: "/dev/architecture", activeMatch: "^/dev/" },
  { text: "FAQ", link: "/faq" },
  { text: "下载", link: RELEASES },
];

const zhSidebar: DefaultTheme.Sidebar = {
  "/guide/": [
    {
      text: "入门",
      collapsed: false,
      items: [
        { text: "AI Switch 是什么", link: "/guide/introduction" },
        { text: "安装", link: "/guide/installation" },
        { text: "快速开始", link: "/guide/quick-start" },
      ],
    },
    {
      text: "核心能力",
      collapsed: false,
      items: [
        { text: "平台支持矩阵", link: "/guide/platform-support" },
        { text: "账号与算力池", link: "/guide/accounts" },
        { text: "协议路由与桥接", link: "/guide/protocol-routing" },
        { text: "模型连通性测试", link: "/guide/model-test" },
        { text: "用量与请求统计", link: "/guide/usage-stats" },
        { text: "稳定性与自动恢复", link: "/guide/reliability" },
      ],
    },
  ],
  "/features/": [
    {
      text: "功能详解",
      collapsed: false,
      items: [
        { text: "Vibe 终端与皮肤", link: "/features/vibe" },
        { text: "会话管理", link: "/features/sessions" },
        { text: "MCP 服务器", link: "/features/mcp" },
        { text: "技能管理", link: "/features/skills" },
        { text: "本地小工具", link: "/features/tools" },
      ],
    },
  ],
  "/deploy/": [
    {
      text: "部署",
      collapsed: false,
      items: [
        { text: "桌面端", link: "/deploy/desktop" },
        { text: "Web 服务模式", link: "/deploy/web-service" },
        { text: "独立服务器", link: "/deploy/standalone-server" },
        { text: "远程访问与 HTTPS", link: "/deploy/remote-access" },
      ],
    },
  ],
  "/dev/": [
    {
      text: "开发",
      collapsed: false,
      items: [
        { text: "架构总览", link: "/dev/architecture" },
        { text: "本地开发", link: "/dev/local-setup" },
        { text: "发布流程", link: "/dev/release" },
      ],
    },
  ],
};

/* ------------------------------ English ------------------------------ */

const enNav: DefaultTheme.NavItem[] = [
  { text: "Guide", link: "/en/guide/introduction", activeMatch: "^/en/guide/" },
  { text: "Features", link: "/en/features/vibe", activeMatch: "^/en/features/" },
  { text: "Deploy", link: "/en/deploy/desktop", activeMatch: "^/en/deploy/" },
  { text: "Develop", link: "/en/dev/architecture", activeMatch: "^/en/dev/" },
  { text: "FAQ", link: "/en/faq" },
  { text: "Download", link: RELEASES },
];

// Sidebar keys must carry the /en/ prefix. getSidebar matches on relativePath,
// so a "/guide/" key would never match en/guide/*.md and English pages would
// silently render without a sidebar.
const enSidebar: DefaultTheme.Sidebar = {
  "/en/guide/": [
    {
      text: "Getting Started",
      collapsed: false,
      items: [
        { text: "What is AI Switch", link: "/en/guide/introduction" },
        { text: "Installation", link: "/en/guide/installation" },
        { text: "Quick Start", link: "/en/guide/quick-start" },
      ],
    },
    {
      text: "Core Capabilities",
      collapsed: false,
      items: [
        { text: "Platform Support Matrix", link: "/en/guide/platform-support" },
        { text: "Accounts and the Pool", link: "/en/guide/accounts" },
        { text: "Protocol Routing and Bridging", link: "/en/guide/protocol-routing" },
        { text: "Model Connectivity Tests", link: "/en/guide/model-test" },
        { text: "Usage and Request Stats", link: "/en/guide/usage-stats" },
        { text: "Reliability and Auto Recovery", link: "/en/guide/reliability" },
      ],
    },
  ],
  "/en/features/": [
    {
      text: "Features",
      collapsed: false,
      items: [
        { text: "Vibe Terminal and Skins", link: "/en/features/vibe" },
        { text: "Session Management", link: "/en/features/sessions" },
        { text: "MCP Servers", link: "/en/features/mcp" },
        { text: "Skills Management", link: "/en/features/skills" },
        { text: "Local Utilities", link: "/en/features/tools" },
      ],
    },
  ],
  "/en/deploy/": [
    {
      text: "Deploy",
      collapsed: false,
      items: [
        { text: "Desktop", link: "/en/deploy/desktop" },
        { text: "Web Service Mode", link: "/en/deploy/web-service" },
        { text: "Standalone Server", link: "/en/deploy/standalone-server" },
        { text: "Remote Access and HTTPS", link: "/en/deploy/remote-access" },
      ],
    },
  ],
  "/en/dev/": [
    {
      text: "Develop",
      collapsed: false,
      items: [
        { text: "Architecture", link: "/en/dev/architecture" },
        { text: "Local Setup", link: "/en/dev/local-setup" },
        { text: "Release Process", link: "/en/dev/release" },
      ],
    },
  ],
};

/* ------------------------------- Config ------------------------------- */

export default defineConfig({
  base: "/ai-switch/",
  // GitHub Pages cannot map /foo to foo.html, so clean URLs would 404 site-wide.
  cleanUrls: false,
  lastUpdated: true,
  metaChunk: true,
  srcExclude: ["README.md", "**/_*.md"],

  // head is rendered verbatim — base is NOT prepended, so absolute paths are
  // spelled out. Keep name/property as the first key: meta dedup uses it.
  head: [
    ["link", { rel: "icon", type: "image/svg+xml", href: "/ai-switch/favicon.svg" }],
    ["meta", { name: "theme-color", content: "#10b981" }],
    ["meta", { property: "og:type", content: "website" }],
    ["meta", { property: "og:site_name", content: "AI Switch" }],
  ],

  sitemap: { hostname: HOSTNAME },

  locales: {
    root: {
      label: "简体中文",
      lang: "zh-CN",
      title: "AI Switch",
      titleTemplate: ":title | AI Switch",
      description:
        "AI Switch 是一个开源的 AI 供应商与账号切换工具，支持 Codex、Claude Code、Gemini CLI、Grok 等 7 个平台，提供算力池调度、四种上游协议桥接、桌面端与自托管 Web 服务。",
      head: [
        [
          "meta",
          {
            name: "keywords",
            content:
              "AI Switch,账号切换,算力池,协议桥接,Codex,Claude Code,Gemini CLI,Grok,API 路由,Tauri,开源,自托管",
          },
        ],
        ["meta", { property: "og:locale", content: "zh_CN" }],
      ],
      themeConfig: {
        // themeConfig is shallow-merged per locale, so nav/sidebar/footer and
        // every UI string must live inside the locale block in full.
        nav: zhNav,
        sidebar: zhSidebar,
        outline: { level: [2, 3], label: "本页目录" },
        docFooter: { prev: "上一页", next: "下一页" },
        editLink: {
          pattern: `${REPO}/edit/main/docs-site/docs/:path`,
          text: "在 GitHub 上编辑此页",
        },
        lastUpdated: {
          text: "最后更新于",
          formatOptions: { dateStyle: "short", timeStyle: "short" },
        },
        returnToTopLabel: "回到顶部",
        sidebarMenuLabel: "菜单",
        darkModeSwitchLabel: "外观",
        lightModeSwitchTitle: "切换到浅色模式",
        darkModeSwitchTitle: "切换到深色模式",
        langMenuLabel: "切换语言",
        skipToContentLabel: "跳到主要内容",
        notFound: {
          title: "页面不存在",
          quote: "你访问的页面可能已被移动，或者从未存在过。",
          linkLabel: "返回首页",
          linkText: "返回首页",
        },
        footer: {
          message: "基于 MIT 许可发布",
          copyright: "Copyright © 2026 xyito",
        },
      },
    },

    en: {
      label: "English",
      lang: "en-US",
      link: "/en/",
      title: "AI Switch",
      titleTemplate: ":title | AI Switch",
      description:
        "AI Switch is an open-source desktop and self-hosted web app for AI provider and account switching, with pool scheduling, four upstream protocol bridges, and support for Codex, Claude Code, Gemini CLI, Grok and more.",
      head: [
        [
          "meta",
          {
            name: "keywords",
            content:
              "AI Switch,account switching,credential pool,protocol bridging,Codex,Claude Code,Gemini CLI,Grok,API routing,Tauri,open source,self-hosted",
          },
        ],
        ["meta", { property: "og:locale", content: "en_US" }],
      ],
      themeConfig: {
        nav: enNav,
        sidebar: enSidebar,
        outline: { level: [2, 3], label: "On this page" },
        editLink: {
          pattern: `${REPO}/edit/main/docs-site/docs/:path`,
          text: "Edit this page on GitHub",
        },
        lastUpdated: {
          text: "Last updated",
          formatOptions: { dateStyle: "short", timeStyle: "short" },
        },
        footer: {
          message: "Released under the MIT License.",
          copyright: "Copyright © 2026 xyito",
        },
      },
    },
  },

  // Only genuinely shared keys belong here — anything locale-specific would be
  // clobbered by the shallow merge above.
  themeConfig: {
    logo: "/logo.svg",
    socialLinks: [{ icon: "github", link: REPO }],
    search: {
      provider: "local",
      options: {
        miniSearch: {
          options: {
            // MiniSearch splits on non-alphanumerics, which turns a whole
            // Chinese phrase into one token. Emit per-character tokens too so
            // Chinese search actually returns hits.
            tokenize: (text: string) =>
              text
                .split(/[^\p{L}\p{N}]+/u)
                .flatMap((word) =>
                  /[一-龥]/.test(word) ? [word, ...word.split("")] : [word],
                )
                .filter(Boolean),
            processTerm: (term: string) => term.toLowerCase(),
          },
          searchOptions: { fuzzy: 0.2, prefix: true },
        },
        locales: {
          // localeIndex for the default language is the literal string "root".
          root: {
            translations: {
              button: { buttonText: "搜索文档", buttonAriaLabel: "搜索文档" },
              modal: {
                displayDetails: "显示详细列表",
                resetButtonTitle: "清除查询条件",
                backButtonTitle: "关闭搜索",
                noResultsText: "无法找到相关结果",
                footer: {
                  selectText: "选择",
                  navigateText: "切换",
                  closeText: "关闭",
                },
              },
            },
          },
        },
      },
    },
  },

  transformPageData(pageData) {
    const canonical =
      HOSTNAME +
      pageData.relativePath.replace(/(^|\/)index\.md$/, "$1").replace(/\.md$/, ".html");

    pageData.frontmatter.head ??= [];
    pageData.frontmatter.head.push(
      ["link", { rel: "canonical", href: canonical }],
      ["meta", { property: "og:url", content: canonical }],
      ["meta", { property: "og:title", content: pageData.title ?? "AI Switch" }],
      ["meta", { property: "og:description", content: pageData.description ?? "" }],
    );
  },
});
