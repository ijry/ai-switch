import type { SkillSource } from "../../lib/api/types";
import { supportedLanguages } from "../../lib/i18n";
import type { Language, TranslationKey } from "../../lib/i18n";
import { bundledSkillCopy } from "./skillCopy";

export const DEFAULT_SKILL_CONTENT = "---\nname: New skill\ndescription: Describe what this skill does\n---\n\n# New skill\n\n";

const SKILL_PACKAGE_NAME_KEYS: Record<string, TranslationKey> = {
  "ai-switch.core": "skills.packageAiSwitchCore",
  "ai-switch.science": "skills.packageAiSwitchScience",
};

export function skillPackageNameKey(id: string): TranslationKey | undefined {
  return SKILL_PACKAGE_NAME_KEYS[id];
}

const SKILL_PACKAGE_SUMMARY_KEYS: Record<string, TranslationKey> = {
  "ai-switch.core": "skills.packageAiSwitchCoreSummary",
  "ai-switch.science": "skills.packageAiSwitchScienceSummary",
};

export function skillPackageSummaryKey(id: string): TranslationKey | undefined {
  return SKILL_PACKAGE_SUMMARY_KEYS[id];
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

type SkillLike = {
  id: string;
  name?: string | null;
  description?: string | null;
};

export type SkillDisplayCopy = {
  name: string;
  description: string | null;
};

/// What the list and the detail header show for a Skill.
///
/// The Skills AI Switch bundles carry frontmatter written for the agent, not for a
/// human scanning a list: `name` is the kebab-case id and `description` is a long
/// English trigger sentence. So for a bundled id the shipped copy wins in both
/// languages — that is what makes the Simplified Chinese UI read as Chinese without
/// rewriting the Skill files, and it also spares English users the 400-character
/// trigger blob. The preview pane still renders the real file verbatim.
export function skillDisplayCopy(item: SkillLike, language: Language): SkillDisplayCopy {
  const bundled = bundledSkillCopy(item.id, language);
  if (bundled) {
    return { name: bundled.name, description: bundled.summary };
  }
  return {
    name: item.name?.trim() || item.id,
    description: item.description?.trim() || null,
  };
}

/// Filter text is matched against every language's copy plus the raw metadata, so
/// "头脑风暴" and "brainstorming" find the same Skill whichever language the UI is in.
export function skillSearchHaystack(item: SkillLike): string {
  const parts = [item.id, item.name ?? "", item.description ?? ""];
  for (const { code } of supportedLanguages) {
    const copy = bundledSkillCopy(item.id, code);
    if (copy) parts.push(copy.name, copy.summary);
  }
  return parts.join(" ").toLowerCase();
}
