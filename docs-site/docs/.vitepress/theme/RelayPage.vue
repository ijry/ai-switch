<script setup lang="ts">
/* 中转站页面的唯一入口。这一页整体是自定义组件，不走 Markdown：
   docs/relay/index.md 与 docs/en/relay/index.md 都只有 frontmatter + <RelayPage />。
   两个 locale 共用这一份文案字典，改一处必须同时改另一处。

   风险提示走弹窗（原生 <dialog>），对接规范在独立页面 /relay/spec.html，
   目的都是把首屏留给卡片。 */
import { computed, ref } from "vue";
import { useData, withBase } from "vitepress";
import { RELAYS } from "./relays";
import RelayCards from "./RelayCards.vue";

const { localeIndex } = useData();
const isEn = computed(() => localeIndex.value === "en");

// withBase only prepends base when the path starts with a slash.
const specLink = computed(() => withBase(`${isEn.value ? "/en" : ""}/relay/spec.html`));

const COPY = {
  root: {
    eyebrow: "第三方服务 · 非本项目运营",
    title: "中转站",
    lead: "实测可用的第三方 AI 中转站清单。这些站点不由本项目运营，用之前先看风险提示。",
    openRisk: "先读这一段",
    jumpSpec: "对接规范",

    riskTitle: "先读这一段",
    riskLead: "收录只代表某个时间点实测能用，不构成任何形式的担保或推荐承诺。",
    riskClose: "知道了",
    riskCloseLabel: "关闭",
    risks: [
      {
        head: "额度、倍率、邀请规则随时会变",
        body: "站点也可能限流、跑路或直接关停。卡片上标了实测日期，超过三个月请以站点自己的公告为准。",
      },
      {
        head: "中转站能看到你发过去的全部请求内容",
        body: "涉密代码、生产环境凭据、客户数据不要经第三方中转，这跟用哪一家无关。",
      },
      {
        head: "公益站按「随时可能停」来规划",
        body: "别把它当主力，更别让它成为唯一的路。",
      },
      {
        head: "出问题请找站点自己的支持渠道",
        body: "本项目无法代为处理充值、退款或封号申诉。",
      },
    ],

    stationsTitle: "实测站点",
    stationsCount: (n: number) => `共 ${n} 个`,
    hoverHint: "鼠标移到卡片上看须知与提示",
  },

  en: {
    eyebrow: "Third-party services · not operated by this project",
    title: "Relay Providers",
    lead:
      "Hand-tested third-party AI relay providers. None of them are operated by this project — read the risk notice before you use one.",
    openRisk: "Read this first",
    jumpSpec: "Integration spec",

    riskTitle: "Read this first",
    riskLead:
      "Listing a provider only means it was tested working at a point in time; it is not a warranty or an endorsement.",
    riskClose: "Got it",
    riskCloseLabel: "Close",
    risks: [
      {
        head: "Credit, rates, and referral terms change without notice",
        body: "A provider may also throttle, disappear, or shut down. Each card carries its verification date — past three months, trust the provider's own announcements over this page.",
      },
      {
        head: "A relay can see the full content of every request you send through it",
        body: "Confidential code, production credentials, and customer data should not go through a third-party relay, regardless of which one.",
      },
      {
        head: "Treat a public-benefit relay as something that can stop at any time",
        body: "Don't make it your primary, and never make it your only path.",
      },
      {
        head: "For problems, use the provider's own support channels",
        body: "This project cannot handle payments, refunds, or ban appeals on your behalf.",
      },
    ],

    stationsTitle: "Verified providers",
    stationsCount: (n: number) => `${n} total`,
    hoverHint: "Hover a card for notes and tips",
  },
} as const;

const t = computed(() => (isEn.value ? COPY.en : COPY.root));

const riskDialog = ref<HTMLDialogElement | null>(null);

// showModal gives us the focus trap, Esc handling and inert background for
// free; the ref is only ever touched from a click handler, so SSR never sees
// it. The dialog renders closed server-side because it has no `open` attribute.
const openRisk = () => riskDialog.value?.showModal();
const closeRisk = () => riskDialog.value?.close();
</script>

