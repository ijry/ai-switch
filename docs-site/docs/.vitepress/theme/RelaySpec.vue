<script setup lang="ts">
/* 对接规范 —— 与 src-tauri 的实现一一对应，不要凭印象改。
   每一条都能在代码里找到出处：
   - scheme 注册：src-tauri/tauri.conf.json 的 plugins.deep-link.desktop.schemes
   - 受理判定：src-tauri/src/lib.rs 的 is_deeplink_url
   - 解析与校验：src-tauri/src/services/deeplink_service.rs 的 parse_deeplink_url
   - app 别名：src-tauri/src/models/platform.rs 的 PlatformId::parse
   - 落库字段：deeplink_service.rs 的 to_create_api_input
   - 确认弹窗展示项：src/components/deeplink/DeepLinkImportDialog.tsx
   改动这些代码时，这一页必须同步改。 */
import { computed, ref } from "vue";
import { useData } from "vitepress";

const { localeIndex } = useData();
const isEn = computed(() => localeIndex.value === "en");

const EXAMPLE_CLAUDE =
  "aiswitch://v1/import?resource=provider&app=claude&name=Example%20Relay" +
  "&endpoint=https%3A%2F%2Fapi.example.com%2Fv1&apiKey=sk-xxxxxxxx" +
  "&sonnetModel=claude-sonnet-5&opusModel=claude-opus-4-8";

const EXAMPLE_CODEX =
  "aiswitch://v1/import?resource=provider&app=codex&name=Example%20Relay" +
  "&endpoint=https%3A%2F%2Fapi.example.com%2Fv1&apiKey=sk-xxxxxxxx" +
  "&model=gpt-5.6-sol";

