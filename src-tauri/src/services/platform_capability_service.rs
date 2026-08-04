use crate::{
    error::AppError,
    models::platform::{
        CapabilityAvailability, CapabilityRule, PlatformCapability, PlatformId, PlatformOperation,
        PlatformOperations, SupportLevel,
    },
};

const API_CREDENTIALS_ONLY: &str = "capability.api_credentials_only";
const NATIVE_CONFIG_UNAVAILABLE: &str = "capability.native_config_unavailable";
const OFFICIAL_ACCOUNT_UNAVAILABLE: &str = "capability.official_account_unavailable";
const DEEPLINK_UNAVAILABLE: &str = "capability.deeplink_unavailable";
const QUOTA_UNAVAILABLE: &str = "capability.quota_unavailable";

pub struct PlatformCapabilityService;

impl PlatformCapabilityService {
    pub fn list() -> Vec<PlatformCapability> {
        PlatformId::ALL.into_iter().map(capability_for).collect()
    }

    pub fn get(platform: PlatformId) -> PlatformCapability {
        capability_for(platform)
    }

    pub fn require(
        platform: PlatformId,
        operation: PlatformOperation,
    ) -> Result<CapabilityRule, AppError> {
        let capability = Self::get(platform);
        let rule = capability.operations.get(operation).clone();
        if rule.availability == CapabilityAvailability::Unavailable {
            return Err(AppError::Validation {
                code: "capability.unavailable",
                message: format!(
                    "{} does not support {}",
                    platform.display_name(),
                    operation.as_str()
                ),
                details: rule.reason_code.clone(),
                recoverable: true,
            });
        }
        Ok(rule)
    }
}

fn capability_for(platform: PlatformId) -> PlatformCapability {
    let partial = matches!(
        platform,
        PlatformId::OpenCode | PlatformId::OpenClaw | PlatformId::Hermes
    );
    let official_quota = if platform == PlatformId::Gemini || partial {
        unavailable(QUOTA_UNAVAILABLE)
    } else {
        supported()
    };
    let operations = if partial {
        PlatformOperations {
            route_credentials: supported(),
            generic_api_routing: api_credentials_only(),
            config_write: unavailable(NATIVE_CONFIG_UNAVAILABLE),
            official_import: unavailable(OFFICIAL_ACCOUNT_UNAVAILABLE),
            official_account_routing: unavailable(OFFICIAL_ACCOUNT_UNAVAILABLE),
            deeplink_import: unavailable(DEEPLINK_UNAVAILABLE),
            official_quota,
            model_test: api_credentials_only(),
            terminal_launch: supported(),
            session_resume: supported(),
        }
    } else {
        PlatformOperations {
            route_credentials: supported(),
            generic_api_routing: supported(),
            config_write: supported(),
            official_import: supported(),
            official_account_routing: supported(),
            deeplink_import: supported(),
            official_quota,
            model_test: supported(),
            terminal_launch: supported(),
            session_resume: supported(),
        }
    };

    PlatformCapability {
        platform,
        display_name: platform.display_name().to_string(),
        support_level: if partial {
            SupportLevel::Partial
        } else {
            SupportLevel::Supported
        },
        operations,
    }
}

fn supported() -> CapabilityRule {
    CapabilityRule {
        availability: CapabilityAvailability::Supported,
        reason_code: None,
        credential_kinds: Vec::new(),
        requires_base_url: false,
        requires_api_dialect: false,
    }
}

fn api_credentials_only() -> CapabilityRule {
    CapabilityRule {
        availability: CapabilityAvailability::Partial,
        reason_code: Some(API_CREDENTIALS_ONLY.to_string()),
        credential_kinds: vec!["api".to_string()],
        requires_base_url: true,
        requires_api_dialect: true,
    }
}

fn unavailable(reason_code: &str) -> CapabilityRule {
    CapabilityRule {
        availability: CapabilityAvailability::Unavailable,
        reason_code: Some(reason_code.to_string()),
        credential_kinds: Vec::new(),
        requires_base_url: false,
        requires_api_dialect: false,
    }
}

#[cfg(test)]
mod tests {
    use super::PlatformCapabilityService;
    use crate::{
        error::AppError,
        models::platform::{CapabilityAvailability, PlatformId, PlatformOperation, SupportLevel},
    };

    #[test]
    fn capability_matrix_matches_phase_a_contract() {
        let matrix = PlatformCapabilityService::list();
        assert_eq!(matrix.len(), 7);

        let hermes = matrix
            .iter()
            .find(|item| item.platform == PlatformId::Hermes)
            .unwrap();
        assert_eq!(hermes.support_level, SupportLevel::Partial);
        assert_eq!(
            hermes.operations.config_write.availability,
            CapabilityAvailability::Unavailable
        );
        assert_eq!(
            hermes.operations.generic_api_routing.availability,
            CapabilityAvailability::Partial
        );
        assert_eq!(
            hermes.operations.official_account_routing.availability,
            CapabilityAvailability::Unavailable
        );

        let gemini = matrix
            .iter()
            .find(|item| item.platform == PlatformId::Gemini)
            .unwrap();
        assert_eq!(
            gemini.operations.config_write.availability,
            CapabilityAvailability::Supported
        );
        assert_eq!(
            gemini.operations.official_quota.availability,
            CapabilityAvailability::Unavailable
        );
    }

    #[test]
    fn require_rejects_only_unavailable_operations() {
        let partial = PlatformCapabilityService::require(
            PlatformId::Hermes,
            PlatformOperation::GenericApiRouting,
        )
        .expect("partial capability remains callable");
        assert_eq!(partial.availability, CapabilityAvailability::Partial);

        let error =
            PlatformCapabilityService::require(PlatformId::Hermes, PlatformOperation::ConfigWrite)
                .expect_err("native Hermes config writing is unavailable");
        assert!(matches!(
            error,
            AppError::Validation {
                code: "capability.unavailable",
                ..
            }
        ));
    }
}