<template>
  <div class="rp">
    <!-- Hero -->
    <section class="rp-hero">
      <div class="rp-wrap rp-hero-inner">
        <div class="rp-hero-text">
          <p class="rp-eyebrow">{{ t.eyebrow }}</p>
          <h1 class="rp-title">{{ t.title }}</h1>
          <p class="rp-lead">{{ t.lead }}</p>
        </div>
        <div class="rp-jump">
          <button class="rp-btn rp-btn-warn" type="button" @click="openRisk">
            <svg
              class="rp-btn-icon"
              viewBox="0 0 16 16"
              aria-hidden="true"
              fill="currentColor"
            >
              <path
                d="M8 1.6 15 14H1L8 1.6Zm0 3.9a.75.75 0 0 0-.75.75v3a.75.75 0 0 0 1.5 0v-3A.75.75 0 0 0 8 5.5Zm0 5.2a.9.9 0 1 0 0 1.8.9.9 0 0 0 0-1.8Z"
              />
            </svg>
            {{ t.openRisk }}
          </button>
          <a class="rp-btn rp-btn-ghost" :href="specLink">{{ t.jumpSpec }} →</a>
        </div>
      </div>
    </section>

    <!-- Cards -->
    <section class="rp-band">
      <div class="rp-wrap">
        <div class="rp-section-head">
          <h2 class="rp-h2">{{ t.stationsTitle }}</h2>
          <span class="rp-count">{{ t.stationsCount(RELAYS.length) }}</span>
          <span class="rp-hint">{{ t.hoverHint }}</span>
        </div>
        <RelayCards />
      </div>
    </section>

    <!-- Risk notice, opened from the hero -->
    <dialog ref="riskDialog" class="rd" @click.self="closeRisk">
      <div class="rd-panel">
        <header class="rd-head">
          <h2 class="rd-title">{{ t.riskTitle }}</h2>
          <button
            class="rd-x"
            type="button"
            :aria-label="t.riskCloseLabel"
            @click="closeRisk"
          >
            <svg viewBox="0 0 16 16" aria-hidden="true">
              <path
                d="M4 4l8 8M12 4l-8 8"
                stroke="currentColor"
                stroke-width="1.6"
                stroke-linecap="round"
                fill="none"
              />
            </svg>
          </button>
        </header>
        <div class="rd-body">
          <p class="rd-lead">{{ t.riskLead }}</p>
          <ul class="rd-list">
            <li v-for="r in t.risks" :key="r.head">
              <span class="rd-item-head">{{ r.head }}</span>
              <span class="rd-item-body">{{ r.body }}</span>
            </li>
          </ul>
        </div>
        <footer class="rd-foot">
          <button class="rd-ok" type="button" @click="closeRisk">
            {{ t.riskClose }}
          </button>
        </footer>
      </div>
    </dialog>
  </div>
</template>

<style scoped>
.rp-wrap {
  max-width: 1152px;
  margin: 0 auto;
  padding: 0 24px;
}

.rp-h2 {
  margin: 0;
  font-size: 22px;
  line-height: 1.3;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--as-ink);
}

.rp-band {
  padding: 32px 0 64px;
  background: var(--as-bg);
  border-top: 1px solid var(--as-line);
}

/* ---------- hero ---------- */

.rp-hero {
  padding: 30px 0 28px;
  background:
    radial-gradient(760px 300px at 8% -20%, rgba(16, 185, 129, 0.2), transparent 62%),
    radial-gradient(620px 260px at 95% 4%, rgba(245, 158, 11, 0.14), transparent 60%),
    linear-gradient(160deg, #1c1917 0%, #0c0a09 100%);
}

/* Buttons sit beside the copy rather than under it, so the hero is only as tall
   as the text stack — the button row used to add ~56px of pure height. */
.rp-hero-inner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 32px;
}

.rp-hero-text {
  min-width: 0;
}

.rp-eyebrow {
  margin: 0 0 10px;
  font-size: 11.5px;
  font-weight: 600;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: #6ee7b7;
}

