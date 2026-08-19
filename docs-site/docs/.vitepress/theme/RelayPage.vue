<script setup lang="ts">
/* 中转站页面的唯一入口。这一页整体是自定义组件，不走 Markdown：
   docs/relay/index.md 与 docs/en/relay/index.md 都只有 frontmatter + <RelayPage />。
   两个 locale 共用这一份文案字典，改一处必须同时改另一处。 */
import { computed } from "vue";
import { useData } from "vitepress";
import { RELAYS } from "./relays";
import RelayCards from "./RelayCards.vue";
import RelaySpec from "./RelaySpec.vue";

const { localeIndex } = useData();
const isEn = computed(() => localeIndex.value === "en");

const COPY = {
  root: {
    eyebrow: "第三方服务 · 非本项目运营",
    title: "中转站",
    lead:
      "这一页收录经过实测可用的第三方 AI 中转站，以及供中转站接入的一键添加协议规范。",
    jumpStations: "看站点",
    jumpSpec: "对接规范",

    riskTitle: "先读这一段",
    riskLead: "收录只代表某个时间点实测能用，不构成任何形式的担保或推荐承诺。",
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
  },

  en: {
    eyebrow: "Third-party services · not operated by this project",
    title: "Relay Providers",
    lead:
      "Hand-tested third-party AI relay providers, plus the one-click-add protocol spec that relay operators can integrate against.",
    jumpStations: "Providers",
    jumpSpec: "Integration spec",

    riskTitle: "Read this first",
    riskLead:
      "Listing a provider only means it was tested working at a point in time; it is not a warranty or an endorsement.",
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
  },
} as const;

const t = computed(() => (isEn.value ? COPY.en : COPY.root));
</script>

<template>
  <div class="rp">
    <!-- Hero -->
    <section class="rp-hero">
      <div class="rp-wrap">
        <p class="rp-eyebrow">{{ t.eyebrow }}</p>
        <h1 class="rp-title">{{ t.title }}</h1>
        <p class="rp-lead">{{ t.lead }}</p>
        <div class="rp-jump">
          <a class="rp-btn rp-btn-primary" href="#stations">{{ t.jumpStations }}</a>
          <a class="rp-btn rp-btn-ghost" href="#spec">{{ t.jumpSpec }}</a>
        </div>
      </div>
    </section>

    <!-- Risk notice + cards -->
    <section id="stations" class="rp-band">
      <div class="rp-wrap">
        <div class="rp-risk">
          <p class="rp-risk-title">{{ t.riskTitle }}</p>
          <p class="rp-risk-lead">{{ t.riskLead }}</p>
          <ul class="rp-risk-list">
            <li v-for="r in t.risks" :key="r.head">
              <span class="rp-risk-head">{{ r.head }}</span>
              <span class="rp-risk-body">{{ r.body }}</span>
            </li>
          </ul>
        </div>

        <div class="rp-section-head">
          <h2 class="rp-h2">{{ t.stationsTitle }}</h2>
          <span class="rp-count">{{ t.stationsCount(RELAYS.length) }}</span>
        </div>
        <RelayCards />
      </div>
    </section>

    <!-- Integration spec -->
    <section id="spec" class="rp-band rp-band-alt">
      <div class="rp-wrap">
        <RelaySpec />
      </div>
    </section>
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
  font-size: 26px;
  line-height: 1.3;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--as-ink);
}

.rp-band {
  padding: 60px 0;
  background: var(--as-bg);
  border-top: 1px solid var(--as-line);
}

.rp-band-alt {
  padding-bottom: 72px;
  background: var(--as-bg-alt);
}

/* ---------- hero ---------- */

.rp-hero {
  padding: 64px 0 68px;
  background:
    radial-gradient(820px 380px at 10% -14%, rgba(16, 185, 129, 0.2), transparent 62%),
    radial-gradient(700px 340px at 94% 6%, rgba(245, 158, 11, 0.14), transparent 60%),
    linear-gradient(160deg, #1c1917 0%, #0c0a09 100%);
}

.rp-eyebrow {
  margin: 0 0 16px;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: #6ee7b7;
}

.rp-title {
  margin: 0;
  border: 0;
  padding: 0;
  font-size: 42px;
  line-height: 1.16;
  font-weight: 800;
  letter-spacing: -0.025em;
  background: linear-gradient(120deg, #ffffff 38%, #6ee7b7 76%, #fcd34d);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
  color: transparent;
}

.rp-lead {
  margin: 20px 0 0;
  max-width: 62ch;
  font-size: 16px;
  line-height: 1.75;
  color: #a8a29e;
}

.rp-jump {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 28px;
}

.rp-btn {
  display: inline-flex;
  align-items: center;
  height: 42px;
  padding: 0 22px;
  border: 1px solid transparent;
  border-radius: 21px;
  font-size: 14px;
  font-weight: 600;
  text-decoration: none;
  transition: transform 0.15s ease, background-color 0.15s ease, border-color 0.15s ease;
}

.rp-btn:hover {
  transform: translateY(-1px);
}

.rp-btn-primary {
  background: #10b981;
  color: #052e21;
}

.rp-btn-primary:hover {
  background: #34d399;
}

.rp-btn-ghost {
  border-color: rgba(250, 250, 249, 0.28);
  color: #fafaf9;
}

.rp-btn-ghost:hover {
  border-color: #6ee7b7;
  background: rgba(16, 185, 129, 0.12);
}

/* ---------- risk notice ---------- */

.rp-risk {
  padding: 22px 24px;
  border: 1px solid var(--vp-c-warning-2);
  border-left-width: 3px;
  border-radius: 12px;
  background: var(--vp-c-warning-soft);
}

.rp-risk-title {
  margin: 0;
  font-size: 15px;
  font-weight: 700;
  color: var(--as-ink);
}

.rp-risk-lead {
  margin: 6px 0 0;
  max-width: 84ch;
  font-size: 14px;
  line-height: 1.7;
  color: var(--as-muted);
}

.rp-risk-list {
  list-style: none;
  margin: 16px 0 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  gap: 14px 28px;
}

.rp-risk-list li {
  margin: 0;
  padding-left: 14px;
  border-left: 2px solid var(--vp-c-warning-2);
}

.rp-risk-head {
  display: block;
  font-size: 14px;
  font-weight: 700;
  line-height: 1.6;
  color: var(--as-ink);
}

.rp-risk-body {
  display: block;
  margin-top: 3px;
  font-size: 13.5px;
  line-height: 1.7;
  color: var(--as-muted);
}

/* ---------- section head ---------- */

.rp-section-head {
  display: flex;
  align-items: baseline;
  gap: 12px;
  margin-top: 44px;
}

.rp-count {
  padding: 2px 10px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 700;
  color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
}

@media (max-width: 720px) {
  .rp-hero {
    padding: 48px 0 52px;
  }

  .rp-band {
    padding: 48px 0;
  }

  .rp-title {
    font-size: 32px;
  }

  .rp-h2 {
    font-size: 22px;
  }
}

@media (max-width: 460px) {
  .rp-wrap {
    padding: 0 18px;
  }

  .rp-title {
    font-size: 28px;
  }

  .rp-btn {
    width: 100%;
    justify-content: center;
  }
}
</style>
