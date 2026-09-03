/**
 * Client-side mirror of the Codex catalog defaults in
 * `src-tauri/src/services/route_model_capability.rs`.
 *
 * The mapping editor has to show which context window and which reasoning
 * efforts a row will advertise *before* the account is saved, and the backend is
 * what actually writes the catalog. So the two tables have to agree: change one
 * and change the other. A mapping that declares nothing keeps using the
 * baseline, which is how every mapping written before these fields existed
 * behaves.
 */

/** Efforts the editor offers. The backend keeps whatever is stored, so this list
 * bounds what the UI can produce rather than what the catalog can carry. */
export const CODEX_REASONING_LEVEL_OPTIONS = [
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
  "ultra",
] as const;

/** GPT baseline models ship with a known effort ladder; the editor preselects it
 * so "auto" and "explicit" start from the same place. */
const CODEX_BASELINE_REASONING_PROFILES: Record<string, readonly string[]> = {
  "gpt-5.6-sol": ["low", "medium", "high", "xhigh", "max", "ultra"],
  "gpt-5.6-terra": ["low", "medium", "high", "xhigh", "max", "ultra"],
  "gpt-5.6-luna": ["low", "medium", "high", "xhigh", "max"],
  "gpt-5.5": ["low", "medium", "high", "xhigh"],
};

/** Anything the profile table does not know — a relay's own model id — starts on
 * the three efforts every reasoning model understands. */
const CODEX_DEFAULT_REASONING_LEVELS: readonly string[] = ["low", "medium", "high"];

export function codexBaselineReasoningLevels(model: string): readonly string[] {
  return (
    CODEX_BASELINE_REASONING_PROFILES[model.trim().toLowerCase()] ??
    CODEX_DEFAULT_REASONING_LEVELS
  );
}

/**
 * What one row will actually advertise. An absent *or empty* list means "use the
 * baseline": the editor writes an empty list when the user unticks every box,
 * and an empty effort menu would leave the client with nothing to pick.
 */
export function codexEffectiveReasoningLevels(
  model: string,
  declared: string[] | null | undefined,
): readonly string[] {
  const normalized = normalizeCodexReasoningLevels(declared);
  return normalized.length > 0 ? normalized : codexBaselineReasoningLevels(model);
}

/** Trim, lowercase and dedupe — the same normalization the backend applies, so a
 * round trip through the editor cannot change what gets advertised. */
export function normalizeCodexReasoningLevels(
  declared: string[] | null | undefined,
): string[] {
  if (!Array.isArray(declared)) {
    return [];
  }
  const seen = new Set<string>();
  const normalized: string[] = [];
  for (const level of declared) {
    if (typeof level !== "string") {
      continue;
    }
    const value = level.trim().toLowerCase();
    if (!value || seen.has(value)) {
      continue;
    }
    seen.add(value);
    normalized.push(value);
  }
  return normalized;
}

/** True when the row leans on the baseline instead of a hand-picked list. */
export function usesCodexBaselineReasoning(
  declared: string[] | null | undefined,
): boolean {
  return normalizeCodexReasoningLevels(declared).length === 0;
}

/**
 * Context windows offered as one-click choices.
 *
 * Deliberately decimal (1M = 1_000_000, not 1_048_576): the Claude 1M tier uses
 * the binary value as its marker, and matching it here would make a Codex row
 * look like a Claude 1M declaration to the CPA transfer format.
 */
export const CODEX_CONTEXT_WINDOW_OPTIONS = [
  { value: 128_000, label: "128K" },
  { value: 200_000, label: "200K" },
  { value: 256_000, label: "256K" },
  { value: 400_000, label: "400K" },
  { value: 1_000_000, label: "1M" },
] as const;

/** Written into the catalog when a row declares no window and its upstream model
 * is not one of the known 1M families. Mirrors `CODEX_DEFAULT_CONTEXT_WINDOW` on
 * the Rust side — the conservative number on purpose: under-claiming only makes
 * Codex compact early, while over-claiming lets it pack to 95% of a window the
 * upstream does not have and the turn dies on a 400. A row that knows better
 * declares its own value. */
export const CODEX_DEFAULT_CONTEXT_WINDOW = 128_000;

/** Decimal 1M, for the same reason the option list uses it. */
export const CODEX_ONE_M_CONTEXT_WINDOW = 1_000_000;

/**
 * Upstream model families that really serve 1M context, matched on the start of
 * the mapped-to name so every dated or sized variant is covered
 * (`deepseek-v4-flash-0731`, `glm-5.3-air`, …).
 */
export const CODEX_ONE_M_UPSTREAM_PREFIXES = [
  "deepseek-v4",
  "glm-5.2",
  "glm-5.3",
  "qwen-3.8",
  "kimi-k3",
] as const;

/**
 * The window a row will advertise when it declares none. Keyed by the *upstream*
 * model, not the alias the client asks for, since the window is a property of
 * what actually serves the request.
 *
 * Relays publish these families under a vendor path as often as bare
 * (`z-ai/glm-5.3`), so the last path segment is tried too — otherwise the same
 * model would silently fall back to the generic default on half the relays.
 */
export function codexDefaultContextWindow(upstreamModel: string): number {
  const name = upstreamModel.trim().toLowerCase();
  const bare = name.slice(name.lastIndexOf("/") + 1);
  return CODEX_ONE_M_UPSTREAM_PREFIXES.some(
    (prefix) => name.startsWith(prefix) || bare.startsWith(prefix),
  )
    ? CODEX_ONE_M_CONTEXT_WINDOW
    : CODEX_DEFAULT_CONTEXT_WINDOW;
}

/** Label for a stored window, including one an import brought in that is not on
 * the option list. */
export function codexContextWindowLabel(value: number): string {
  const known = CODEX_CONTEXT_WINDOW_OPTIONS.find((option) => option.value === value);
  if (known) {
    return known.label;
  }
  if (value >= 1_000_000 && value % 1_000_000 === 0) {
    return `${value / 1_000_000}M`;
  }
  if (value >= 1_000 && value % 1_000 === 0) {
    return `${value / 1_000}K`;
  }
  return String(value);
}

/** Reads a stored `context_window`, rejecting the shapes a hand-edited config or
 * an older export can hold. */
export function normalizeCodexContextWindow(
  value: unknown,
): number | null {
  const parsed =
    typeof value === "number"
      ? value
      : typeof value === "string" && value.trim()
        ? Number(value)
        : Number.NaN;
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return null;
  }
  return Math.trunc(parsed);
}
