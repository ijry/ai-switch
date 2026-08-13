import type { SkillSource } from "../../lib/api/types";
import type { TranslationKey } from "../../lib/i18n";

export const DEFAULT_SKILL_CONTENT = "---\nname: New skill\ndescription: Describe what this skill does\n---\n\n# New skill\n\n";

const SKILL_PACKAGE_NAME_KEYS: Record<string, TranslationKey> = {
  "ai-switch.core": "skills.packageAiSwitchCore",
  "ai-switch.science": "skills.packageAiSwitchScience",
};

export function skillPackageNameKey(id: string): TranslationKey | undefined {
  return SKILL_PACKAGE_NAME_KEYS[id];
}

const SKILL_SOURCE_LABEL_KEYS: Record<SkillSource, TranslationKey> = {
  builtin: "skills.sourceBuiltin",
  codex: "skills.sourceCodex",
  agents: "skills.sourceAgents",
  project: "skills.sourceProject",
  unknown: "skills.sourceUnknown",
};

export function skillSourceLabelKey(source: SkillSource): TranslationKey {
  return SKILL_SOURCE_LABEL_KEYS[source] ?? "skills.sourceUnknown";
}
