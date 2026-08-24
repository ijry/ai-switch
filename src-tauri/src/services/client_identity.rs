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

/// Impersonated Codex CLI originator/User-Agent.
pub const CODEX_CLI_ORIGINATOR: &str = "codex_cli_rs";
const CODEX_CLI_VERSION: &str = "0.80.0";

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

/// Fill-if-missing identity headers that make an OpenAI/Responses-dialect
/// request look like the Codex CLI.
pub fn codex_cli_identity_headers() -> Vec<(&'static str, String)> {
    vec![
        ("user-agent", codex_cli_user_agent()),
        ("originator", CODEX_CLI_ORIGINATOR.to_string()),
    ]
}

/// Codex CLI User-Agent, e.g. `codex_cli_rs/0.80.0 (Windows 15.7.2; x86_64) Terminal`.
pub fn codex_cli_user_agent() -> String {
    format!(
        "{CODEX_CLI_ORIGINATOR}/{CODEX_CLI_VERSION} ({} 15.7.2; {}) Terminal",
        os_name(),
        arch_name()
    )
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
        assert!(ua.starts_with("codex_cli_rs/0.80.0 ("));
        assert!(ua.ends_with(") Terminal"));
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
