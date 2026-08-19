---
title: Relay Providers
description: A hand-tested list of third-party AI relay providers, with rates, signup credit, referral terms, and how to wire them into AI Switch. Links on this page carry referral parameters.
---

# Relay Providers

Third-party AI relay providers that have been **verified to work**, plus how to wire them into AI Switch.

::: info About the links on this page
The signup button on each card is a **referral link** (it carries an `aff` parameter). Signing up through it gives you the new-user credit shown on the card, and also earns the project author the corresponding referral credit. If you would rather not generate a referral, go to the provider's site directly and sign up there — nothing about your usage changes.
:::

::: warning Read this first
These providers are **not operated by AI Switch and are outside this project's control**. Listing one only means it was tested working at a point in time; it is not a warranty or an endorsement:

- **Credit, rates, and referral terms change without notice**, and a provider may throttle, disappear, or shut down. Each card carries its verification date — past three months, trust the provider's own announcements over this page.
- **A relay can see the full content of every request you send through it.** Confidential code, production credentials, and customer data should not go through a third-party relay, regardless of which one.
- **Treat a public-benefit relay as something that can stop at any time.** Don't make it your primary, and never make it your only path.
- For problems, use the provider's own support channels — this project cannot handle payments, refunds, or ban appeals on your behalf.
:::

<RelayCards />

## Wiring one into AI Switch

Once you have the relay's base URL and API key, this is no different from adding any other API account:

1. Open the **Accounts** screen, pick the platform (Codex / Claude Code / …), and create a new API account.
2. Fill in the **base URL** and **API key**.
3. Pick the **upstream protocol** — this decides which bridge path is used, and getting it wrong produces an immediate protocol error. Relays normally document which interface format they are compatible with:
   - Compatible with OpenAI Chat Completions → `openai`
   - Compatible with OpenAI Responses → `openai-responses`
   - Compatible with Anthropic Messages → `anthropic`
   - Compatible with Gemini generateContent → `gemini`
4. Save, then run a [model connectivity test](/en/guide/model-test) to confirm it actually works before adding it to the pool.

::: tip If the model list won't load, type it in
Some relays don't expose a model-list endpoint, or the names it returns don't match what actually works. AI Switch lets you enter model names by hand — copy the model IDs out of the relay's documentation and forwarding works fine. The cards above note which providers need this and the exact model names.
:::

### Running several relays together

Relays are generally less reliable than official endpoints, which makes them a good fit for the [pool](/en/guide/accounts):

- Set your primary account's **route priority** to 1 and relays to 3. While the primary is healthy everything goes there; when it fails, traffic falls through to the relays.
- Give several relay accounts the **same priority** and the pool will round-robin across them, spreading the load.
- Relays are sensitive to concurrency — **leave the concurrency limit at the default of 1**.
- For providers that reset quota daily, set [auto recovery](/en/guide/reliability) to scheduled, just after the reset time. Use the health-check mode when the rate-limit window is unpredictable.

### Map model names when they disagree

Relay model IDs frequently differ from what the client asks for. You don't need to touch the CLI: use the account's **model mapping** to map the client-side model name onto the real upstream one. The request list then shows `requested->upstream`, so you can see at a glance that the mapping took effect.

## Suggesting a provider

PRs and issues are welcome, but please include **your own test results** (how you signed up, which models actually worked, the rate) and the date you tested. Entries that are just a link with no verification won't be listed.

The data lives in `docs-site/docs/.vitepress/theme/relays.ts`, with the Chinese and English copy in the same entry — update both when you change one.

## Next steps

- [Accounts and the pool](/en/guide/accounts): priority, concurrency, and the state machine
- [Protocol routing and bridging](/en/guide/protocol-routing): four upstream protocols and seven bridge paths
- [Model connectivity tests](/en/guide/model-test): confirm a relay account really works
- [Reliability and auto recovery](/en/guide/reliability): backoff and recovery configuration
