# AI Switch Official Account Quota Refresh C5 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a real official account quota refresh path using explicit HTTPS JSON endpoint metadata.

**Architecture:** Extend the existing account quota cache service with a metadata-driven HTTP JSON adapter. The backend owns endpoint validation, auth environment lookup, response parsing, redaction, snapshot insertion, and account linking. The frontend adds a refresh button beside the existing manual quota form.

**Tech Stack:** Tauri 2, Rust, `reqwest`, SQLite, React, TypeScript, Vitest.

## Global Constraints

- Do not scrape private official account sessions.
- Do not store raw tokens.
- Require HTTPS quota endpoints.
- Allow auth only through environment-variable references.
- Store quota results in existing `quota_snapshots`.
- Preserve manual quota snapshot recording.

---

### Task 1: Backend Quota Refresh

- [x] Add `RefreshAccountQuotaSnapshotRequest`.
- [x] Add `AccountService::refresh_account_quota_snapshot`.
- [x] Validate `account_metadata_json.quota_query.endpoint_url`.
- [x] Fetch JSON with optional `Authorization` from `auth_env_key`.
- [x] Parse `status`, `remaining_label`, `reset_at`, and `summary`.
- [x] Redact sensitive keys from stored raw excerpts.
- [x] Add Tauri command `refresh_official_account_quota_snapshot`.
- [x] Add Rust tests for parsing, redaction, and metadata validation.

### Task 2: Frontend Quota Refresh

- [x] Add API type/client wrapper.
- [x] Add Accounts UI refresh action.
- [x] Add API client test.
- [x] Add Accounts screen test.
- [x] Update README with C5 metadata and verification notes.

### Task 3: Verification

- [x] Run `cargo fmt`.
- [x] Run `pnpm typecheck`.
- [x] Run `pnpm test:run`.
- [x] Run `pnpm rust:check`.
- [x] Run `pnpm rust:test`.
