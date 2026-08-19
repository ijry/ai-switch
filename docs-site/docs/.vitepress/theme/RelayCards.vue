<script setup lang="ts">
/* 中转站卡片。默认只显示身份信息 + 关键数字 + 入口按钮，须知与使用提示折叠起来，
   指针设备上悬停展开，触屏和键盘用卡片里的按钮展开。
   卡片配色不写在 relays.ts 里 —— 数据文件只管事实，配色按顺序从下面这份调色板取，
   新增站点会自动拿到下一个色。 */
import { computed, ref } from "vue";
import { useData } from "vitepress";
import { RELAYS, type RelayCopy } from "./relays";

const { localeIndex } = useData();
const isEn = computed(() => localeIndex.value === "en");

const COPY = {
  root: {
    signupLabel: "注册即得",
    inviteLabel: "邀请奖励",
    testedLabel: "实测",
    notesLabel: "须知",
    tipsLabel: "使用提示",
    modelsLabel: "1x 覆盖模型",
    visit: "通过邀请链接注册",
    affNote: "含邀请参数",
    verifiedPrefix: "实测于",
    staleWarning: "距上次实测已超过三个月，信息可能已变化，请以站点为准。",
    more: "须知与提示",
  },
  en: {
    signupLabel: "On signup",
    inviteLabel: "Referral bonus",
    testedLabel: "Tested",
    notesLabel: "Notes",
    tipsLabel: "Usage tips",
    modelsLabel: "Models at 1x",
    visit: "Sign up via referral link",
    affNote: "contains a referral parameter",
    verifiedPrefix: "Verified on",
    staleWarning:
      "Last verified more than three months ago — details may have changed. Trust the provider's own site over this page.",
    more: "Notes and tips",
  },
} as const;

const t = computed(() => (isEn.value ? COPY.en : COPY.root));
const copyFor = (r: (typeof RELAYS)[number]): RelayCopy => (isEn.value ? r.en : r.root);

// R,G,B triplets rather than hex, so every fill below can be an rgba() of the
// same hue at a different alpha over var(--as-panel) — one entry covers both
// colour modes. Two triplets per entry because one cannot do both jobs:
//   tint — the 400-level colour. Used for fills, borders and accent text in
//          dark mode, where it clears 6:1 against the dark panel.
//   deep — the 700/800-level colour. Accent text in light mode (>=5.4:1 on
//          white, whereas the tint would be ~2.5:1) and the solid button
//          background in both modes (>=5.4:1 against white button text).
const PALETTE = [
  { tint: "52, 211, 153", deep: "4, 120, 87" }, // emerald
  { tint: "167, 139, 250", deep: "109, 40, 217" }, // violet
  { tint: "251, 191, 36", deep: "146, 64, 14" }, // amber
  { tint: "56, 189, 248", deep: "3, 105, 161" }, // sky
];
const accent = (i: number) => PALETTE[i % PALETTE.length];

const open = ref<Set<string>>(new Set());
const isOpen = (id: string) => open.value.has(id);

