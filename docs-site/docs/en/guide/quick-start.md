---
title: Quick Start
description: "Get your first request through AI Switch in five steps: add a route account, set priority and concurrency and join the pool, start the local proxy, point your CLI at it, and confirm billing in the usage stats."
---

# Quick Start

This page takes you from nothing to a first request flowing through AI Switch. It takes about five minutes. You'll need one working upstream account — a base URL and an API key, whether from a relay provider or an official API.

The walkthrough uses **Codex**. Other platforms follow the same flow; only the default interface format differs.

::: info About the labels below
The accounts screen is not localized yet — its labels appear in Chinese regardless of the app language you pick. Below, each control is given by its literal on-screen label with an English gloss, so you can match what you actually see.
:::

## Step 1: Add a route account

Open AI Switch and click **Codex** under the **Agents** group in the sidebar. That's also the default screen on launch.

Click the **+** button in the top toolbar (tooltip 新增账号, "add account") to open the account dialog. Three tabs sit at the top; stay on **「API 账号」** ("API account"). 批量导入 ("bulk import") is for importing official sign-in state, and 导入其他客户端 ("import from another client") reads CC Switch's accounts off this machine directly.

Fill in these fields in order.

**账号名称** ("account name") — anything recognizable, such as `relay-primary`. This name shows up later in the usage stats request list, so a meaningful one pays off when reconciling spend.

**API Key** — your upstream key. This is a multi-line box: **one key per line**. Pasting several lines creates several accounts at once, grouped as one batch. For a single account, use one line.

Two helper buttons sit alongside it. If your key is base64-encoded, use 「Base64 解码」 ("decode base64"). If it's in a screenshot, use 「OCR识别」 ("OCR") to pick an image and extract it.

**Base URL** — the upstream endpoint, for example `https://your-relay.example.com/v1`. Use whatever address the upstream documentation gives you.

**接口格式** ("interface format") — this is the **upstream protocol**, i.e. which dialect your account speaks. Four options:

| Option | Dialect | Upstream API |
| --- | --- | --- |
| OpenAI Chat Completions | `openai` | Chat Completions |
| OpenAI Responses | `openai-responses` | Responses API |
| Claude Messages | `anthropic` | Messages API |
| Gemini | `gemini` | generateContent |

**Choose based on what the upstream actually supports, not on which CLI you want to use.** Mismatches are bridged by AI Switch, so a Chat Completions account is perfectly usable with Codex.

The Codex platform defaults to `openai`. Selecting `anthropic` reveals an extra **「Claude 鉴权字段」** ("Claude auth field") choice: `ANTHROPIC_AUTH_TOKEN` sends `Authorization: Bearer` (what most relays expect), while `ANTHROPIC_API_KEY` sends `x-api-key` (the official Anthropic style). When unsure, try the former first.

**模型映射** ("model mappings", optional) — use this when the model name the client requests differs from what the upstream serves. If your CLI asks for `gpt-5.5` but your relay only has `gpt-4o`, add a `gpt-5.5 → gpt-4o` entry. If the names already agree, skip it.

::: tip Model mappings also drive routing
Mappings aren't just renaming. The proxy checks whether any account in the pool serves the requested model and returns `route_pool.model_unmatched` outright if none does, rather than sending a doomed request. Getting the mappings right is what lets routing match at all.
:::

At the bottom is a **「创建后加入算力池」** ("join the pool after creation") checkbox, **checked by default**. Leave it checked.

Click **「保存账号」** ("save account"). On success you'll see a toast reading 已新增 N 个账号并加入算力池 ("added N accounts and joined the pool").

## Step 2: Set priority and concurrency, confirm pool membership

The account is already in the pool. Now tune its routing parameters.

Find the new account in the list and click **「编辑账号」** ("edit account") at the end of its row; a panel slides out on the right. Two fields matter most.

**路由优先级** ("route priority") — 1 to 5, **default 3**, lower numbers win. The proxy tiers strictly: every priority-1 account must be unavailable before a priority-2 account is used.

With a single account there's nothing to change. Once you have several, a reasonable split is:

- Primary account (cheap, stable, plenty of credit) at **1**
- Backup at **3**
- Last resort (expensive, or emergency-only) at **5**

When the primary gets rate-limited, the proxy drops to the backup with no action from you.

**最大并发数** ("max concurrency") — **default 5, minimum 1**. How many requests this account may run at once.

