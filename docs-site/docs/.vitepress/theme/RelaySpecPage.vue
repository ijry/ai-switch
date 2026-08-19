<script setup lang="ts">
/* 对接规范的独立页面外壳。规范正文在 RelaySpec.vue 里，这里只负责页头和返回入口：
   docs/relay/spec.md 与 docs/en/relay/spec.md 都只有 frontmatter + <RelaySpecPage />。
   两个 locale 共用这一份文案字典，改一处必须同时改另一处。 */
import { computed } from "vue";
import { useData, withBase } from "vitepress";
import RelaySpec from "./RelaySpec.vue";

const { localeIndex } = useData();
const isEn = computed(() => localeIndex.value === "en");

// withBase only prepends base when the path starts with a slash. cleanUrls is
// off, so the directory index has to be spelled out as index.html.
const backLink = computed(() =>
  withBase(`${isEn.value ? "/en" : ""}/relay/index.html`),
);

const COPY = {
  root: {
    back: "中转站",
    eyebrow: "面向中转站运营方",
    title: "对接规范",
    lead:
      "中转站可以在自己的控制台放一个「一键添加到 AI Switch」按钮，指向下面这个 scheme 链接。用户点击后 AI Switch 会被唤起，弹出确认框，确认后直接生成一个 API 账号，不需要手抄 base URL 和密钥。",
  },
  en: {
    back: "Relay providers",
    eyebrow: "For relay operators",
    title: "Integration spec",
    lead:
      "A relay provider can put an “Add to AI Switch” button in its own console that points at the scheme URL below. Clicking it launches AI Switch, which shows a confirmation dialog and then creates an API account directly — no copying base URLs and keys by hand.",
  },
} as const;

const t = computed(() => (isEn.value ? COPY.en : COPY.root));
</script>

<template>
  <div class="sp">
    <section class="sp-hero">
      <div class="sp-wrap">
        <a class="sp-back" :href="backLink">
          <svg viewBox="0 0 12 12" aria-hidden="true">
            <path
              d="M7 2.5 3.5 6 7 9.5"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
          {{ t.back }}
        </a>
        <p class="sp-eyebrow">{{ t.eyebrow }}</p>
        <h1 class="sp-title">{{ t.title }}</h1>
        <p class="sp-lead">{{ t.lead }}</p>
      </div>
    </section>

    <section class="sp-body">
      <div class="sp-wrap">
        <RelaySpec />
      </div>
    </section>
  </div>
</template>

<style scoped>
.sp-wrap {
  max-width: 1152px;
  margin: 0 auto;
  padding: 0 24px;
}

.sp-hero {
  padding: 30px 0 32px;
  background:
    radial-gradient(700px 280px at 6% -22%, rgba(16, 185, 129, 0.18), transparent 62%),
    linear-gradient(160deg, #1c1917 0%, #0c0a09 100%);
}

.sp-back {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  margin-bottom: 14px;
  font-size: 13px;
  font-weight: 600;
  text-decoration: none;
  color: #a8a29e;
  transition: color 0.15s;
}

.sp-back:hover {
  color: #6ee7b7;
}

.sp-back svg {
  width: 12px;
  height: 12px;
}

.sp-eyebrow {
  margin: 0 0 8px;
  font-size: 11.5px;
  font-weight: 600;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: #6ee7b7;
}

.sp-title {
  margin: 0;
  border: 0;
  padding: 0;
  font-size: 32px;
  line-height: 1.15;
  font-weight: 800;
  letter-spacing: -0.025em;
  color: #fafaf9;
}

.sp-lead {
  margin: 12px 0 0;
  max-width: 76ch;
  font-size: 14.5px;
  line-height: 1.75;
  color: #a8a29e;
}

.sp-body {
  padding: 44px 0 72px;
  background: var(--as-bg);
  border-top: 1px solid var(--as-line);
}

@media (max-width: 720px) {
  .sp-hero {
    padding: 24px 0 26px;
  }

  .sp-title {
    font-size: 27px;
  }

  .sp-body {
    padding: 34px 0 56px;
  }
}

@media (max-width: 460px) {
  .sp-wrap {
    padding: 0 18px;
  }
}
</style>