function toggle(id: string) {
  // Reassign so the template re-renders — Set mutation alone is not tracked.
  const next = new Set(open.value);
  next.has(id) ? next.delete(id) : next.add(id);
  open.value = next;
}

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
    <li
      v-for="(r, i) in RELAYS"
      :key="r.id"
      class="rl-card"
      :class="{ 'is-open': isOpen(r.id) }"
      :style="{ '--rl-a': accent(i).tint, '--rl-d': accent(i).deep }"
    >
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

      <div class="rl-facts">
        <div class="rl-fact">
          <span class="rl-fact-k">{{ t.signupLabel }}</span>
          <span class="rl-fact-v">{{ copyFor(r).signup }}</span>
        </div>
        <div class="rl-fact">
          <span class="rl-fact-k">{{ t.inviteLabel }}</span>
          <span class="rl-fact-v rl-fact-v-sm">{{ copyFor(r).invite }}</span>
        </div>
      </div>

      <!-- The label stays constant: on a pointer device hover alone can expand
           the fold without touching `open`, so a "collapse" label would
           contradict the screen. The chevron carries the state instead. -->
      <button
        class="rl-toggle"
        type="button"
        :aria-expanded="isOpen(r.id)"
        :aria-controls="`rl-fold-${r.id}`"
        @click="toggle(r.id)"
      >
        <span>{{ t.more }}</span>
        <svg class="rl-chev" viewBox="0 0 12 12" aria-hidden="true">
          <path
            d="M2.5 4.5 6 8l3.5-3.5"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>

      <!-- Collapsed by default; grid-template-rows animates 0fr -> 1fr. -->
      <div :id="`rl-fold-${r.id}`" class="rl-fold">
        <div class="rl-fold-inner">
          <p class="rl-block-head">{{ t.notesLabel }}</p>
          <ul class="rl-list">
            <li v-for="n in copyFor(r).notes" :key="n">{{ n }}</li>
          </ul>

          <template v-if="copyFor(r).tips?.length">
            <p class="rl-block-head rl-block-head-tips">{{ t.tipsLabel }}</p>
            <ul class="rl-list">
              <li v-for="tip in copyFor(r).tips" :key="tip">{{ tip }}</li>
            </ul>
          </template>

          <p class="rl-tested">
            <span class="rl-block-head">{{ t.testedLabel }}</span>
            {{ copyFor(r).tested }}
          </p>
        </div>
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
  margin: 20px 0 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  align-items: start;
  gap: 18px;
}

.rl-card {
  /* Which of the two triplets plays which role flips with the colour mode.
     `.dark` sits on <html>, and scoped CSS only tags the last selector, so the
     ancestor override below resolves correctly. */
  --rl-fg: var(--rl-d);
  --rl-edge: var(--rl-d);
  --rl-btn-bg: var(--rl-d);
  --rl-btn-fg: #ffffff;

  display: flex;
  flex-direction: column;
  gap: 12px;
  margin: 0;
  padding: 18px 18px 16px;
  border: 1px solid rgba(var(--rl-edge), 0.26);
  border-radius: 14px;
  background:
    radial-gradient(120% 90% at 100% 0%, rgba(var(--rl-a), 0.14), transparent 58%),
    linear-gradient(160deg, rgba(var(--rl-a), 0.09), rgba(var(--rl-a), 0) 52%),
    var(--as-panel);
  transition: border-color 0.2s ease, box-shadow 0.2s ease, transform 0.2s ease;
}

.dark .rl-card {
  --rl-fg: var(--rl-a);
  --rl-edge: var(--rl-a);
  --rl-btn-bg: var(--rl-a);
  --rl-btn-fg: #0c0a09;
}

.rl-card:hover,
.rl-card:focus-within {
  border-color: rgba(var(--rl-edge), 0.55);
  box-shadow: 0 10px 28px rgba(var(--rl-a), 0.16);
  transform: translateY(-2px);
}

/* ---------- identity ---------- */

