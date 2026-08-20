<script setup lang="ts">
/* 中转站卡片。默认只显示身份信息 + 关键数字 + 入口按钮，须知与使用提示做成一层
   覆盖层：悬停时卡片变成强调色底、白字，卡片高度一点都不变（覆盖层是
   absolute，完全不参与高度计算）。卡片高度由默认视图决定，须知比卡片长就在
   覆盖层内部滚动 —— 不为最长的那条须知预留空白。
   覆盖层只盖住主体，不盖页脚 —— 页脚里的注册按钮如果被盖住，鼠标一移上来就
   被覆盖层挡住，那个按钮就永远点不到了。页脚的邀请参数披露也因此始终可见。
   卡片配色不写在 relays.ts 里 —— 数据文件只管事实，配色按顺序从下面这份调色板取，
   新增站点会自动拿到下一个色。 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
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
// Eight entries so a sixth or seventh provider still gets a distinct colour
// rather than repeating the first. Measured worst case across all eight:
// deep-on-white 5.47, tint-on-dark-panel 5.86, white-on-deep 5.47 — all AA.
const PALETTE = [
  { tint: "52, 211, 153", deep: "4, 120, 87" }, // emerald
  { tint: "167, 139, 250", deep: "109, 40, 217" }, // violet
  { tint: "251, 191, 36", deep: "146, 64, 14" }, // amber
  { tint: "56, 189, 248", deep: "3, 105, 161" }, // sky
  { tint: "251, 113, 133", deep: "159, 18, 57" }, // rose
  { tint: "45, 212, 191", deep: "15, 118, 110" }, // teal
  { tint: "129, 140, 248", deep: "67, 56, 202" }, // indigo
  { tint: "232, 121, 249", deep: "134, 25, 143" }, // fuchsia
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

// --- scroll-edge fades -----------------------------------------------------
// The overlay clips its content, so a scrolled card would otherwise appear to
// start mid-sentence. These set --rl-fade-t / --rl-fade-b to 1 only when content
// really is hidden past that edge; the mask in the CSS does the rest. Done from
// JS because CSS cannot query scroll position.
const scrollers = ref<HTMLElement[]>([]);
const registerScroller = (el: unknown) => {
  if (el instanceof HTMLElement && !scrollers.value.includes(el)) {
    scrollers.value.push(el);
  }
};

function syncFades(el: HTMLElement) {
  // 1px of slack: fractional layout means scrollTop rarely hits the exact
  // maximum, and a permanently-on bottom fade would look like a bug.
  const atTop = el.scrollTop <= 1;
  const atBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 1;
  el.style.setProperty("--rl-fade-t", atTop ? "0" : "1");
  el.style.setProperty("--rl-fade-b", atBottom ? "0" : "1");
}

const syncAll = () => scrollers.value.forEach(syncFades);
const onScroll = (e: Event) => syncFades(e.currentTarget as HTMLElement);

let ro: ResizeObserver | undefined;

onMounted(() => {
  scrollers.value.forEach((el) => el.addEventListener("scroll", onScroll, { passive: true }));
  // Reflow changes what overflows: viewport resizes, and web fonts landing after
  // first paint. Observe the scroller *and* its content wrapper — the box and
  // what's inside it can each change without the other.
  if (typeof ResizeObserver !== "undefined") {
    ro = new ResizeObserver(syncAll);
    scrollers.value.forEach((el) => {
      ro!.observe(el);
      if (el.firstElementChild) ro!.observe(el.firstElementChild);
    });
  }
  syncAll();
});

onBeforeUnmount(() => {
  scrollers.value.forEach((el) => el.removeEventListener("scroll", onScroll));
  ro?.disconnect();
});

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
      <!-- Default view. Sets the card height on its own; the overlay is absolute
           and so never changes it. -->
      <div class="rl-body">
        <div class="rl-default">
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
            <div class="rl-fact" v-if="copyFor(r).invite">
              <span class="rl-fact-k">{{ t.inviteLabel }}</span>
              <span class="rl-fact-v rl-fact-v-sm">{{ copyFor(r).invite }}</span>
            </div>
          </div>
        </div>

        <!-- Overlay. Always in the DOM (so its text is indexable and reachable by
             screen readers); revealed by opacity, which costs no layout. Content
             longer than the card scrolls inside .rl-over-scroll. -->
        <div :id="`rl-fold-${r.id}`" class="rl-over">
          <div class="rl-over-scroll" :ref="registerScroller">
            <!-- Single wrapper so a ResizeObserver on it sees the content grow;
                 observing the scroller alone would only catch its box changing. -->
            <div class="rl-over-inner">
              <!-- The overlay hides the identity block, so it repeats the name —
                   otherwise you lose track of which provider you're reading. The
                   toggle below already reads "notes and tips", so titling this
                   block that too would just duplicate it. -->
              <p class="rl-over-title">{{ copyFor(r).name }}</p>

              <template v-if="copyFor(r).notes?.length">
                <p class="rl-block-head">{{ t.notesLabel }}</p>
                <ul class="rl-list">
                  <li v-for="n in copyFor(r).notes" :key="n">{{ n }}</li>
                </ul>
              </template>

              <template v-if="copyFor(r).tips?.length">
                <p
                  class="rl-block-head"
                  :class="{ 'rl-block-head-tips': copyFor(r).notes?.length }"
                >
                  {{ t.tipsLabel }}
                </p>
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
        </div>
      </div>

      <!-- The label stays constant: on a pointer device hover alone can raise the
           overlay without touching `open`, so a "collapse" label would
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
  /* No `align-items: start` — the default `stretch` makes every card in a row as
     tall as the tallest one, so a provider with fewer notes doesn't leave a short
     card next to a long one. .rl-foot's `margin-top: auto` then pins the signup
     button to the bottom of whichever card got stretched. */
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

  position: relative;
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