.rp-title {
  margin: 0;
  border: 0;
  padding: 0;
  font-size: 34px;
  line-height: 1.15;
  font-weight: 800;
  letter-spacing: -0.025em;
  background: linear-gradient(120deg, #ffffff 38%, #6ee7b7 76%, #fcd34d);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
  color: transparent;
}

.rp-lead {
  margin: 10px 0 0;
  max-width: 64ch;
  font-size: 14.5px;
  line-height: 1.7;
  color: #a8a29e;
}

.rp-jump {
  display: flex;
  flex-wrap: wrap;
  flex-shrink: 0;
  gap: 10px;
}

.rp-btn {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 38px;
  padding: 0 18px;
  border: 1px solid transparent;
  border-radius: 19px;
  font-size: 13.5px;
  font-weight: 600;
  text-decoration: none;
  transition: transform 0.15s ease, background-color 0.15s ease, border-color 0.15s ease;
}

.rp-btn:hover {
  transform: translateY(-1px);
}

.rp-btn-icon {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
}

.rp-btn-warn {
  border-color: rgba(252, 211, 77, 0.42);
  color: #fcd34d;
  background: rgba(245, 158, 11, 0.14);
  cursor: pointer;
}

.rp-btn-warn:hover {
  border-color: #fcd34d;
  background: rgba(245, 158, 11, 0.22);
}

.rp-btn-ghost {
  border-color: rgba(250, 250, 249, 0.26);
  color: #fafaf9;
}

.rp-btn-ghost:hover {
  border-color: #6ee7b7;
  background: rgba(16, 185, 129, 0.12);
}

/* ---------- section head ---------- */

.rp-section-head {
  display: flex;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 6px 12px;
}

.rp-count {
  padding: 2px 10px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 700;
  color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
}

.rp-hint {
  font-size: 12.5px;
  color: var(--as-muted);
}

/* Pointer-only affordance, so don't advertise it where hovering is impossible.
   The cards stay fully expandable by click either way. */
@media (hover: none) {
  .rp-hint {
    display: none;
  }
}

/* ---------- risk dialog ---------- */

/* UA styles put a 2px border, 1em padding and a fit-content box on <dialog>;
   all of it has to go before the panel inside can control the look. */
.rd {
  width: min(720px, calc(100vw - 32px));
  max-height: min(82vh, 680px);
  margin: auto;
  padding: 0;
  border: 0;
  border-radius: 16px;
  background: transparent;
  overflow: visible;
}

.rd::backdrop {
  background: rgba(12, 10, 9, 0.62);
  backdrop-filter: blur(2px);
}

.rd-panel {
  display: flex;
  flex-direction: column;
  max-height: min(82vh, 680px);
  border: 1px solid var(--as-line);
  border-radius: 16px;
  background: var(--as-panel);
  box-shadow: 0 24px 60px rgba(12, 10, 9, 0.28);
  overflow: hidden;
}

.rd-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 18px 20px;
  border-bottom: 1px solid var(--as-line);
  background: var(--vp-c-warning-soft);
}

.rd-title {
  margin: 0;
  border: 0;
  padding: 0;
  font-size: 17px;
  font-weight: 700;
  line-height: 1.3;
  color: var(--as-ink);
}

.rd-x {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 30px;
  height: 30px;
  border-radius: 8px;
  color: var(--as-muted);
  cursor: pointer;
  transition: color 0.15s, background-color 0.15s;
}

.rd-x:hover {
  color: var(--as-ink);
  background: var(--as-line);
}

.rd-x svg {
  width: 16px;
  height: 16px;
}

.rd-body {
  padding: 18px 20px 20px;
  overflow-y: auto;
}

.rd-lead {
  margin: 0;
  font-size: 14px;
  line-height: 1.75;
  color: var(--as-muted);
}

.rd-list {
  list-style: none;
  margin: 16px 0 0;
  padding: 0;
  display: grid;
  gap: 12px;
}

.rd-list li {
  margin: 0;
  padding-left: 13px;
  border-left: 2px solid var(--vp-c-warning-2);
}

.rd-item-head {
  display: block;
  font-size: 14px;
  font-weight: 700;
  line-height: 1.6;
  color: var(--as-ink);
}

.rd-item-body {
  display: block;
  margin-top: 3px;
  font-size: 13.5px;
  line-height: 1.7;
  color: var(--as-muted);
}

.rd-foot {
  padding: 14px 20px;
  border-top: 1px solid var(--as-line);
  text-align: right;
  background: var(--as-bg-alt);
}

.rd-ok {
  height: 36px;
  padding: 0 20px;
  border-radius: 18px;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--vp-c-white);
  background: var(--vp-c-brand-2);
  cursor: pointer;
  transition: background-color 0.15s;
}

.rd-ok:hover {
  background: var(--vp-c-brand-1);
}

@media (max-width: 860px) {
  /* Not enough room for copy and buttons on one line — go back to stacking. */
  .rp-hero-inner {
    flex-direction: column;
    align-items: flex-start;
    gap: 16px;
  }
}

@media (max-width: 720px) {
  .rp-hero {
    padding: 26px 0 24px;
  }

  .rp-title {
    font-size: 28px;
  }
}

@media (max-width: 460px) {
  .rp-wrap {
    padding: 0 18px;
  }

  .rp-title {
    font-size: 26px;
  }

  .rp-btn {
    flex: 1 1 auto;
    justify-content: center;
  }
}
</style>