const COPY = {
  root: {
    anatomyTitle: "URL 结构",
    anatomyNote:
      "scheme、版本段、路径三者是固定值，写错任何一个都会被拒绝解析。所有参数值必须做 URL 百分号编码。",
    anatomy: [
      { seg: "aiswitch://", label: "scheme，固定" },
      { seg: "v1", label: "协议版本，只接受 v1" },
      { seg: "/import", label: "路径，固定" },
      { seg: "?resource=provider", label: "资源类型，只接受 provider" },
      { seg: "&app=…", label: "目标平台" },
      { seg: "&name=…", label: "账号显示名" },
      { seg: "&endpoint=…", label: "base URL" },
      { seg: "&apiKey=…", label: "API 密钥" },
    ],

    paramsTitle: "参数",
    thName: "参数",
    thRequired: "必填",
    thDesc: "说明",
    yes: "必填",
    no: "可选",
    params: [
      {
        name: "resource",
        required: true,
        desc: "只接受 provider。其他值会返回「暂不支持的 resource」。",
      },
      {
        name: "app",
        required: true,
        desc: "目标平台，取值见下方对照表。opencode / openclaw / hermes 虽然是合法平台名，但不支持深链接导入，会被能力校验拒绝。",
      },
      {
        name: "name",
        required: true,
        desc: "账号在列表里的显示名。会做 trim，trim 后不能为空。建议直接写站点名，便于用户识别。",
      },
      {
        name: "endpoint",
        required: true,
        desc: "中转站的 API base URL，必须是 http 或 https。可以用逗号分隔写多个候选，取第一个能解析成 http(s) 的；其余会被忽略。",
      },
      {
        name: "apiKey",
        required: true,
        desc: "API 密钥。trim 后不能为空。确认框里只显示掩码（首 4 位 + *** + 末 4 位），不会明文展示。",
      },
      {
        name: "model",
        required: false,
        desc: "单模型映射，用于 codex / gemini / grok。填中转站真实可用的模型 ID。",
      },
      {
        name: "haikuModel · sonnetModel · opusModel",
        required: false,
        desc: "Claude 的三档模型映射，可以只填其中一个或两个。app 不是 claude 时这三个参数会被忽略。",
      },
      {
        name: "homepage · notes",
        required: false,
        desc: "会被解析，但当前版本的确认框不展示、导入时也不落库。填了不报错，但也不会有任何效果。",
      },
    ],

    dialectTitle: "app 与上游协议的对应关系",
    dialectLead:
      "上游协议不是 URL 参数，由 app 直接决定，链接里不用也不能指定。这意味着一个中转站如果对某个平台只兼容另一种接口格式，就不能用深链接接入，得让用户手动建账号。",
    thApp: "app 取值",
    thPlatform: "平台",
    thDialect: "上游协议",
    dialects: [
      { app: "codex · openai · chatgpt", platform: "Codex", dialect: "openai-responses" },
      {
        app: "claude · anthropic · claude-code · claude-desktop",
        platform: "Claude Code",
        dialect: "anthropic",
      },
      { app: "gemini · google · gemini-cli", platform: "Gemini CLI", dialect: "gemini" },
      { app: "grok · xai · x-ai · x.ai", platform: "Grok", dialect: "openai" },
    ],
    aliasNote:
      "别名匹配前会先 trim、转小写，并把空格和连字符统一成下划线，所以 Claude-Code 和 claude_code 等价。",

    modelTitle: "模型映射怎么填",
    modelLead:
      "模型参数写的是「上游真实模型 ID」，AI Switch 会把客户端固定请求的那个模型名映射到它。左边是客户端请求名，右边是你填进去的值。",
    modelClaudeHead: "app=claude",
    modelOtherHead: "app=codex / gemini / grok",
    modelClaude: [
      { param: "haikuModel", from: "claude-haiku-4-5" },
      { param: "sonnetModel", from: "claude-sonnet-5" },
      { param: "opusModel", from: "claude-opus-4-8" },
    ],
    modelOther: [
      { param: "model", from: "gpt-5", note: "app=codex" },
      { param: "model", from: "gemini-2.5-flash", note: "app=gemini" },
      { param: "model", from: "grok-3", note: "app=grok" },
    ],
    modelNote:
      "模型参数可以完全不填，导入后由用户自己在账号里加映射。留空不影响导入成功。",

    exampleTitle: "示例",
    exampleClaudeLabel: "Claude Code，两档模型映射",
    exampleCodexLabel: "Codex，单模型映射",
    copy: "复制",
    copied: "已复制",

    flowTitle: "用户点下去之后发生什么",
    flow: [
      "系统按注册的 scheme 唤起 AI Switch；应用已在运行则复用现有窗口并前置。",
      "解析链接。任何一步校验不过，弹出错误提示，不会创建任何东西。",
      "弹出确认框，列出平台、名称、base URL、密钥掩码、模型映射条数与来源 scheme。",
      "确认框里有「导入后加入算力池」勾选项，默认勾选。用户确认后才落库。",
    ],

    limitsTitle: "限制",
    limits: [
      "一条链接只能导入一个账号。没有批量格式，多个账号就给多个按钮。",
      "只能建 API 类型账号，不能导入官方登录态账号。",
      "自定义请求头、自定义密钥字段名、1M 上下文开关都不在协议里，需要这些的账号只能手动建。",
      "aiswitch:// 由桌面端注册，Web 服务模式下打开的页面不会响应这个 scheme。",
      "另有 ccswitch:// 兼容导入协议，默认关闭，需要用户自己在设置里开启，且仅 Windows 与 Linux 可用。中转站请一律用 aiswitch://。",
    ],

    securityTitle: "链接里带着明文密钥",
    securityBody:
      "整条 URL 里的 apiKey 是明文。请只在用户登录后的私有页面里生成，不要放进公开文档、群聊或分享图片；也不要在服务端日志里记录完整链接。AI Switch 侧只在确认框里显示掩码，并在错误信息与日志中对 apiKey 做脱敏，但链接离开你的站点之后就不在任何一方的控制范围内了。",
  },

  en: {
    anatomyTitle: "URL anatomy",
    anatomyNote:
      "The scheme, version segment, and path are fixed values; getting any of them wrong means the link is rejected outright. Every parameter value must be percent-encoded.",
    anatomy: [
      { seg: "aiswitch://", label: "scheme, fixed" },
      { seg: "v1", label: "protocol version, only v1" },
      { seg: "/import", label: "path, fixed" },
      { seg: "?resource=provider", label: "resource, only provider" },
      { seg: "&app=…", label: "target platform" },
      { seg: "&name=…", label: "account display name" },
      { seg: "&endpoint=…", label: "base URL" },
      { seg: "&apiKey=…", label: "API key" },
    ],

    paramsTitle: "Parameters",
    thName: "Parameter",
    thRequired: "Required",
    thDesc: "Notes",
    yes: "Required",
    no: "Optional",
    params: [
      {
        name: "resource",
        required: true,
        desc: "Only provider is accepted. Anything else returns an unsupported-resource error.",
      },
      {
        name: "app",
        required: true,
        desc: "Target platform — see the table below. opencode / openclaw / hermes are valid platform names but do not support deep-link import, so the capability check rejects them.",
      },
      {
        name: "name",
        required: true,
        desc: "Display name in the account list. Trimmed, and must not be empty after trimming. Use your site name so users can identify it.",
      },
      {
        name: "endpoint",
        required: true,
        desc: "The relay's API base URL; must be http or https. You may pass a comma-separated list of candidates — the first one that parses as http(s) wins and the rest are ignored.",
      },
      {
        name: "apiKey",
        required: true,
        desc: "The API key. Must not be empty after trimming. The dialog shows a mask only (first 4 + *** + last 4), never the key itself.",
      },
      {
        name: "model",
        required: false,
        desc: "Single model mapping, for codex / gemini / grok. Use a model ID that actually works on your relay.",
      },
      {
        name: "haikuModel · sonnetModel · opusModel",
        required: false,
        desc: "Claude's three model tiers. You can pass just one or two. These are ignored when app is not claude.",
      },
      {
        name: "homepage · notes",
        required: false,
        desc: "Parsed, but the current dialog does not display them and import does not persist them. Passing them is not an error, it simply has no effect.",
      },
    ],

    dialectTitle: "app to upstream protocol",
    dialectLead:
      "The upstream protocol is not a URL parameter — app determines it, and the link cannot override it. So if your relay is only compatible with a different interface format for a given platform, deep-link import is not usable there and users have to create the account by hand.",
    thApp: "app value",
    thPlatform: "Platform",
    thDialect: "Upstream protocol",
    dialects: [
      { app: "codex · openai · chatgpt", platform: "Codex", dialect: "openai-responses" },
      {
        app: "claude · anthropic · claude-code · claude-desktop",
        platform: "Claude Code",
        dialect: "anthropic",
      },
      { app: "gemini · google · gemini-cli", platform: "Gemini CLI", dialect: "gemini" },
      { app: "grok · xai · x-ai · x.ai", platform: "Grok", dialect: "openai" },
    ],
    aliasNote:
      "Aliases are matched after trimming, lowercasing, and normalizing spaces and hyphens to underscores, so Claude-Code and claude_code are equivalent.",

    modelTitle: "Filling in model mappings",
    modelLead:
      "A model parameter carries the real upstream model ID. AI Switch maps the fixed model name the client asks for onto it. Left is the client-side name, right is the value you supply.",
    modelClaudeHead: "app=claude",
    modelOtherHead: "app=codex / gemini / grok",
    modelClaude: [
      { param: "haikuModel", from: "claude-haiku-4-5" },
      { param: "sonnetModel", from: "claude-sonnet-5" },
      { param: "opusModel", from: "claude-opus-4-8" },
    ],
    modelOther: [
      { param: "model", from: "gpt-5", note: "app=codex" },
      { param: "model", from: "gemini-2.5-flash", note: "app=gemini" },
      { param: "model", from: "grok-3", note: "app=grok" },
    ],
    modelNote:
      "Model parameters are entirely optional — leave them out and the user adds mappings themselves after import. An empty mapping does not affect import.",

    exampleTitle: "Examples",
    exampleClaudeLabel: "Claude Code, two model tiers",
    exampleCodexLabel: "Codex, single model",
    copy: "Copy",
    copied: "Copied",

    flowTitle: "What happens when a user clicks",
    flow: [
      "The OS launches AI Switch via the registered scheme; if it is already running, the existing window is reused and brought to the front.",
      "The link is parsed. If any check fails, an error is shown and nothing is created.",
      "A confirmation dialog lists the platform, name, base URL, masked key, number of model mappings, and the source scheme.",
      "The dialog has an “add to the pool after import” checkbox, ticked by default. Nothing is written until the user confirms.",
    ],

    limitsTitle: "Limits",
    limits: [
      "One link imports exactly one account. There is no batch format — render one button per account.",
      "API accounts only; official signed-in accounts cannot be imported this way.",
      "Custom headers, a custom API-key field name, and the 1M-context flag are not part of the protocol. Accounts that need those must be created manually.",
      "aiswitch:// is registered by the desktop app, so a page opened against the web service mode will not respond to it.",
      "There is also a ccswitch:// compatibility import protocol. It is off by default, has to be enabled by the user in settings, and is only available on Windows and Linux. Relay providers should always use aiswitch://.",
    ],

    securityTitle: "The link carries a key in plain text",
    securityBody:
      "apiKey sits in the URL in plain text. Generate these links only on pages behind the user's own login; do not put them in public documentation, group chats, or screenshots, and do not log the complete URL server-side. AI Switch shows only a mask in the dialog and redacts apiKey in errors and logs — but once the link leaves your site it is outside anyone's control.",
  },
} as const;

