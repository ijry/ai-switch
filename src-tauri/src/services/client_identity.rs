//! Shared CLI client-identity headers.
//!
//! Some upstream gateways (e.g. `agentrouter.org`) fingerprint incoming
//! requests and reject anything that does not look like the official Claude
//! Code / Codex CLI with `unauthorized client detected`. To pass those gates
//! we mirror the real CLI request signatures, matching how cc-switch spoofs
//! them.
//!
//! These headers are applied to both the local route proxy (for outbound
//! upstream requests) and the model-list fetch. Keep the versions roughly in
//! sync with the CLIs they impersonate.

/// Anthropic beta marker upstream gateways look for to verify the request came
/// from Claude Code. Must be present in the `anthropic-beta` header.
pub const CLAUDE_CODE_BETA_MARKER: &str = "claude-code-20250219";
/// Beta marker that actually enables Anthropic's 1M context window.
///
/// The `[1M]` suffix Claude Code appends to a model value only advertises the
/// capability in its `/model` menu — the suffix is stripped before the upstream
/// request, so without this marker nothing tells the gateway to open the larger
/// window and it answers "1m 上下文已经全量可用，请启用 1m 上下文后重试".
pub const ANTHROPIC_ONE_M_CONTEXT_BETA: &str = "context-1m-2025-08-07";
/// Default `anthropic-beta` value when the client did not send one.
pub const CLAUDE_CODE_DEFAULT_BETA: &str = "claude-code-20250219,interleaved-thinking-2025-05-14";
/// Impersonated Claude Code CLI User-Agent.
pub const CLAUDE_CODE_USER_AGENT: &str = "claude-cli/2.1.2 (external, cli)";

/// Claude Code's official system-prompt first block.
///
/// Gateways running sub2api's `claude_code_only` group gate score the request's
/// `system` blocks against this exact string (Dice coefficient ≥ 0.5) and reject
/// anything below the threshold with `this group only allows Claude Code
/// clients`. Header spoofing alone does not pass — the body has to carry it too.
pub const CLAUDE_CODE_SYSTEM_PROMPT: &str =
    "You are Claude Code, Anthropic's official CLI for Claude.";

