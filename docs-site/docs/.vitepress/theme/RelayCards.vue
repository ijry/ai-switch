<script setup lang="ts">
import { computed } from "vue";
import { useData } from "vitepress";
import { RELAYS, type RelayCopy } from "./relays";

const { localeIndex } = useData();
const isEn = computed(() => localeIndex.value === "en");

const COPY = {
  root: {
    rateLabel: "倍率",
    signupLabel: "注册额度",
    inviteLabel: "邀请",
    testedLabel: "实测",
    notesLabel: "须知",
    tipsLabel: "使用提示",
    modelsLabel: "1x 覆盖模型",
    visit: "通过邀请链接注册",
    affNote: "含邀请参数",
    verifiedPrefix: "实测于",
    staleWarning: "距上次实测已超过三个月，信息可能已变化，请以站点为准。",
  },
  en: {
    rateLabel: "Rate",
    signupLabel: "Signup credit",
    inviteLabel: "Referrals",
    testedLabel: "Tested",
    notesLabel: "Notes",
    tipsLabel: "Usage tips",
    modelsLabel: "Models at 1x",
    visit: "Sign up via referral link",
    affNote: "contains a referral parameter",
    verifiedPrefix: "Verified on",
    staleWarning:
      "Last verified more than three months ago — details may have changed. Trust the provider's own site over this page.",
  },
} as const;

const t = computed(() => (isEn.value ? COPY.en : COPY.root));
const copyFor = (r: (typeof RELAYS)[number]): RelayCopy => (isEn.value ? r.en : r.root);

// Evaluated on the server at build time and again in the browser at page-load,
// so the two can disagree once a card crosses the three-month line. The warning
// is therefore rendered inside <ClientOnly> — it must reflect *now*, not the
// build date, and SSR has no matching markup to mismatch against.
const isStale = (iso: string) => {
  const then = new Date(`${iso}T00:00:00Z`).getTime();
  if (Number.isNaN(then)) return false;
  const months = (Date.now() - then) / (1000 * 60 * 60 * 24 * 30.4375);
  return months > 3;
};

const fmtDate = (iso: string) =>
  isEn.value
    ? new Date(`${iso}T00:00:00Z`).toLocaleDateString("en-US", {
        year: "numeric",
        month: "short",
        day: "numeric",
        timeZone: "UTC",
      })
    : iso.replace(/^(\d{4})-(\d{2})-(\d{2})$/, "$1 年 $2 月 $3 日");
</script>

<template>
  <ul class="rl-grid">
    <li v-for="r in RELAYS" :key="r.id" class="rl-card">
      <header class="rl-head">
        <div class="rl-title-row">
          <h3 class="rl-name">{{ copyFor(r).name }}</h3>
          <span class="rl-rate">{{ copyFor(r).rate }}</span>
        </div>
        <p class="rl-host">{{ r.host }}</p>
      </header>

      <ul class="rl-models" :aria-label="t.modelsLabel">
        <li v-for="m in r.models" :key="m" class="rl-model">{{ m }}</li>
      </ul>

      <dl class="rl-facts">
        <dt>{{ t.signupLabel }}</dt>
        <dd>{{ copyFor(r).signup }}</dd>
        <dt>{{ t.inviteLabel }}</dt>
        <dd>{{ copyFor(r).invite }}</dd>
        <dt>{{ t.testedLabel }}</dt>
        <dd>{{ copyFor(r).tested }}</dd>
      </dl>

      <div class="rl-block">
        <p class="rl-block-head">{{ t.notesLabel }}</p>
        <ul class="rl-list">
          <li v-for="n in copyFor(r).notes" :key="n">{{ n }}</li>
        </ul>
      </div>

      <div v-if="copyFor(r).tips?.length" class="rl-block rl-block-tips">
        <p class="rl-block-head">{{ t.tipsLabel }}</p>
        <ul class="rl-list">
          <li v-for="tip in copyFor(r).tips" :key="tip">{{ tip }}</li>
        </ul>
      </div>

      <footer class="rl-foot">
        <a
          class="rl-btn"
          :href="r.aff"
          target="_blank"
          rel="noopener nofollow sponsored"
        >
          {{ t.visit }} →
        </a>
        <p class="rl-meta">
          {{ t.verifiedPrefix }} {{ fmtDate(r.verifiedAt) }} · {{ t.affNote }}
        </p>
        <ClientOnly>
          <p v-if="isStale(r.verifiedAt)" class="rl-stale">{{ t.staleWarning }}</p>
        </ClientOnly>
      </footer>
    </li>
  </ul>
</template>

<style scoped>
.rl-grid {
  list-style: none;
  padding: 0;
  margin: 24px 0 0;
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 20px;
}

.rl-card {
  display: flex;
  flex-direction: column;
  gap: 16px;
  margin: 0;
  padding: 22px 22px 20px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 14px;
  background: var(--vp-c-bg-soft);
}

.rl-head {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.rl-title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.rl-name {
  margin: 0;
  border: 0;
  padding: 0;
  font-size: 18px;
  font-weight: 700;
  line-height: 1.3;
  letter-spacing: -0.01em;
}

.rl-rate {
  flex-shrink: 0;
  padding: 3px 10px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 700;
  color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
}

.rl-host {
  margin: 0;
  font-size: 13px;
  font-family: var(--vp-font-family-mono);
  color: var(--vp-c-text-3);
}

.rl-models {
  list-style: none;
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: 0;
  padding: 0;
}

.rl-model {
  margin: 0;
  padding: 3px 9px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  font-size: 12px;
  font-family: var(--vp-font-family-mono);
  color: var(--vp-c-text-2);
  background: var(--vp-c-bg);
}

.rl-facts {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px 14px;
  margin: 0;
  font-size: 14px;
}

.rl-facts dt {
  font-weight: 600;
  color: var(--vp-c-text-2);
  white-space: nowrap;
}

.rl-facts dd {
  margin: 0;
  color: var(--vp-c-text-1);
  line-height: 1.65;
}

.rl-block {
  padding-top: 14px;
  border-top: 1px solid var(--vp-c-divider);
}

.rl-block-head {
  margin: 0 0 6px;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--vp-c-text-3);
}

.rl-list {
  margin: 0;
  padding-left: 18px;
  font-size: 14px;
  line-height: 1.7;
  color: var(--vp-c-text-2);
}

.rl-list li + li {
  margin-top: 3px;
}

.rl-block-tips .rl-list {
  color: var(--vp-c-text-1);
}

.rl-foot {
  margin-top: auto;
  padding-top: 16px;
  border-top: 1px solid var(--vp-c-divider);
}

.rl-btn {
  display: inline-flex;
  align-items: center;
  height: 38px;
  padding: 0 18px;
  border-radius: 19px;
  font-size: 14px;
  font-weight: 600;
  text-decoration: none;
  color: var(--vp-c-white);
  background: var(--vp-c-brand-2);
  transition: background-color 0.2s;
}

.rl-btn:hover {
  background: var(--vp-c-brand-1);
}

.rl-meta {
  margin: 10px 0 0;
  font-size: 12.5px;
  color: var(--vp-c-text-3);
}

.rl-stale {
  margin: 8px 0 0;
  padding: 8px 10px;
  border-radius: 8px;
  font-size: 12.5px;
  line-height: 1.6;
  color: var(--vp-c-warning-1, #b8860b);
  background: var(--vp-c-warning-soft, rgba(234, 179, 8, 0.14));
}

@media (max-width: 640px) {
  .rl-grid {
    grid-template-columns: 1fr;
  }
}
</style>