const t = computed(() => (isEn.value ? COPY.en : COPY.root));

const copiedKey = ref<string | null>(null);

async function copy(key: string, text: string) {
  try {
    await navigator.clipboard.writeText(text);
    copiedKey.value = key;
    window.setTimeout(() => {
      if (copiedKey.value === key) copiedKey.value = null;
    }, 1600);
  } catch {
    // Clipboard access can be denied outright (insecure origin, permission
    // policy). Leave the button label unchanged rather than claim success.
  }
}
</script>

<template>
  <section class="rs">
    <!-- URL anatomy -->
    <div class="rs-block rs-block-first">
      <h3 class="rs-h3">{{ t.anatomyTitle }}</h3>
      <div class="rs-anatomy">
        <div v-for="a in t.anatomy" :key="a.seg" class="rs-seg">
          <code class="rs-seg-code">{{ a.seg }}</code>
          <span class="rs-seg-label">{{ a.label }}</span>
        </div>
      </div>
      <p class="rs-note">{{ t.anatomyNote }}</p>
    </div>

    <!-- Parameters -->
    <div class="rs-block">
      <h3 class="rs-h3">{{ t.paramsTitle }}</h3>
      <div class="rs-table rs-table-params">
        <div class="rs-tr rs-th">
          <span>{{ t.thName }}</span>
          <span>{{ t.thRequired }}</span>
          <span>{{ t.thDesc }}</span>
        </div>
        <div v-for="p in t.params" :key="p.name" class="rs-tr">
          <code class="rs-param">{{ p.name }}</code>
          <span>
            <span class="rs-pill" :class="p.required ? 'rs-pill-req' : 'rs-pill-opt'">
              {{ p.required ? t.yes : t.no }}
            </span>
          </span>
          <span class="rs-desc">{{ p.desc }}</span>
        </div>
      </div>
    </div>

    <!-- app -> dialect -->
    <div class="rs-block">
      <h3 class="rs-h3">{{ t.dialectTitle }}</h3>
      <p class="rs-sub">{{ t.dialectLead }}</p>
      <div class="rs-table rs-table-dialect">
        <div class="rs-tr rs-th">
          <span>{{ t.thApp }}</span>
          <span>{{ t.thPlatform }}</span>
          <span>{{ t.thDialect }}</span>
        </div>
        <div v-for="d in t.dialects" :key="d.platform" class="rs-tr">
          <code class="rs-param">{{ d.app }}</code>
          <span class="rs-desc">{{ d.platform }}</span>
          <code class="rs-dialect">{{ d.dialect }}</code>
        </div>
      </div>
      <p class="rs-note">{{ t.aliasNote }}</p>
    </div>

    <!-- Model mappings -->
    <div class="rs-block">
      <h3 class="rs-h3">{{ t.modelTitle }}</h3>
      <p class="rs-sub">{{ t.modelLead }}</p>
      <div class="rs-model-grid">
        <div class="rs-model-card">
          <p class="rs-model-head"><code>{{ t.modelClaudeHead }}</code></p>
          <div v-for="m in t.modelClaude" :key="m.param" class="rs-map">
            <code class="rs-map-from">{{ m.from }}</code>
            <span class="rs-map-arrow">→</span>
            <code class="rs-map-param">{{ m.param }}</code>
          </div>
        </div>
        <div class="rs-model-card">
          <p class="rs-model-head"><code>{{ t.modelOtherHead }}</code></p>
          <div v-for="m in t.modelOther" :key="m.note" class="rs-map">
            <code class="rs-map-from">{{ m.from }}</code>
            <span class="rs-map-arrow">→</span>
            <code class="rs-map-param">{{ m.param }}</code>
            <span class="rs-map-note">{{ m.note }}</span>
          </div>
        </div>
      </div>
      <p class="rs-note">{{ t.modelNote }}</p>
    </div>

    <!-- Examples -->
    <div class="rs-block">
      <h3 class="rs-h3">{{ t.exampleTitle }}</h3>
      <div class="rs-example">
        <div class="rs-example-bar">
          <span class="rs-example-label">{{ t.exampleClaudeLabel }}</span>
          <button class="rs-copy" type="button" @click="copy('claude', EXAMPLE_CLAUDE)">
            {{ copiedKey === "claude" ? t.copied : t.copy }}
          </button>
        </div>
        <code class="rs-example-code">{{ EXAMPLE_CLAUDE }}</code>
      </div>
      <div class="rs-example">
        <div class="rs-example-bar">
          <span class="rs-example-label">{{ t.exampleCodexLabel }}</span>
          <button class="rs-copy" type="button" @click="copy('codex', EXAMPLE_CODEX)">
            {{ copiedKey === "codex" ? t.copied : t.copy }}
          </button>
        </div>
        <code class="rs-example-code">{{ EXAMPLE_CODEX }}</code>
      </div>
    </div>

    <!-- Flow + limits -->
    <div class="rs-two">
      <div class="rs-block rs-block-flush">
        <h3 class="rs-h3">{{ t.flowTitle }}</h3>
        <ol class="rs-steps">
          <li v-for="(s, i) in t.flow" :key="i">{{ s }}</li>
        </ol>
      </div>
      <div class="rs-block rs-block-flush">
        <h3 class="rs-h3">{{ t.limitsTitle }}</h3>
        <ul class="rs-limits">
          <li v-for="l in t.limits" :key="l">{{ l }}</li>
        </ul>
      </div>
    </div>

    <!-- Security -->
    <aside class="rs-security">
      <p class="rs-security-head">{{ t.securityTitle }}</p>
      <p class="rs-security-body">{{ t.securityBody }}</p>
    </aside>
  </section>
