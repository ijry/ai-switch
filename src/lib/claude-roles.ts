/**
 * The Claude role slots AI Switch exposes as request-model aliases.
 *
 * Shared by the mapping editor and the account-list summary so the two cannot
 * drift: the summary previously carried its own copy of the alias list, which is
 * how it ended up showing internal aliases where the editor showed role names.
 *
 * These values are ours — they are what a client asks for and what the proxy
 * rewrites into each account's real upstream model. The env *keys* they get
 * written to (`ANTHROPIC_DEFAULT_*_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL`) belong
 * to Claude Code and are defined on the Rust side.
 */

export const CLAUDE_SUBAGENT_ALIAS = "claude-subagent";
export const CLAUDE_FALLBACK_ALIAS = "*";

export type ClaudeRole = {
  /** The alias a client requests, stored as a mapping's `from`. */
  alias: string;
  /** Human-facing role name. This is what lists and summaries should show. */
  label: string;
  /** Whether the row's display name can be edited. */
  editableLabel: boolean;
  /**
   * Whether this role may advertise a `[1m]` variant.
   *
   * Only Haiku is excluded: it is the small fast model with no 1M context tier.
   * Subagent and the fallback do get the flag — the proxy strips the `[1m]`
   * suffix before resolving a mapping, so `claude-subagent[1m]` matches the same
   * entry as `claude-subagent`.
   */
  supportsOneM: boolean;
  /** Placeholder shown instead of an editable display name. */
  hint: string | null;
  /** Substrings used to auto-pick an upstream model for this role. */
  keywords: readonly string[];
};

/** The four roles Claude Code shows in its `/model` menu, in menu order. */
export const CLAUDE_MENU_ROLES: readonly ClaudeRole[] = [
  {
    alias: "claude-sonnet-alias",
    label: "Sonnet",
    editableLabel: true,
    supportsOneM: true,
    hint: null,
    keywords: ["sonnet"],
  },
  {
    alias: "claude-opus-alias",
    label: "Opus",
    editableLabel: true,
    supportsOneM: true,
    hint: null,
    keywords: ["opus"],
  },
  {
    alias: "claude-fable-alias",
    label: "Fable",
    editableLabel: true,
    supportsOneM: true,
    hint: null,
    keywords: ["fable"],
  },
  {
    alias: "claude-haiku-alias",
    label: "Haiku",
    editableLabel: true,
    supportsOneM: false,
    hint: null,
    keywords: ["haiku", "flash", "mini", "lite"],
  },
];

/**
 * Roles that exist outside the `/model` menu. Appended after the menu roles so
 * editor rows keep their positions — row aria-labels are 1-indexed by position.
 */
export const CLAUDE_EXTRA_ROLES: readonly ClaudeRole[] = [
  {
    alias: CLAUDE_SUBAGENT_ALIAS,
    label: "Subagent",
    editableLabel: false,
    supportsOneM: true,
    hint: "不显示在 /model 菜单",
    keywords: [],
  },
  {
    alias: CLAUDE_FALLBACK_ALIAS,
    label: "默认兜底模型",
    editableLabel: false,
    supportsOneM: true,
    hint: "未匹配的模型走这里",
    keywords: [],
  },
];

export const CLAUDE_ROLES: readonly ClaudeRole[] = [...CLAUDE_MENU_ROLES, ...CLAUDE_EXTRA_ROLES];

export function claudeRoleForAlias(alias: string): ClaudeRole | undefined {
  const trimmed = alias.trim();
  return CLAUDE_ROLES.find((role) => role.alias === trimmed);
}

/**
 * Role name for an alias, falling back to the alias itself.
 *
 * A hand-written alias has no role, and showing it verbatim is correct — it is
 * what the user typed.
 */
export function claudeRoleLabel(alias: string): string {
  return claudeRoleForAlias(alias)?.label ?? alias.trim();
}

/**
 * Whether an alias may declare 1M support.
 *
 * Unknown aliases are allowed it: only the built-in roles have a context tier we
 * can reason about.
 */
export function claudeAliasSupportsOneM(alias: string): boolean {
  const role = claudeRoleForAlias(alias);
  return role ? role.supportsOneM : true;
}
