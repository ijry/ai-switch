//! How the pool names the models it advertises, and how a client-facing name is
//! resolved back to one account.
//!
//! Aggregate mode merges every account's aliases into one word list and the
//! router rotates. Precise mode expands the list per account so a request can
//! name the account it wants: `<账号前缀>/<别名>`.
//!
//! Official accounts are never expanded. Their bodies are forwarded without
//! model rewriting, so every official account in a pool serves exactly the same
//! model set, and pooling them exists for quota rotation — pinning one would
//! cancel the only reason they are pooled. They share the reserved
//! [`OFFICIAL_MODEL_PREFIX`] instead.

/// Shared prefix for every official account in the pool.
pub(crate) const OFFICIAL_MODEL_PREFIX: &str = "official";

/// Longest account prefix we emit. Model ids end up as JSON object keys (ZCode)
/// and `slug` values (Codex catalog), and a 200-character account name would
/// make the picker unreadable long before it broke anything.
const MAX_PREFIX_CHARS: usize = 32;

/// How many characters of the credential id disambiguate two accounts that
/// sanitize to the same prefix.
const COLLISION_SUFFIX_CHARS: usize = 6;

/// How many characters of the credential id stand in for a name that sanitizes
/// to nothing at all.
const FALLBACK_PREFIX_CHARS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PoolModelMode {
    /// Today's behavior: one merged, de-duplicated word list for the pool.
    #[default]
    Aggregate,
    /// One entry per API account, plus one merged entry set for the official
    /// accounts.
    Precise,
}

impl PoolModelMode {
    /// Anything unrecognized reads as [`PoolModelMode::Aggregate`]: a broken
    /// switch must never be the thing that makes the pool unusable.
    pub(crate) fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "precise" => Self::Precise,
            _ => Self::Aggregate,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Aggregate => "aggregate",
            Self::Precise => "precise",
        }
    }
}

/// The prefix an account would carry if no other account collided with it.
///
/// Keeps CJK: most users name their relays in Chinese, and a model id is a JSON
/// string everywhere it lands. Drops what would make the name ambiguous or
/// unparseable instead — `/` because it is the separator, `:` because Gemini
/// paths end in `:generateContent`, whitespace because config editors and shells
/// mangle it.
pub(crate) fn account_model_prefix(display_name: &str, credential_id: &str) -> String {
    let sanitized = sanitize_prefix(display_name);
    if sanitized.is_empty() {
        return format!("acct-{}", short_id(credential_id, FALLBACK_PREFIX_CHARS));
    }
    if sanitized.eq_ignore_ascii_case(OFFICIAL_MODEL_PREFIX) {
        // The reserved prefix outranks an account that happens to be called
        // "official": losing one account's plain name is recoverable, losing the
        // whole official group's addressability is not.
        return with_collision_suffix(&sanitized, credential_id);
    }
    sanitized
}

/// Prefixes for a whole pool, in the order the members were given.
///
/// Two accounts that sanitize to the same prefix **both** get a suffix, never
/// just the second one. The prefix is then a pure function of one account, which
/// is what lets the catalog and the router agree: the router only ever sees the
/// accounts that are healthy right now, so an order- or set-dependent suffix
/// would drift the moment an account went `error`.
pub(crate) fn assign_member_prefixes(members: &[(&str, &str)]) -> Vec<String> {
    let base: Vec<String> = members
        .iter()
        .map(|(id, display_name)| account_model_prefix(display_name, id))
        .collect();

    base.iter()
        .enumerate()
        .map(|(index, prefix)| {
            let collides = base
                .iter()
                .enumerate()
                .any(|(other, candidate)| other != index && candidate.eq_ignore_ascii_case(prefix));
            if collides {
                with_collision_suffix(prefix, members[index].0)
            } else {
                prefix.clone()
            }
        })
        .collect()
}

/// Every prefix the router accepts for one account.
///
/// Both the bare prefix and the suffixed one, because the catalog may have been
/// written while a same-named account existed (or while it did not). A name that
/// only exists in suffixed form — the reserved word, or a name that sanitizes to
/// nothing — is accepted in that form alone.
pub(crate) fn accepted_prefixes(display_name: &str, credential_id: &str) -> Vec<String> {
    let base = account_model_prefix(display_name, credential_id);
    let suffixed = with_collision_suffix(&base, credential_id);
    if base == suffixed {
        return vec![base];
    }
    vec![base, suffixed]
}

/// Split a client-facing model id into its account prefix and the alias behind
/// it, at the **first** separator: relay model names carry vendor paths of their
/// own (`z-ai/glm-5.3`), so everything after the first `/` belongs to the alias.
pub(crate) fn split_prefixed_model(model: &str) -> Option<(&str, &str)> {
    let (prefix, rest) = model.trim().split_once('/')?;
    let prefix = prefix.trim();
    let rest = rest.trim();
    if prefix.is_empty() || rest.is_empty() {
        return None;
    }
    Some((prefix, rest))
}

/// Whether this prefix addresses the official group rather than one account.
pub(crate) fn is_official_model_prefix(prefix: &str) -> bool {
    prefix.trim().eq_ignore_ascii_case(OFFICIAL_MODEL_PREFIX)
}