.rl-head {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.rl-title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.rl-name {
  margin: 0;
  border: 0;
  padding: 0;
  font-size: 16.5px;
  font-weight: 700;
  line-height: 1.3;
  letter-spacing: -0.01em;
  color: var(--as-ink);
}

.rl-rate {
  flex-shrink: 0;
  padding: 2px 9px;
  border-radius: 999px;
  font-size: 11.5px;
  font-weight: 700;
  color: rgb(var(--rl-fg));
  background: rgba(var(--rl-a), 0.16);
}

.rl-host {
  margin: 0;
  font-size: 12.5px;
  font-family: var(--vp-font-family-mono);
  color: var(--as-muted);
}

.rl-models {
  list-style: none;
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  margin: 0;
  padding: 0;
}

.rl-model {
  margin: 0;
  padding: 2px 8px;
  border: 1px solid rgba(var(--rl-edge), 0.3);
  border-radius: 6px;
  font-size: 11.5px;
  font-family: var(--vp-font-family-mono);
  color: var(--as-muted);
  background: var(--as-panel);
}

/* ---------- key numbers ---------- */

.rl-facts {
  display: grid;
  gap: 8px;
  padding: 12px 0 0;
  border-top: 1px solid var(--as-line);
}

.rl-fact {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.rl-fact-k {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--as-muted);
}

.rl-fact-v {
  font-size: 15px;
  font-weight: 700;
  line-height: 1.45;
  color: var(--as-ink);
}

.rl-fact-v-sm {
  font-size: 13px;
  font-weight: 400;
  line-height: 1.6;
  color: var(--as-muted);
}

/* ---------- disclosure ---------- */

.rl-toggle {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  align-self: flex-start;
  font-size: 12px;
  font-weight: 600;
  color: rgb(var(--rl-fg));
  cursor: pointer;
}

.rl-chev {
  width: 12px;
  height: 12px;
  transition: transform 0.22s ease;
}

.rl-fold {
  display: grid;
  grid-template-rows: 0fr;
  transition: grid-template-rows 0.26s ease;
}

.rl-fold-inner {
  overflow: hidden;
  min-height: 0;
}

.rl-card.is-open .rl-fold {
  grid-template-rows: 1fr;
}

.rl-card.is-open .rl-chev {
  transform: rotate(180deg);
}

/* Hover-to-reveal is a pointer-only affordance. On touch the button is the
   only way in, so the hover rule must not apply there — a sticky :hover would
   leave the card stuck open with the button reading "collapse". */
@media (hover: hover) and (pointer: fine) {
  .rl-card:hover .rl-fold,
  .rl-card:focus-within .rl-fold {
    grid-template-rows: 1fr;
  }

  .rl-card:hover .rl-chev,
  .rl-card:focus-within .rl-chev {
    transform: rotate(180deg);
  }
}

.rl-block-head {
  margin: 0 0 5px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--as-muted);
}

.rl-block-head-tips {
  margin-top: 12px;
}

/* base.css sets `ol, ul { list-style: none }` globally, so `disc` has to be
   restored here or the ::marker rule below has nothing to colour. */
.rl-list {
  margin: 0;
  padding-left: 16px;
  list-style: disc;
  font-size: 13px;
  line-height: 1.7;
  color: var(--as-muted);
}

.rl-list li + li {
  margin-top: 2px;
}

.rl-list li::marker {
  color: rgba(var(--rl-edge), 0.65);
}

.rl-tested {
  margin: 12px 0 0;
  font-size: 13px;
  line-height: 1.7;
  color: var(--as-muted);
}

.rl-tested .rl-block-head {
  display: block;
  margin-bottom: 3px;
}

/* ---------- footer ---------- */

.rl-foot {
  margin-top: auto;
  padding-top: 14px;
  border-top: 1px solid var(--as-line);
}

.rl-btn {
  display: inline-flex;
  align-items: center;
  height: 36px;
  padding: 0 16px;
  border-radius: 18px;
  font-size: 13.5px;
  font-weight: 600;
  text-decoration: none;
  color: var(--rl-btn-fg);
  background: rgb(var(--rl-btn-bg));
  transition: filter 0.2s;
}

.rl-btn:hover {
  filter: brightness(1.12);
}

.rl-meta {
  margin: 9px 0 0;
  font-size: 12px;
  color: var(--as-muted);
}

.rl-stale {
  margin: 8px 0 0;
  padding: 8px 10px;
  border-radius: 8px;
  font-size: 12.5px;
  line-height: 1.6;
  color: var(--vp-c-warning-1);
  background: var(--vp-c-warning-soft);
}

@media (max-width: 640px) {
  .rl-grid {
    grid-template-columns: 1fr;
  }
}

@media (prefers-reduced-motion: reduce) {
  .rl-card,
  .rl-chev,
  .rl-fold {
    transition: none;
  }

  .rl-card:hover,
  .rl-card:focus-within {
    transform: none;
  }
}
</style>