</template>

<style scoped>
.rs {
  margin: 0;
}

.rs-h3 {
  margin: 0 0 10px;
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0.07em;
  text-transform: uppercase;
  color: var(--as-ink);
}

.rs-sub {
  margin: 0 0 16px;
  max-width: 82ch;
  font-size: 14.5px;
  line-height: 1.75;
  color: var(--as-muted);
}

.rs-note {
  margin: 14px 0 0;
  max-width: 84ch;
  padding-left: 12px;
  border-left: 2px solid var(--as-line);
  font-size: 13px;
  line-height: 1.7;
  color: var(--as-muted);
}

.rs-block {
  margin-top: 40px;
}

.rs-block-first {
  margin-top: 0;
}

.rs-block-flush {
  margin-top: 0;
}

/* ---------- URL anatomy ---------- */

.rs-anatomy {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 18px;
  border: 1px solid var(--as-line);
  border-radius: 12px;
  background: var(--as-panel);
}

.rs-seg {
  display: flex;
  flex-direction: column;
  gap: 5px;
  min-width: 0;
}

.rs-seg-code {
  padding: 6px 10px;
  border-radius: 7px;
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
  line-height: 1.4;
  color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
  white-space: nowrap;
}

.rs-seg-label {
  padding-left: 2px;
  font-size: 11.5px;
  line-height: 1.4;
  color: var(--as-muted);
}