fn sanitize_prefix(display_name: &str) -> String {
    let mut out = String::new();
    for character in display_name.trim().chars() {
        if character.is_alphanumeric() || matches!(character, '.' | '_' | '-') {
            out.push(character);
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.chars().count() >= MAX_PREFIX_CHARS {
            break;
        }
    }
    out.trim_matches('-').to_string()
}

fn with_collision_suffix(prefix: &str, credential_id: &str) -> String {
    format!(
        "{prefix}-{}",
        short_id(credential_id, COLLISION_SUFFIX_CHARS)
    )
}

/// Ids are UUIDs, but a hand-seeded row can be anything — including a name with
/// its own dashes, which is why this sanitizes too.
fn short_id(credential_id: &str, chars: usize) -> String {
    let sanitized = sanitize_prefix(credential_id);
    let source = if sanitized.is_empty() {
        credential_id.trim()
    } else {
        sanitized.as_str()
    };
    let taken: String = source.chars().take(chars).collect();
    if taken.is_empty() {
        "unknown".to_string()
    } else {
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parsing_falls_back_to_aggregate() {
        assert_eq!(PoolModelMode::parse("precise"), PoolModelMode::Precise);
        assert_eq!(PoolModelMode::parse(" Precise "), PoolModelMode::Precise);
        assert_eq!(PoolModelMode::parse("aggregate"), PoolModelMode::Aggregate);
        // A hand-edited row must not take the pool down with it.
        assert_eq!(
            PoolModelMode::parse("per-account"),
            PoolModelMode::Aggregate
        );
        assert_eq!(PoolModelMode::parse(""), PoolModelMode::Aggregate);
        assert_eq!(PoolModelMode::default(), PoolModelMode::Aggregate);
        assert_eq!(PoolModelMode::Precise.as_str(), "precise");
    }

    #[test]
    fn prefixes_drop_separators_and_keep_cjk() {
        assert_eq!(account_model_prefix("TaBiAI", "id-1"), "TaBiAI");
        assert_eq!(account_model_prefix("  247Kan  ", "id-1"), "247Kan");
        // `/` is the separator and `:` ends a Gemini path, so neither may survive.
        assert_eq!(account_model_prefix("z-ai/glm", "id-1"), "z-ai-glm");
        assert_eq!(account_model_prefix("a:b", "id-1"), "a-b");
        assert_eq!(account_model_prefix("my  relay", "id-1"), "my-relay");
        assert_eq!(account_model_prefix("小明的中转", "id-1"), "小明的中转");
        assert_eq!(account_model_prefix("gpt.5_pro-x", "id-1"), "gpt.5_pro-x");
    }

    #[test]
    fn prefixes_are_capped_and_never_end_in_a_separator() {
        let long = "a".repeat(64);
        assert_eq!(account_model_prefix(&long, "id-1").chars().count(), 32);
        let trailing = format!("{} !", "b".repeat(31));
        assert_eq!(account_model_prefix(&trailing, "id-1"), "b".repeat(31));
    }

    #[test]
    fn a_name_that_sanitizes_to_nothing_falls_back_to_the_id() {
        assert_eq!(
            account_model_prefix("///", "abcdef1234-5678"),
            "acct-abcdef12"
        );
        assert_eq!(account_model_prefix("", "abcdef1234"), "acct-abcdef12");
    }

    #[test]
    fn the_reserved_official_prefix_outranks_an_account_called_official() {
        assert_eq!(
            account_model_prefix("official", "abcdef1234"),
            "official-abcdef"
        );
        assert_eq!(
            account_model_prefix("Official", "abcdef1234"),
            "Official-abcdef"
        );
        // …and that account is then only addressable in its suffixed form, so a
        // bare `official/` request still reaches the official group.
        let accepted = accepted_prefixes("official", "abcdef1234");
        assert!(accepted.contains(&"official-abcdef".to_string()));
        assert!(!accepted.iter().any(|prefix| prefix == "official"));
    }

    #[test]
    fn same_named_accounts_both_get_a_suffix() {
        let prefixes = assign_member_prefixes(&[
            ("aaaaaa1111", "TaBiAI"),
            ("bbbbbb2222", "tabiai"),
            ("cccccc3333", "Grox"),
        ]);

        // Both, not just the second: the prefix has to stay a pure function of
        // one account, or it would shift when a sibling stops being healthy.
        assert_eq!(
            prefixes,
            vec![
                "TaBiAI-aaaaaa".to_string(),
                "tabiai-bbbbbb".to_string(),
                "Grox".to_string(),
            ]
        );
    }

    #[test]
    fn the_router_accepts_both_the_bare_and_the_suffixed_prefix() {
        // The catalog may have been written either way, depending on whether a
        // same-named account existed at the time.
        assert_eq!(
            accepted_prefixes("TaBiAI", "aaaaaa1111"),
            vec!["TaBiAI".to_string(), "TaBiAI-aaaaaa".to_string()]
        );
    }

    #[test]
    fn models_split_at_the_first_separator_only() {
        assert_eq!(
            split_prefixed_model("Grox/z-ai/glm-5.3"),
            Some(("Grox", "z-ai/glm-5.3"))
        );
        assert_eq!(
            split_prefixed_model(" official / gpt-5.6-sol "),
            Some(("official", "gpt-5.6-sol"))
        );
        assert_eq!(split_prefixed_model("gpt-5.6-sol"), None);
        assert_eq!(split_prefixed_model("/gpt-5.6-sol"), None);
        assert_eq!(split_prefixed_model("Grox/"), None);
    }

    #[test]
    fn the_official_prefix_is_matched_case_insensitively() {
        assert!(is_official_model_prefix("official"));
        assert!(is_official_model_prefix(" OFFICIAL "));
        assert!(!is_official_model_prefix("official-abcdef"));
    }
}