/* ---------- notes overlay ---------- */

.rl-toggle {
  position: relative;
  z-index: 1;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  align-self: flex-start;
  font-size: 12px;
  font-weight: 600;
  color: rgb(var(--rl-fg));
  cursor: pointer;
  transition: color 0.22s ease;
}

.rl-chev {
  width: 12px;
  height: 12px;
  transition: transform 0.22s ease;
}

/* The card is sized by .rl-default alone — the overlay is absolutely positioned so
   it contributes no height at all. That keeps the card as short as its default
   view (no dead space reserved for the longest notes) while still never changing
   height on hover, since an absolute box can't push anything. Notes longer than
   the card scroll inside the overlay. */
.rl-body {
  position: relative;
  min-height: 0;
}

.rl-over {
  position: absolute;
  inset: 0;
  display: flex;
  /* Kept in the DOM and merely transparent, so the text is indexable and
     reachable by assistive tech even while invisible. `visibility` is what
     actually takes it out of the tab order and off the hit-testing surface —
     opacity alone would leave an invisible layer swallowing clicks. */
  z-index: 1;
  opacity: 0;
  visibility: hidden;
  transition: opacity 0.22s ease, visibility 0.22s;
}

/* The whole card floods with the accent colour, edge to edge — the fill is a
   ::after on the card itself rather than a box inside it, so it covers the
   padding too. Everything that must stay readable (the overlay text, the
   toggle, the footer) is lifted above it with z-index. */
.rl-card::after {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: 13px;
  opacity: 0;
  visibility: hidden;
  transition: opacity 0.22s ease, visibility 0.22s;
  /* Darkening gradient only, never lightening: white text sits on this, and the
     lightest end is the contrast floor (5.48:1 on the weakest palette entry). */
  background:
    linear-gradient(180deg, rgba(0, 0, 0, 0) 40%, rgba(0, 0, 0, 0.22)),
    rgb(var(--rl-d));
}

.rl-over-scroll {
  position: relative;
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  /* Room for the scrollbar so the text doesn't sit under it, and a thin styled
     bar — the default one is opaque and heavy against the accent fill. */
  padding-right: 8px;
  scrollbar-width: thin;
  scrollbar-color: rgba(255, 255, 255, 0.45) transparent;
  overscroll-behavior: contain;
  /* Fade the cut edges so a scrolled card doesn't look like it simply starts
     mid-sentence. A mask is used rather than a gradient overlay because it dims
     whatever is behind it — one rule works for all four accent fills.
     The two 12px stops are driven by JS: --rl-fade-t/-b go to 1 only when there
     really is content past that edge, so a card that fits shows no fade at all. */
  mask-image: linear-gradient(
    to bottom,
    rgba(0, 0, 0, calc(1 - var(--rl-fade-t, 0))) 0,
    #000 12px,
    #000 calc(100% - 12px),
    rgba(0, 0, 0, calc(1 - var(--rl-fade-b, 0))) 100%
  );
}

