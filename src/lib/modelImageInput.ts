import modelImageInputRules from "../modelCapabilities.json";

type RuleMatch = "exact" | "prefix" | "contains";

type ImageInputRule = {
  match: RuleMatch;
  pattern: string;
  supportsImageInput: boolean;
};

const rules = modelImageInputRules.rules as ImageInputRule[];
const unknownDefault = modelImageInputRules.unknownDefault;

function normalizeModelId(model: string): string {
  const trimmed = model.trim().toLowerCase();
  const segment = trimmed.includes("/") ? trimmed.slice(trimmed.lastIndexOf("/") + 1) : trimmed;
  return segment.replace(/_/g, "-");
}

/** Built-in defaults for whether a model accepts images in normal chat input. */
export function defaultSupportsImageInput(model: string): boolean {
  const id = normalizeModelId(model);
  for (const rule of rules) {
    const matched =
      rule.match === "exact"
        ? id === rule.pattern
        : rule.match === "prefix"
          ? id.startsWith(rule.pattern)
          : id.includes(rule.pattern);
    if (matched) {
      return rule.supportsImageInput;
    }
  }
  return unknownDefault;
}
