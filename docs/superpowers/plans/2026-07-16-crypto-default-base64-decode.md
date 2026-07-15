# Crypto Default Base64 Decode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Base64 decode the default operation when opening Crypto Tools.

**Architecture:** Change only the `CryptoToolsScreen` initial React state and align its screen test. Encoding/decoding core logic remains unchanged.

**Tech Stack:** React 18, TypeScript, Vitest, Testing Library.

## Global Constraints

- Work directly on `main`.
- Other operations remain available and unchanged.
- Do not touch unrelated worktree changes such as `src-tauri/Cargo.toml`.

---

### Task 1: Default Crypto Operation

**Files:**
- Modify: `src/screens/CryptoToolsScreen.tsx`
- Modify: `tests/CryptoToolsScreen.test.tsx`

**Interfaces:**
- Consumes: `CryptoOperation` from `src/lib/cryptoTransforms.ts`
- Produces: default screen state `operation === "base64-decode"`

- [ ] **Step 1: Update the failing test**

Change the default behavior test in `tests/CryptoToolsScreen.test.tsx` to:

```tsx
it("decodes Base64 text by default", async () => {
  renderScreen();

  await userEvent.type(screen.getByLabelText("输入文本"), "aGVsbG8g5LiW55WM");
  expect(screen.getByLabelText("输出文本")).toHaveValue("hello 世界");
});
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `pnpm vitest run tests/CryptoToolsScreen.test.tsx`

Expected: FAIL because the screen still defaults to `base64-encode`.

- [ ] **Step 3: Change the default state**

In `src/screens/CryptoToolsScreen.tsx`, change:

```ts
const [operation, setOperation] = useState<CryptoOperation>("base64-encode");
```

to:

```ts
const [operation, setOperation] = useState<CryptoOperation>("base64-decode");
```

- [ ] **Step 4: Verify**

Run: `pnpm vitest run tests/CryptoToolsScreen.test.tsx`

Expected: PASS.

Run: `pnpm typecheck`

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```powershell
git add src/screens/CryptoToolsScreen.tsx tests/CryptoToolsScreen.test.tsx
git commit -m "fix: default crypto tool to base64 decode"
```

Expected: commit includes only the screen and test changes.

---

## Self-Review

- Spec coverage: default operation and test update are covered.
- Placeholder scan: no placeholder or deferred work remains.
- Type consistency: `base64-decode` is already part of `CryptoOperation`.
