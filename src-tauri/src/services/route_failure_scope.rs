/// Whether a failure means "this credential or its network is broken" (park the
/// whole account) rather than "the upstream refused this one model" (park just
/// that model).
///
/// The split matters because a relay that rate-limits `gpt-5.6-sol` usually
/// still serves `glm-5.3` on the same key — parking the account would take out a
/// model that works.
pub(crate) fn is_account_scoped_failure(kind: &str, status: Option<u16>) -> bool {
    match kind {
        // The credential itself, or the path to the upstream, is at fault.
        "refresh" | "request_build" | "transport" | "model_test" => true,
        // A rejected key rejects every model, so settle it once at the account
        // level; every other status is the upstream's verdict on one model.
        "upstream_status" | "model_test_status" => matches!(status, Some(401) | Some(403)),
        // The upstream answered about this specific model.
        "semantic_response_transient" | "response_transform" => false,
        // Unknown kinds park the account: over-parking is recoverable, letting a
        // broken credential keep serving is not.
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::is_account_scoped_failure;

    #[test]
    fn credential_and_network_failures_park_the_whole_account() {
        for kind in ["refresh", "request_build", "transport", "model_test"] {
            assert!(
                is_account_scoped_failure(kind, None),
                "{kind} must be account scoped"
            );
        }
    }

    #[test]
    fn auth_rejections_park_the_whole_account() {
        // A dead key rejects every model, so charging each one separately would
        // just make the account fail N times before it settles.
        for kind in ["upstream_status", "model_test_status"] {
            assert!(is_account_scoped_failure(kind, Some(401)));
            assert!(is_account_scoped_failure(kind, Some(403)));
        }
    }

    #[test]
    fn other_upstream_statuses_park_only_the_requested_model() {
        for status in [400, 404, 408, 429, 500, 502, 503] {
            assert!(
                !is_account_scoped_failure("upstream_status", Some(status)),
                "status {status} must stay model scoped"
            );
            assert!(!is_account_scoped_failure(
                "model_test_status",
                Some(status)
            ));
        }
    }

    #[test]
    fn content_level_failures_park_only_the_requested_model() {
        for kind in ["semantic_response_transient", "response_transform"] {
            assert!(!is_account_scoped_failure(kind, Some(200)));
            assert!(!is_account_scoped_failure(kind, None));
        }
    }

    #[test]
    fn an_unknown_kind_falls_back_to_account_scope() {
        // Erring account-wide is the safe default: it can only over-park, never
        // let a broken credential keep serving.
        assert!(is_account_scoped_failure("something_new", Some(500)));
    }
}