/* ---------- tables ---------- */

.rs-table {
  border: 1px solid var(--as-line);
  border-radius: 12px;
  overflow: hidden;
  background: var(--as-panel);
}

.rs-tr {
  display: grid;
  gap: 16px;
  align-items: start;
  padding: 14px 18px;
  border-top: 1px solid var(--as-line);
}

.rs-tr:first-child {
  border-top: 0;
}

.rs-table-params .rs-tr {
  grid-template-columns: minmax(0, 200px) 92px minmax(0, 1fr);
}

.rs-table-dialect .rs-tr {
  grid-template-columns: minmax(0, 1fr) minmax(0, 160px) minmax(0, 200px);
}

.rs-th {
  padding-top: 11px;
  padding-bottom: 11px;
  font-size: 11.5px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--as-muted);
  background: var(--as-bg-alt);
}

.rs-param,
.rs-dialect {
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
  line-height: 1.55;
  overflow-wrap: anywhere;
}

.rs-param {
  color: var(--as-ink);
  font-weight: 600;
}

.rs-dialect {
  color: var(--vp-c-brand-1);
}

.rs-desc {
  font-size: 14px;
  line-height: 1.7;
  color: var(--as-muted);
}

.rs-pill {
  display: inline-block;
  padding: 2px 9px;
  border-radius: 999px;
  font-size: 11.5px;
  font-weight: 700;
  white-space: nowrap;
}