/// Build a synthetic `metadata.user_id` accepted by sub2api's group gate.
///
/// The gate parses this field and rejects the request when it does not match
/// either the legacy `user_<64hex>_account_<uuid>_session_<uuid>` shape or the
/// JSON shape used from CLI 2.1.78 on. We emit the JSON form to match the
/// version advertised in [`CLAUDE_CODE_USER_AGENT`].
///
/// `seed` keys the derived device id so a caller can stay stable across
/// requests; the session id is random per call, as a real CLI session would be.
pub fn claude_code_metadata_user_id(seed: &str) -> String {
    let device_id = derive_device_id(seed);
    let session_id = uuid::Uuid::new_v4();
    format!(r#"{{"device_id":"{device_id}","account_uuid":"","session_id":"{session_id}"}}"#)
}

/// Derive the 64-char hex device id the gate's regex requires.
///
/// SHA-256 hex is exactly 64 chars, and hashing keeps the id stable per seed
/// without leaking the seed (a credential id) to the upstream relay.
fn derive_device_id(seed: &str) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(seed.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Impersonated Codex CLI originator/User-Agent.
pub const CODEX_CLI_ORIGINATOR: &str = "codex_cli_rs";
/// Codex engine version claimed in the impersonated User-Agent.
///
/// Relays read this out of the UA's leading `client/version` segment and gate on
/// it — sub2api's `codex_cli_only` refuses anything below the configured minimum,
/// and OpenAI's own inference endpoint 404s clients that predate its floor. Keep
/// it at the latest published `@openai/codex` release.
const CODEX_CLI_VERSION: &str = "0.153.4";

/// Header the Codex engine stamps on every inference request.
///
/// sub2api's `codex_cli_only` gate ships exactly one *required* engine-fingerprint
/// signal: any header whose name starts with [`CODEX_ENGINE_HEADER_PREFIX`], on
/// the grounds that ~98.8% of real Codex traffic carries this one. Looking like
/// the official client in `user-agent`/`originator` is therefore not enough — a
/// request without the fingerprint is refused with the very same generic
/// `This account only allows Codex official clients` message.
pub const CODEX_WINDOW_ID_HEADER: &str = "x-codex-window-id";

/// Prefix that marks a header as part of the Codex engine fingerprint.
pub const CODEX_ENGINE_HEADER_PREFIX: &str = "x-codex-";

/// Official Codex client names, exactly as the engine reports them in
/// `originator` and in the leading UA segment.
///
/// Mirrors codex-rs's first-party list (`login/src/auth/default_client.rs`) plus
/// the app-server clients relay gates keep on their own allowlists. Matching is
/// on the whole name rather than "contains codex", so `evil-codex_cli_rs` is not
/// mistaken for the real thing — the same narrowing the gates apply.
const CODEX_OFFICIAL_CLIENT_NAMES: &[&str] = &[
    "codex_cli_rs",
    "codex-tui",
    "codex_vscode",
    "codex_vscode_copilot",
    "codex_app",
    "codex_chatgpt_desktop",
    "codex_atlas",
    "codex_exec",
    "codex_sdk_ts",
];

/// Prefix covering the `Codex Desktop`-style family.
///
/// The trailing space is load-bearing: trimmed to a bare `codex` it would match
/// any name containing the word.
const CODEX_OFFICIAL_CLIENT_FAMILY_PREFIX: &str = "codex ";

/// Fill-if-missing identity headers that make an Anthropic-dialect request look
/// like Claude Code. `anthropic-beta` is handled separately (it must be merged,
/// not just filled) via [`CLAUDE_CODE_BETA_MARKER`].
pub fn claude_code_identity_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("user-agent", CLAUDE_CODE_USER_AGENT),
        ("x-app", "cli"),
        ("anthropic-dangerous-direct-browser-access", "true"),
        ("x-stainless-lang", "js"),
        ("x-stainless-package-version", "0.70.0"),
        ("x-stainless-os", os_name()),
        ("x-stainless-arch", arch_name()),
        ("x-stainless-runtime", "node"),
        ("x-stainless-runtime-version", "v22.20.0"),
        ("x-stainless-retry-count", "0"),
        ("x-stainless-timeout", "600"),
    ]
}

/// Identity headers that make an OpenAI/Responses-dialect request look like the
/// Codex CLI.
///
/// Applied as a set, never spliced into a caller's own headers — see
/// [`codex_paired_originator`] for what a half-Codex identity costs.
pub fn codex_cli_identity_headers() -> Vec<(&'static str, String)> {
    vec![
        ("user-agent", codex_cli_user_agent()),
        ("originator", CODEX_CLI_ORIGINATOR.to_string()),
    ]
}

/// Codex CLI User-Agent, e.g. `codex_cli_rs/0.153.4 (Windows 11; x86_64) Terminal`.
pub fn codex_cli_user_agent() -> String {
    format!(
        "{CODEX_CLI_ORIGINATOR}/{CODEX_CLI_VERSION} ({} {}; {}) Terminal",
        os_name(),
        os_version_hint(),
        arch_name()
    )
}

/// OS version reported next to [`os_name`] in the Codex UA.
///
/// codex-rs fills this from `os_info`; we keep one plausible current release per
/// OS instead of taking a dependency to probe it. No gate parses the value, but a
/// pair that cannot exist (`Windows 15.7.2`) is the kind of tell a fingerprinting
/// relay looks for.
fn os_version_hint() -> &'static str {
    match std::env::consts::OS {
        "macos" => "15.7.2",
        "windows" => "11",
        _ => "22.4.0",
    }
}