Lower it if the upstream is sensitive to concurrency, as official accounts often are; 1 is the most conservative setting. Note this is a **hard limit**: an account at its ceiling is skipped and the proxy tries the next account in the same tier. If every account in the pool is saturated, you get `route_pool.concurrency_exhausted`.

**失败处理策略** ("failure policy", same panel) — defaults to 2 extra retries, a 200 ms interval, 10 consecutive identical semantic errors before the account is marked unhealthy, and a 10-second failure cooldown (cooldown itself is off by default). The defaults suit most setups; leave them for now. These rules are shared between proxy requests and model tests.

Close the panel to save.

::: info If the account isn't in the pool
The bottom switcher has four views: **算力池** ("pool") / **未入池** ("not in pool") / **已归档** ("archived") / **统计** ("stats").

If your account landed under 未入池 because you unchecked the box at creation, switch to that view, select the account, and click **「加入算力池」** ("join pool") in the toolbar.

In the pool view you can drag rows to reorder them. That order is the rotation starting point **within a priority tier** and never overrides tiering.
:::

## Step 3: Start the local proxy

Back in the top toolbar, click the green **▶ button** (tooltip 启动本地路由代理, "start local route proxy").

Once it's running, the toolbar status strip changes from 代理未启动 ("proxy not started") to the proxy address:

```text
http://127.0.0.1:19527
```

The proxy binds to **`127.0.0.1`** on default port **19527**. It listens on loopback only, so other machines on your network cannot reach it.

Local entry paths by platform:

| Platform | Local entry | Protocol |
| --- | --- | --- |
| Codex | `/responses` | OpenAI Responses |
| Claude | `/v1/messages` | Anthropic Messages |
| Gemini CLI | Gemini native | generateContent |

If you need TLS, settings has a "Local capacity pool HTTPS" toggle that generates and imports a root certificate; the proxy address then becomes `https://`.

### Verify before going further

Before pointing a CLI at the proxy, use the built-in test to confirm the path works end to end.

Click the **✈ button** in the toolbar (tooltip 真实生成测试算力池路由, "real generation test through the pool") to open the dialog. On Codex you can pick the test endpoint (`/responses` or `/chat/completions`); leaving the model blank uses the platform default. Click **「开始测试」** ("start test").

This is a **real generation request**, not a reachability probe — the upstream genuinely produces content. The result panel gives you:

- Pass or fail
- **模型输出** ("model output") — the text the model actually returned
- **算力池请求链路** ("pool request chain") — pool entry, selected account, upstream API, with a trace id
- An expandable 查看输入输出 ("view input/output") section showing the request JSON and response body

If this passes, your account, protocol, bridging, and model mappings are all correct. If it fails, the error and the request chain tell you which hop broke.

For finer debugging, open **「实时日志」** ("live log") from the toolbar dropdown. It shows protocol translation in four stages: original request, sent upstream, raw upstream response, final response.

## Step 4: Point your CLI at the local proxy

There are two ways. The first is recommended.

### Option A: Let AI Switch write the config

Click the **📄 button** in the toolbar (tooltip 写入路由配置文件, "write route config files"). It requires the proxy to be running.

AI Switch points the current platform's CLI config at the local proxy. For Codex it edits `~/.codex/config.toml`, adding a model provider named `ai-switch` and selecting it, and writes a model catalog to `~/.codex/ai-switch-model-catalog.json`.

The write is a **safe direct write**: snapshot first, atomic write, concurrent-modification detection, guarded rollback. Your existing settings are preserved.

A 配置写入结果 ("config write result") panel appears below, listing each target's path, status, operation id, and snapshot id.

Target files per platform:

| Platform | File |
| --- | --- |
| Codex | `~/.codex/config.toml` |
| Claude Code | `~/.claude/settings.json` |
| Gemini CLI | `~/.gemini/settings.json` |
| Grok | `~/.grok/settings.json` |

Then just run the CLI:

```bash
codex
```

Every request it makes now goes through AI Switch.

### Option B: Configure manually

OpenCode, OpenClaw, and Hermes don't support native config writing, and you may simply prefer to control the config yourself.

You need two values, both in the toolbar dropdown (click ▾ for 更多测试操作, "more test actions"):

- **「复制 Base URL」** ("copy base URL") — the current proxy address, e.g. `http://127.0.0.1:19527`
- **「复制 sk」** ("copy sk") — the local proxy key, shaped like `sk-ai-switch-<uuid>`

The proxy key is **one per platform**. It is never displayed in plain text; you can only copy it.

