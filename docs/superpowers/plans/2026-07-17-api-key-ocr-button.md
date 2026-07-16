# API Key OCR Button Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an API Key OCR button that recognizes API keys from clipboard or selected images in the API account form.

**Architecture:** Add a small OCR helper module for clipboard image reading, image loading, OCR invocation, and API key extraction. Wire it into `AccountsScreen` beside the existing Base64 decode button, with focused Vitest coverage.

**Tech Stack:** React 18, TypeScript, Vitest, Testing Library, existing `ocrad.js` wrapper.

## Global Constraints

- Work directly on `main`; do not create or switch branches/worktrees.
- Reuse the existing offline OCR engine and do not add network OCR.
- Do not modify unrelated dirty files such as `src-tauri/Cargo.toml`.

---

### Task 1: API Key OCR Helper

**Files:**
- Create: `src/lib/ocr/apiKeyOcr.ts`
- Test: `tests/ApiKeyOcr.test.ts`

**Interfaces:**
- Produces: `readClipboardImageBlob(): Promise<Blob | null>`
- Produces: `recognizeApiKeysFromImageBlob(blob: Blob): Promise<string>`
- Produces: `extractApiKeysFromOcrText(text: string): string`

- [ ] **Step 1: Write extraction tests**

Add tests for extracting `sk-...`, `AIza...`, JWT-like tokens, and fallback cleaned text.

- [ ] **Step 2: Implement helper**

Use `navigator.clipboard.read()` for clipboard images, load blobs into an `HTMLImageElement`, call `recognizeImageText`, and clean OCR output with `extractApiKeysFromOcrText`.

- [ ] **Step 3: Verify helper tests**

Run: `pnpm vitest run tests/ApiKeyOcr.test.ts`

### Task 2: Accounts Screen Integration

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: `readClipboardImageBlob()`
- Consumes: `recognizeApiKeysFromImageBlob(blob: Blob): Promise<string>`

- [ ] **Step 1: Write UI tests**

Cover clipboard image recognition replacing API Key and clipboard miss falling back to a selected image file.

- [ ] **Step 2: Implement UI**

Add OCR state, hidden image file input, `OCR识别` button, clipboard-first flow, file fallback flow, and user-facing errors.

- [ ] **Step 3: Verify screen tests**

Run: `pnpm vitest run tests/AccountsScreen.test.tsx tests/ApiKeyOcr.test.ts`

### Task 3: Regression Verification

**Files:**
- No additional files.

- [ ] **Step 1: Typecheck**

Run: `pnpm typecheck`

- [ ] **Step 2: Full tests**

Run: `pnpm test:run`