/// The `originator` that pairs with `user_agent`, when that UA belongs to a Codex
/// client a relay gate accepts as official.
///
/// `None` means "not a Codex client", and the caller should then replace the whole
/// identity instead of grafting an official `originator` onto a foreign UA. Both
/// halves of that answer matter:
///
/// * sub2api's version check reads the *UA*, not `originator`. An official
///   originator next to `python-requests/2.32` gives the gate a client it accepts
///   and a version it cannot parse, which it rejects as `codex_version_undetectable`
///   — reported as the same generic "only allows Codex official clients".
/// * OpenAI's `/backend-api/codex` 404s when `originator` and the UA's leading
///   client name disagree, so filling in `codex_cli_rs` beside a `codex-tui/…` UA
///   breaks a request that would have gone through untouched.
pub fn codex_paired_originator(user_agent: &str) -> Option<String> {
    let user_agent = user_agent.trim();
    let (leading, rest) = user_agent.split_once('/')?;
    // An official name with an unparseable version is refused just like an unknown
    // client, so it is not an identity worth keeping.
    if !has_codex_engine_version(rest) {
        return None;
    }
    if let Some(name) = official_codex_client_name(leading) {
        return Some(name);
    }
    // `CODEX_INTERNAL_ORIGINATOR_OVERRIDE` renames the leading segment but leaves
    // the engine's own `(name; version)` trailer, so a wrapper such as
    // `cccc/0.153.4 (…) (codex-tui; 0.153.4)` is still a real Codex engine and the
    // gates read the trailer to recognise it.
    official_codex_client_name(&codex_user_agent_trailer_name(user_agent)?)
}

/// Match one client name against the official set.
fn official_codex_client_name(candidate: &str) -> Option<String> {
    let candidate = candidate.trim();
    let lowered = candidate.to_ascii_lowercase();
    if CODEX_OFFICIAL_CLIENT_NAMES.contains(&lowered.as_str()) {
        // The exact set is canonically lowercase, so `CODEX_CLI_RS` is the same
        // client rather than a second identity.
        return Some(lowered);
    }
    // The `Codex ` family is a prefix rather than a fixed name, so it is the one
    // place a client could talk us into echoing arbitrary bytes back as an
    // "official" originator. Official names are short ASCII.
    if candidate.len() > 64
        || !candidate
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
    {
        return None;
    }
    // Matched case-sensitively upstream, so this family's own casing has to survive.
    lowered
        .starts_with(CODEX_OFFICIAL_CLIENT_FAMILY_PREFIX)
        .then(|| candidate.to_string())
}

/// True when a UA's version segment starts with the three-part engine version a
/// relay's version gate parses (`0.153.4-alpha.3` counts, `8.0` does not).
///
/// `rest` is everything after the first `/`; the version ends at the first space
/// or `(`, matching how codex-rs formats the UA.
fn has_codex_engine_version(rest: &str) -> bool {
    let version = rest.split([' ', '(']).next().unwrap_or_default();
    let mut segments = version.splitn(3, '.');
    let (Some(major), Some(minor), Some(patch)) =
        (segments.next(), segments.next(), segments.next())
    else {
        return false;
    };
    let patch_digits = patch
        .split(|char: char| !char.is_ascii_digit())
        .next()
        .unwrap_or_default();
    !patch_digits.is_empty()
        && [major, minor]
            .iter()
            .all(|segment| !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The `name` of the trailing `(name; version)` group codex-rs appends to its UA.
fn codex_user_agent_trailer_name(user_agent: &str) -> Option<String> {
    let (_, trailer) = user_agent.rsplit_once('(')?;
    let (inner, _) = trailer.split_once(')')?;
    let name = inner.split(';').next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Stable `x-codex-window-id` for a routed account.
///
/// A real CLI mints one per window and keeps it for that window's lifetime, so a
/// fresh id on every request would itself be an odd fingerprint. Deriving it from
/// the account id gives all of that account's traffic one window — what a single
/// user's CLI looks like — while hashing keeps the account id out of the value.
pub fn codex_engine_window_id(seed: &str) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(seed.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // Sets the version/variant bits, so the value still parses as the v4 UUID the
    // CLI would have generated.
    uuid::Builder::from_random_bytes(bytes)
        .into_uuid()
        .to_string()
}

/// OS name mapped to the value the Claude/Codex CLIs report.
pub fn os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "MacOS",
        "linux" => "Linux",
        "windows" => "Windows",
        other => other,
    }
}

/// CPU architecture mapped to the value the Claude/Codex CLIs report.
pub fn arch_name() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        "x86" => "x86",
        other => other,
    }
}

/// Merge the Claude Code beta marker into an existing `anthropic-beta` value.
///
/// Returns `None` when the marker is already present (no change needed).
pub fn merge_claude_code_beta(existing: Option<&str>) -> Option<String> {
    match existing.map(str::trim).filter(|value| !value.is_empty()) {
        Some(existing) => merge_beta_marker(Some(existing), CLAUDE_CODE_BETA_MARKER),
        // No header at all: seed the full default rather than the bare marker, so
        // interleaved thinking is not silently dropped.
        None => Some(CLAUDE_CODE_DEFAULT_BETA.to_string()),
    }
}