/* WebKit needs its own pseudo-elements; scrollbar-color above only covers Firefox. */
.rl-over-scroll::-webkit-scrollbar {
  width: 6px;
}

.rl-over-scroll::-webkit-scrollbar-track {
  background: transparent;
}

.rl-over-scroll::-webkit-scrollbar-thumb {
  border-radius: 3px;
  background: rgba(255, 255, 255, 0.42);
}

.rl-over-scroll::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.62);
}

.rl-over-title {
  margin: 0 0 10px;
  font-size: 14.5px;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: #ffffff;
}

.rl-card.is-open::after,
.rl-card.is-open .rl-over {
  opacity: 1;
  visibility: visible;
}

.rl-card.is-open .rl-chev {
  transform: rotate(180deg);
}

/* Hover-to-reveal is a pointer-only affordance. On touch the button is the only
   way in, so the hover rule must not apply there — a sticky :hover would leave
   the overlay stuck up. */
@media (hover: hover) and (pointer: fine) {
  .rl-card:hover::after,
  .rl-card:focus-within::after,
  .rl-card:hover .rl-over,
  .rl-card:focus-within .rl-over {
    opacity: 1;
    visibility: visible;
  }

  .rl-card:hover .rl-chev,
  .rl-card:focus-within .rl-chev {
    transform: rotate(180deg);
  }
}

/* Inside the overlay everything is white on the accent fill. */
.rl-over .rl-block-head {
  color: rgba(255, 255, 255, 0.82);
}

.rl-over .rl-list,
.rl-over .rl-tested {
  /* 0.92 over the weakest fill is 4.90:1 — clears AA for body text. */
  color: rgba(255, 255, 255, 0.92);
}

.rl-over .rl-list li::marker {
  color: rgba(255, 255, 255, 0.55);
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
  position: relative;
  z-index: 1;
  margin-top: auto;
  padding-top: 14px;
  border-top: 1px solid var(--as-line);
  transition: border-color 0.22s ease;
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
  transition: filter 0.2s, background-color 0.22s ease, color 0.22s ease;
}

.rl-btn:hover {
  filter: brightness(1.12);
}

.rl-meta {
  margin: 9px 0 0;
  font-size: 12px;
  color: var(--as-muted);
  transition: color 0.22s ease;
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

/* ---------- flooded state ----------
   Once the card floods, everything sitting on the fill has to switch to white.
   The CTA inverts to a white pill: its normal background is the very colour now
   behind it, so leaving it would make the button disappear. White-on-deep for
   the pill text is the same >=5.48:1 pairing, just reversed. */
.rl-card.is-open .rl-toggle,
.rl-card.is-open .rl-meta {
  color: rgba(255, 255, 255, 0.88);
}

.rl-card.is-open .rl-foot {
  border-top-color: rgba(255, 255, 255, 0.22);
}

.rl-card.is-open .rl-btn {
  color: rgb(var(--rl-d));
  background: #ffffff;
}

@media (hover: hover) and (pointer: fine) {
  .rl-card:hover .rl-toggle,
  .rl-card:focus-within .rl-toggle,
  .rl-card:hover .rl-meta,
  .rl-card:focus-within .rl-meta {
    color: rgba(255, 255, 255, 0.88);
  }

  .rl-card:hover .rl-foot,
  .rl-card:focus-within .rl-foot {
    border-top-color: rgba(255, 255, 255, 0.22);
  }

  .rl-card:hover .rl-btn,
  .rl-card:focus-within .rl-btn {
    color: rgb(var(--rl-d));
    background: #ffffff;
  }
}

@media (max-width: 640px) {
  .rl-grid {
    grid-template-columns: 1fr;
  }
}

@media (prefers-reduced-motion: reduce) {
  .rl-card,
  .rl-chev,
  .rl-over {
    transition: none;
  }

  .rl-card:hover,
  .rl-card:focus-within {
    transform: none;
  }
}
</style>
