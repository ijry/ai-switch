# Vibe Skin Audio Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add optional package-defined audio for Vibe skins, including event sounds, ambient sounds, and a persisted user-facing audio toggle.

**Architecture:** Extend `src/lib/vibeSkin.ts` to normalize and import safe audio assets exactly like image assets. Add a focused playback layer in `src/screens/VibeScreen.tsx` that owns user activation, ambient cleanup, and event playback, while skin components only emit semantic callbacks.

**Tech Stack:** React, TypeScript, Vitest, JSZip skin package import, browser `HTMLAudioElement`.

## Global Constraints

- Work directly on `main`.
- Do not allow remote audio URLs, absolute filesystem paths, arbitrary JavaScript, or skin-provided code.
- Only `.mp3`, `.ogg`, and `.wav` package audio assets are accepted.
- Skin audio must be optional and disabled through the appearance dialog.
- Existing skins without `audio` must remain behaviorally unchanged.

---

### Task 1: Skin Audio Schema And Import

**Files:**
- Modify: `src/lib/vibeSkin.ts`
- Modify: `tests/lib/vibeSkin.test.ts`

**Interfaces:**
- Produces: `VibeSkinAudioEvent`, `VibeSkinAudioDefinition`, and `VibeSkinDefinition.audio`.
- Produces: imported package audio references converted to `data:audio/...` URLs.

- [ ] **Step 1: Add failing tests for audio import and sanitization**

Add a test that imports a zip skin package with:

```ts
audio: {
  enabled: true,
  volume: 0.6,
  events: {
    agentSelect: "assets/sounds/weapon.ogg",
    hologramInteract: "assets/sounds/holo.wav",
    radarPulse: "https://example.com/rejected.mp3",
  },
  ambient: [
    { id: "radar", src: "assets/sounds/radar.mp3", loop: true, volume: 0.25 },
    { id: "bad", src: "C:/bad.wav" },
  ],
}
```

Expected assertions:

```ts
expect(skin.audio?.events?.agentSelect).toMatch(/^data:audio\/ogg;base64,/);
expect(skin.audio?.events?.hologramInteract).toMatch(/^data:audio\/wav;base64,/);
expect(skin.audio?.events?.radarPulse).toBeUndefined();
expect(skin.audio?.ambient).toHaveLength(1);
expect(skin.audio?.ambient?.[0]?.src).toMatch(/^data:audio\/mpeg;base64,/);
```

- [ ] **Step 2: Implement audio types and normalization**

Add:

```ts
export type VibeSkinAudioEvent = "agentSelect" | "hologramInteract" | "radarPulse";
export type VibeSkinAudioAmbient = {
  id: string;
  src: string;
  loop?: boolean;
  intervalMs?: number;
  volume?: number;
};
export type VibeSkinAudioDefinition = {
  enabled?: boolean;
  volume?: number;
  events?: Partial<Record<VibeSkinAudioEvent, string>>;
  ambient?: VibeSkinAudioAmbient[];
};
```

- [ ] **Step 3: Resolve package audio references**

Use a new audio resolver that mirrors image resolution but only accepts audio MIME types.

- [ ] **Step 4: Run tests**

Run: `pnpm vitest run tests/lib/vibeSkin.test.ts`

Expected: PASS.

### Task 2: Playback Runtime And Appearance Toggle

**Files:**
- Modify: `src/screens/VibeScreen.tsx`
- Modify: `src/lib/vibeSkin.ts`
- Modify: `src/lib/i18n.tsx`
- Modify: `tests/VibeScreen.test.tsx`

**Interfaces:**
- Consumes: `activeSkin.audio`.
- Produces: `skinAudioEnabled` stored in `VIBE_APPEARANCE_STORAGE_KEY`.
- Produces: `playSkinAudioEvent(eventName)`.

- [ ] **Step 1: Add appearance preference test**

Extend appearance dialog tests to assert the `Skin sound effects` / `皮肤音效` checkbox exists and persists.

- [ ] **Step 2: Extend stored appearance preference**

Add `skinAudioEnabled?: boolean` to the stored appearance type and read/write helpers.

- [ ] **Step 3: Add playback helpers inside `VibeScreen`**

Use:

```ts
const playSkinAudioEvent = useCallback((event: VibeSkinAudioEvent) => {
  if (!skinAudioEnabled || activeSkin.audio?.enabled === false) return;
  const src = activeSkin.audio?.events?.[event];
  if (!src) return;
  const audio = new Audio(src);
  audio.volume = clampVolume(activeSkin.audio?.volume ?? 0.5);
  void audio.play().catch(() => undefined);
}, [activeSkin.audio, skinAudioEnabled]);
```

- [ ] **Step 4: Add ambient lifecycle**

Start ambient audio only after first Vibe pointer/key interaction, and stop it when the skin, toggle, or screen changes.

- [ ] **Step 5: Add appearance dialog checkbox**

Add a checkbox labeled from i18n:

```tsx
<input type="checkbox" checked={skinAudioEnabled} onChange={(event) => setSkinAudioEnabled(event.target.checked)} />
```

- [ ] **Step 6: Run tests**

Run: `pnpm vitest run tests/VibeScreen.test.tsx`

Expected: PASS.

### Task 3: Starship Event Wiring And Built-In Package Assets

**Files:**
- Modify: `src/components/vibe/StarshipHologram.tsx`
- Modify: `src/screens/VibeScreen.tsx`
- Modify: `src/skins/starship-cockpit/skin.json`
- Create: `src/skins/starship-cockpit/assets/sounds/weapon-switch.wav`
- Create: `src/skins/starship-cockpit/assets/sounds/hologram-tap.wav`
- Create: `src/skins/starship-cockpit/assets/sounds/radar-pulse.wav`
- Modify: `tests/VibeScreen.test.tsx`
- Modify: `tests/lib/vibeSkin.test.ts`

**Interfaces:**
- Consumes: `playSkinAudioEvent("agentSelect" | "hologramInteract")`.
- Produces: built-in starship skin audio package metadata.

- [ ] **Step 1: Add callback prop to hologram**

Add:

```ts
type StarshipHologramProps = {
  className?: string;
  label: string;
  onInteract?: () => void;
};
```

Call `onInteract` on pointer down.

- [ ] **Step 2: Trigger agent sound**

Call `playSkinAudioEvent("agentSelect")` when an agent option is selected.

- [ ] **Step 3: Add small bundled WAV assets**

Generate short local WAV files for starship sounds and reference them from `skin.json`.

- [ ] **Step 4: Run full verification**

Run:

```bash
pnpm vitest run tests/VibeScreen.test.tsx tests/lib/vibeSkin.test.ts
pnpm typecheck
```

Expected: PASS.

### Task 4: Commit

**Files:**
- All files changed by Tasks 1-3.

- [ ] **Step 1: Review diff**

Run: `git diff --stat` and confirm only Vibe audio files changed.

- [ ] **Step 2: Commit**

Run:

```bash
git add -- docs/superpowers/plans/2026-07-19-vibe-skin-audio.md src tests
git commit -m "Add Vibe skin audio support"
```