/// Merge the 1M-context marker into an existing `anthropic-beta` value.
///
/// Returns `None` when the marker is already present (no change needed).
pub fn merge_one_m_context_beta(existing: Option<&str>) -> Option<String> {
    merge_beta_marker(existing, ANTHROPIC_ONE_M_CONTEXT_BETA)
}

/// Prepend `marker` to a comma-separated `anthropic-beta` value unless it is
/// already listed. Returns `None` when nothing needs to change.
///
/// Comparison is on trimmed segments: gateways accept `a, b` with spaces, and
/// treating ` context-1m-2025-08-07` as a different marker would duplicate it.
fn merge_beta_marker(existing: Option<&str>, marker: &str) -> Option<String> {
    match existing.map(str::trim).filter(|value| !value.is_empty()) {
        Some(existing) => {
            if existing.split(',').any(|part| part.trim() == marker) {
                None
            } else {
                Some(format!("{marker},{existing}"))
            }
        }
        None => Some(marker.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_beta_marker_when_absent() {
        assert_eq!(
            merge_claude_code_beta(Some("interleaved-thinking-2025-05-14")),
            Some("claude-code-20250219,interleaved-thinking-2025-05-14".to_string())
        );
    }

    #[test]
    fn skips_merge_when_marker_present() {
        assert_eq!(
            merge_claude_code_beta(Some("foo,claude-code-20250219,bar")),
            None
        );
    }

    #[test]
    fn defaults_beta_when_missing() {
        assert_eq!(
            merge_claude_code_beta(None),
            Some(CLAUDE_CODE_DEFAULT_BETA.to_string())
        );
        assert_eq!(
            merge_claude_code_beta(Some("   ")),
            Some(CLAUDE_CODE_DEFAULT_BETA.to_string())
        );
    }

    #[test]
    fn merges_one_m_marker_only_once() {
        // The `[1M]` model suffix is stripped before the upstream request, so this
        // marker is the only thing that actually opens the larger window.
        assert_eq!(
            merge_one_m_context_beta(Some("claude-code-20250219")),
            Some("context-1m-2025-08-07,claude-code-20250219".to_string())
        );
        assert_eq!(
            merge_one_m_context_beta(None),
            Some(ANTHROPIC_ONE_M_CONTEXT_BETA.to_string())
        );
        // Already present: no change, and no duplicate.
        assert_eq!(
            merge_one_m_context_beta(Some("a,context-1m-2025-08-07,b")),
            None
        );
        // Gateways accept spaces after commas; a space must not read as a
        // different marker and duplicate it.
        assert_eq!(
            merge_one_m_context_beta(Some("a, context-1m-2025-08-07")),
            None
        );
    }

    #[test]
    fn codex_user_agent_has_expected_shape() {
        let ua = codex_cli_user_agent();
        assert!(ua.starts_with(&format!("codex_cli_rs/{CODEX_CLI_VERSION} (")));
        assert!(ua.ends_with(") Terminal"));
        // Our own UA has to clear the version gate it is meant to satisfy.
        assert_eq!(
            codex_paired_originator(&ua),
            Some(CODEX_CLI_ORIGINATOR.to_string())
        );
    }

    #[test]
    fn pairs_the_originator_with_the_clients_own_codex_user_agent() {
        // The bug this pairing fixes: filling in `codex_cli_rs` beside a
        // `codex-tui` UA is the mismatch OpenAI's inference endpoint 404s on.
        assert_eq!(
            codex_paired_originator("codex-tui/0.153.4 (Ubuntu 22.4.0; x86_64) xterm-256color"),
            Some("codex-tui".to_string())
        );
        assert_eq!(
            codex_paired_originator("codex_vscode/1.0.0 (Windows 11; x86_64) vscode"),
            Some("codex_vscode".to_string())
        );
        // The exact set is canonically lowercase.
        assert_eq!(
            codex_paired_originator("CODEX_CLI_RS/0.153.4 (Windows 11; x86_64) Terminal"),
            Some("codex_cli_rs".to_string())
        );
        // `Codex ` family: matched case-sensitively upstream, so the casing stays.
        assert_eq!(
            codex_paired_originator("Codex Desktop/1.4.0 (MacOS 15.7.2; arm64) Codex"),
            Some("Codex Desktop".to_string())
        );
        // Originator override renames the leading segment; the engine's own
        // `(name; version)` trailer still identifies it.
        assert_eq!(
            codex_paired_originator(
                "cccc/0.153.4 (MacOS 15.7.2; arm64) iTerm2 (codex-tui; 0.153.4)"
            ),
            Some("codex-tui".to_string())
        );
    }

    #[test]
    fn refuses_to_pair_an_originator_with_a_foreign_user_agent() {
        // Every case here is one the gate would reject anyway, so the caller has to
        // replace the identity wholesale instead of topping it up.
        for user_agent in [
            "claude-cli/2.1.2 (external, cli)",
            "python-requests/2.32",
            "curl/8.18.0",
            // Official name, version the gate cannot parse: `codex_version_undetectable`.
            "codex_cli_rs/beta (Windows 11; x86_64) Terminal",
            // The prefix has to lead — "contains codex" is how fakes get through.
            "evil-codex_cli_rs/0.153.4 (Windows 11; x86_64) Terminal",
            // Trailer is the OS group, not a client identity.
            "unknown/1.2.3 (Windows 11; x86_64) Terminal",
            "",
        ] {
            assert_eq!(
                codex_paired_originator(user_agent),
                None,
                "should not be treated as an official Codex client: {user_agent}"
            );
        }
        // The `Codex ` family is a prefix, so it needs its own bound: an official
        // client name is short ASCII, not a paragraph a caller wants echoed back as
        // its originator.
        let padded = format!(
            "Codex {}/1.2.3 (Windows 11; x86_64) Terminal",
            "x".repeat(64)
        );
        assert_eq!(codex_paired_originator(&padded), None);
    }

    #[test]
    fn window_id_is_a_stable_uuid_per_account() {
        let first = codex_engine_window_id("account-1");
        let again = codex_engine_window_id("account-1");
        let other = codex_engine_window_id("account-2");

        assert_eq!(first, again, "one account is one window");
        assert_ne!(first, other, "different accounts are different windows");
        let parsed = uuid::Uuid::parse_str(&first).expect("uuid shape");
        assert_eq!(parsed.get_version_num(), 4, "the CLI generates v4: {first}");
    }

    #[test]
    fn metadata_user_id_matches_gateway_json_shape() {
        let raw = claude_code_metadata_user_id("account-1");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("json");

        let device_id = parsed["device_id"].as_str().expect("device_id");
        assert_eq!(
            device_id.len(),
            64,
            "gate requires 64 hex chars: {device_id}"
        );
        assert!(device_id.chars().all(|c| c.is_ascii_hexdigit()));

        // The gate parses session_id as a 36-char UUID.
        let session_id = parsed["session_id"].as_str().expect("session_id");
        assert_eq!(session_id.len(), 36);
        assert!(uuid::Uuid::parse_str(session_id).is_ok());

        assert_eq!(parsed["account_uuid"], "");
    }

    #[test]
    fn metadata_device_id_is_stable_per_seed_but_session_is_not() {
        let extract = |raw: &str| -> (String, String) {
            let value: serde_json::Value = serde_json::from_str(raw).expect("json");
            (
                value["device_id"].as_str().unwrap().to_string(),
                value["session_id"].as_str().unwrap().to_string(),
            )
        };

        let (device_a, session_a) = extract(&claude_code_metadata_user_id("account-1"));
        let (device_b, session_b) = extract(&claude_code_metadata_user_id("account-1"));
        let (device_c, _) = extract(&claude_code_metadata_user_id("account-2"));

        assert_eq!(device_a, device_b, "same seed must reuse the device id");
        assert_ne!(device_a, device_c, "different seeds must differ");
        assert_ne!(session_a, session_b, "each probe is its own session");
    }

    #[test]
    fn claude_identity_includes_stainless_headers() {
        let headers = claude_code_identity_headers();
        assert!(headers.iter().any(|(name, _)| *name == "x-app"));
        assert!(headers
            .iter()
            .any(|(name, _)| *name == "x-stainless-package-version"));
    }
}
