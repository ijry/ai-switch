import type { PlatformId } from "./api/types";

export const MODEL_TEST_MODELS_STORAGE_KEY = "ai-switch.model-test-models";

const POOL_KEY_PREFIX = "pool:";

export type ModelTestModelEntry = {
  model: string;
  platform: PlatformId;
};

export type ModelTestModelMap = Record<string, ModelTestModelEntry>;

export function poolModelTestKey(platform: PlatformId): string {
  return `${POOL_KEY_PREFIX}${platform}`;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function normalizeEntry(value: unknown): ModelTestModelEntry | null {
  if (!isPlainObject(value)) {
    return null;
  }
  const { model, platform } = value;
  if (typeof model !== "string" || typeof platform !== "string") {
    return null;
  }
  return { model, platform: platform as PlatformId };
}

export function loadModelTestModels(
  storage: Pick<Storage, "getItem"> = window.localStorage,
): ModelTestModelMap {
  try {
    const raw = storage.getItem(MODEL_TEST_MODELS_STORAGE_KEY);
    if (!raw) {
      return {};
    }
    const parsed: unknown = JSON.parse(raw);
    if (!isPlainObject(parsed)) {
      return {};
    }
    const map: ModelTestModelMap = {};
    for (const [key, value] of Object.entries(parsed)) {
      const entry = normalizeEntry(value);
      if (entry) {
        map[key] = entry;
      }
    }
    return map;
  } catch {
    // Storage can be unavailable in restricted browser contexts.
    return {};
  }
}

export function pruneModelTestModelMap(
  map: ModelTestModelMap,
  liveAccountIds: Iterable<string>,
  platform: PlatformId,
): ModelTestModelMap {
  const live = new Set(liveAccountIds);
  const orphans = Object.keys(map).filter((key) => {
    // Pool keys belong to no account, and other platforms' accounts are absent
    // from this platform's account list, so neither can be judged here.
    if (key.startsWith(POOL_KEY_PREFIX) || map[key].platform !== platform) {
      return false;
    }
    return !live.has(key);
  });
  if (orphans.length === 0) {
    // Same reference means "nothing changed" to both setState and the writer.
    return map;
  }
  const next = { ...map };
  for (const key of orphans) {
    delete next[key];
  }
  return next;
}

function writeMap(
  map: ModelTestModelMap,
  storage: Pick<Storage, "setItem">,
): void {
  try {
    storage.setItem(MODEL_TEST_MODELS_STORAGE_KEY, JSON.stringify(map));
  } catch {
    // Storage can be unavailable in restricted browser contexts.
  }
}

export function saveModelTestModel(
  key: string,
  model: string,
  platform: PlatformId,
  storage: Pick<Storage, "getItem" | "setItem"> = window.localStorage,
): void {
  const map = loadModelTestModels(storage);
  const trimmed = model.trim();
  if (trimmed) {
    map[key] = { model: trimmed, platform };
  } else {
    // An empty name means "no cache", so drop the key instead of storing "".
    delete map[key];
  }
  writeMap(map, storage);
}

export function pruneModelTestModels(
  liveAccountIds: Iterable<string>,
  platform: PlatformId,
  storage: Pick<Storage, "getItem" | "setItem"> = window.localStorage,
): void {
  const map = loadModelTestModels(storage);
  const pruned = pruneModelTestModelMap(map, liveAccountIds, platform);
  if (pruned !== map) {
    writeMap(pruned, storage);
  }
}