.rs-pill-req {
  color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
}

.rs-pill-opt {
  color: var(--as-muted);
  background: var(--as-bg-alt);
}

/* ---------- model mappings ---------- */

.rs-model-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  gap: 16px;
}

.rs-model-card {
  padding: 18px;
  border: 1px solid var(--as-line);
  border-radius: 12px;
  background: var(--as-panel);
}

.rs-model-head {
  margin: 0 0 12px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--as-line);
}

.rs-model-head code {
  font-family: var(--vp-font-family-mono);
  font-size: 12.5px;
  font-weight: 700;
  color: var(--as-ink);
}

.rs-map {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 8px;
  padding: 5px 0;
  font-family: var(--vp-font-family-mono);
  font-size: 12.5px;
  line-height: 1.6;
}

.rs-map-from {
  color: var(--as-muted);
  overflow-wrap: anywhere;
}

.rs-map-arrow {
  color: var(--as-line);
  font-family: var(--vp-font-family-base);
}

.rs-map-param {
  color: var(--vp-c-brand-1);
  font-weight: 600;
  overflow-wrap: anywhere;
}

.rs-map-note {
  margin-left: auto;
  font-family: var(--vp-font-family-base);
  font-size: 11.5px;
  color: var(--as-muted);
}

/* ---------- examples ---------- */

.rs-example {
  border: 1px solid var(--as-line);
  border-radius: 12px;
  overflow: hidden;
  background: var(--as-panel);
}

.rs-example + .rs-example {
  margin-top: 12px;
}

.rs-example-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--as-line);
  background: var(--as-bg-alt);
}

.rs-example-label {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--as-muted);
}

.rs-copy {
  flex-shrink: 0;
  padding: 4px 12px;
  border: 1px solid var(--as-line);
  border-radius: 999px;
  font-size: 12px;
  font-weight: 600;
  color: var(--as-muted);
  background: var(--as-panel);
  cursor: pointer;
  transition: color 0.15s, border-color 0.15s;
}

.rs-copy:hover {
  color: var(--vp-c-brand-1);
  border-color: var(--vp-c-brand-1);
}

.rs-example-code {
  display: block;
  padding: 16px 18px;
  font-family: var(--vp-font-family-mono);
  font-size: 12.5px;
  line-height: 1.85;
  color: var(--as-ink);
  overflow-wrap: anywhere;
}

/* ---------- flow + limits ---------- */

.rs-two {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 32px;
  margin-top: 40px;
}

/* base.css sets `ol, ul { list-style: none }` globally, so the markers have to
   be turned back on explicitly here or the ::marker rule below is dead. */
.rs-steps,
.rs-limits {
  margin: 0;
  padding-left: 20px;
  font-size: 14px;
  line-height: 1.8;
  color: var(--as-muted);
}

.rs-steps {
  list-style: decimal;
}

.rs-limits {
  list-style: disc;
}

.rs-steps li + li,
.rs-limits li + li {
  margin-top: 8px;
}

.rs-steps li::marker {
  font-weight: 700;
  color: var(--vp-c-brand-1);
}

/* ---------- security ---------- */

.rs-security {
  margin-top: 40px;
  padding: 20px 22px;
  border: 1px solid var(--vp-c-warning-2);
  border-left-width: 3px;
  border-radius: 12px;
  background: var(--vp-c-warning-soft);
}

.rs-security-head {
  margin: 0 0 8px;
  font-size: 14.5px;
  font-weight: 700;
  color: var(--as-ink);
}

.rs-security-body {
  margin: 0;
  max-width: 88ch;
  font-size: 14px;
  line-height: 1.8;
  color: var(--as-muted);
}

@media (max-width: 860px) {
  .rs-table-params .rs-tr,
  .rs-table-dialect .rs-tr {
    grid-template-columns: minmax(0, 1fr);
    gap: 8px;
  }

  .rs-th {
    display: none;
  }
}

@media (max-width: 640px) {
  .rs-block {
    margin-top: 32px;
  }
}
</style>