Its job is to tell the proxy which platform a request belongs to. Send it in the `Authorization: Bearer`, `x-api-key`, or `x-goog-api-key` header, or as a `key` / `api_key` query parameter. It is **local authentication only and is never forwarded upstream** — the upstream sees the account's own real key.

Put those two values into your tool's base URL and API key settings.

### Verify with curl

To check directly, send a request yourself. The menu items 复制 curl（PowerShell / Git Bash） and 复制 CMD curl generate ready-made commands, or write it out:

```bash
curl -X POST http://127.0.0.1:19527/responses \
  -H "Authorization: Bearer sk-ai-switch-your-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-5.5",
    "input": "Reply with exactly: ai-switch-ok",
    "stream": false
  }'
```

```powershell
$key = "sk-ai-switch-your-key"
$body = @{
  model  = "gpt-5.5"
  input  = "Reply with exactly: ai-switch-ok"
  stream = $false
} | ConvertTo-Json

curl.exe -X POST http://127.0.0.1:19527/responses `
  -H "Authorization: Bearer $key" `
  -H "Content-Type: application/json" `
  -d $body
```

For the Claude platform, use `/v1/messages` and an Anthropic Messages body.

The proxy also serves an OpenAI-style model list. A `GET` to the models path returns the deduplicated client-facing model ids aggregated across every account in the pool, without forwarding upstream. That's what 查看模型列表 ("view model list") in the menu shows.

## Step 5: Confirm billing in the usage stats

Switch the bottom view to **「统计」** ("stats").

Four time windows sit at the top: **当日 / 本周 / 本月 / 累计** (today, this week, this month, all time).

Below them, six metric cards:

| Card | Meaning |
| --- | --- |
| 请求 | request count |
| 输入 Token | total input tokens |
| 输出 Token | total output tokens |
| 缓存 Token | total cached tokens |
| Token 总计 | input + output |
| 总费用（USD） | total spend converted to USD |

Under that is **「请求列表」** ("request list"), one row per request: timestamp, **account name**, status, path, model, token total, price, source, and a 详情 ("details") expander.

This is where you confirm billing. Your test request from earlier should already be listed:

- **Account name** tells you which account actually served the request — an immediate check on whether your priorities are doing what you intended
- **Price** is the cost of that request, recorded in whatever currency the account reported (USD or CNY, each to six decimal places)
- **Model** is the final requested model name, so a mapping that took effect shows its mapped value here

The stats refresh every 5 seconds, and the request list pages 20 rows at a time.

::: warning About cost figures
Costs come from pricing information the upstream returns in its responses. If an upstream doesn't report price, nothing is recorded here. The total card converts to USD, with CNY converted at a fixed rate of 7.1, so treat the number as **indicative** rather than as a bill. Your provider's dashboard is authoritative.
:::

## After the first request works

Once traffic flows, these are the usual next moves.

**Add more accounts and lay out the priorities.** This is where the pool earns its keep — the primary dies and traffic drops down without you noticing. See [Accounts and the Pool](/en/guide/accounts).

**Set up auto recovery for failed accounts.** The 自动恢复 ("auto recovery") setting in the edit panel supports daily schedules and health-check probing, so a rate-limited account returns to the pool on its own. See [Reliability and Auto Recovery](/en/guide/reliability).

**Configure your other CLIs.** Switch to another platform in the sidebar and repeat. Each platform has its own pool and its own proxy key. Check [Platform Support Matrix](/en/guide/platform-support) first for what's supported.

**Understand protocol bridging.** To reason about which accounts can serve which CLIs, see [Protocol Routing and Bridging](/en/guide/protocol-routing).

**Access it from a browser or phone.** See [Web Service Mode](/en/deploy/web-service).

**Work directly in the terminal.** See [Vibe Terminal and Skins](/en/features/vibe) and [Session Management](/en/features/sessions).

## Troubleshooting

| Error | Cause |
| --- | --- |
| 代理未启动 | The proxy was never started, or failed to start |
| `No enabled route credentials in pool` | No usable account in the pool — all disabled, archived, or not in a healthy state |
| `route_pool.model_unmatched` | No account in the pool serves the requested model; check your model mappings |
| `route_pool.concurrency_exhausted` | Every account is at its concurrency ceiling; raise 最大并发数 or add accounts |
| `route_proxy.key_invalid` | The proxy key is invalid, expired, or belongs to another AI Switch instance; copy it again |
| `route_proxy.platform_unresolved` | The request carried neither a proxy key nor a platform header |

More answers are in the [FAQ](/en/faq), and model testing is covered in [Model Connectivity Tests](/en/guide/model-test).
