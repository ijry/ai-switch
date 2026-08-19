/* ---------------------------------------------------------------------------
   中转站清单 / Relay provider list

   维护提示 —— 这个文件里的内容全部是**第三方服务的当前状态**，不是 AI Switch
   自身的行为，代码里没有任何东西可以用来校验它。它会过期，而且会过期得很快：
   中转站的倍率、赠送额度、邀请规则、可用模型随时会改，站点也可能直接关停。

   改动规则：
   1. 任何一条数字（倍率 / 额度 / 邀请）变了，必须同时更新 verifiedAt。
   2. verifiedAt 距今超过三个月，页面会提示信息可能过期 —— 要么重新实测并更新
      日期，要么把该条目下架。不要只改日期不实测。
   3. root 与 en 两份文案必须同时改，两个 locale 共用这一份数据。
   4. aff 链接含邀请参数。每张卡片的页脚都写着「含邀请参数」，链接本身带
      rel="noopener nofollow sponsored"（见 RelayCards.vue）。删掉披露就变成
      隐瞒返利，别删。
   5. 卡片配色不在这里 —— RelayCards.vue 按数组顺序从固定调色板取色，新增站点
      自动拿到下一个色，不需要在数据里指定。
--------------------------------------------------------------------------- */

export type RelayCopy = {
  /** 站点显示名（含「公益站」这类限定语） */
  name: string;
  /** 实测结论：注册用什么账号、是否通过 */
  tested: string;
  /** 倍率说明 */
  rate: string;
  /** 注册即得额度 */
  signup: string;
  /** 邀请规则 */
  invite: string;
  /** 须知，逐条列出 */
  notes: string[];
  /** 使用提示（可选），比 notes 更偏操作细节 */
  tips?: string[];
};

export type Relay = {
  id: string;
  /** 邀请链接，含 aff 参数 */
  aff: string;
  /** 去掉协议头的站点域名，卡片上作为副标题显示 */
  host: string;
  /** 1x 倍率覆盖的模型，作为徽章展示 */
  models: string[];
  /** 最近一次实测日期，ISO yyyy-mm-dd */
  verifiedAt: string;
  root: RelayCopy;
  en: RelayCopy;
};

export const RELAYS: Relay[] = [
  {
    id: "agentrouter",
    aff: "https://agentrouter.org/register?aff=t2rx",
    host: "agentrouter.org",
    models: ["gpt-5.6", "opus 5"],
    verifiedAt: "2026-08-19",
    root: {
      name: "AgentRouter",
      tested: "用 2026 年之前注册的 GitHub 老账号注册通过。",
      rate: "1x 倍率",
      signup: "注册即得 $100 额度",
      invite: "每邀请一位新用户：新用户获得 $50 额度，邀请人获得 $100 额度。",
      notes: ["不需要加群，注册完就能用", "使用 GitHub 账号注册"],
      tips: [
        "拉取模型列表可能失败。取不到就直接手填模型名：gpt-5.6-sol、claude-opus-5。",
        "每日签到需要重新登录一次，可领 25 额度。",
      ],
    },
    en: {
      name: "AgentRouter",
      tested: "Signed up successfully with a GitHub account created before 2026.",
      rate: "1x rate",
      signup: "$100 credit on signup",
      invite:
        "Per invited user: the new user gets $50 in credit and the referrer gets $100.",
      notes: [
        "No chat group to join — usable immediately after signup",
        "Sign up with a GitHub account",
      ],
      tips: [
        "Fetching the model list may fail. If it does, type the model names in by hand: gpt-5.6-sol, claude-opus-5.",
        "The daily check-in requires logging in again, and grants 25 credit.",
      ],
    },
  },
  {
    id: "gorouter",
    aff: "https://gorouter.app/sign-up?aff=VjCQ",
    host: "gorouter.app",
    models: ["opus 4.8", "opus 5"],
    verifiedAt: "2026-08-19",
    root: {
      name: "GoRouter 公益站",
      tested: "用 GitHub 账号注册通过。",
      rate: "1x 倍率",
      signup: "注册即得 $50 额度",
      invite: "每邀请一位新用户：新用户获得 $20 额度，邀请人获得 $40 额度。",
      notes: [
        "不需要加群，注册完就能用",
        "使用 GitHub 账号注册",
        "只支持 Claude Code",
      ],
    },
    en: {
      name: "GoRouter (public-benefit relay)",
      tested: "Signed up successfully with a GitHub account.",
      rate: "1x rate",
      signup: "$50 credit on signup",
      invite:
        "Per invited user: the new user gets $20 in credit and the referrer gets $40.",
      notes: [
        "No chat group to join — usable immediately after signup",
        "Sign up with a GitHub account",
        "Claude Code only",
      ],
    },
  },
];
