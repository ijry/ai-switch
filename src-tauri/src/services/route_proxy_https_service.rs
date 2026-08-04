use crate::app_state::AppState;
use crate::config_writer::ConfigWriter;
use crate::error::AppError;
use crate::models::config_snapshot::ConfigWriteOutcome;
use crate::models::route_proxy_https::{
    RouteProxyHttpsConfig, RouteProxyHttpsOperationOutcome, RouteProxyHttpsStatus,
    RouteProxyTrustRecord, RouteProxyTrustStatus,
};
use crate::paths::AppPaths;
use crate::services::route_config_service::RouteConfigService;
use crate::services::route_proxy_https_trust::{
    RouteProxyHttpsTrustExecutor, RouteProxyTrustOutcome, SystemRouteProxyHttpsTrustExecutor,
};
use crate::services::route_proxy_service::{
    RouteProxyService, RouteProxyStatus, RouteProxyTransport,
};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};
use serde::{Deserialize, Serialize};
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha256Digest, Sha256};
use std::path::{Path, PathBuf};
use time::{Duration, OffsetDateTime};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
use x509_parser::extensions::GeneralName;

const ROOT_CERTIFICATE_FILE: &str = "root-ca.pem";
const ROOT_PRIVATE_KEY_FILE: &str = "root-ca-key.pem";
const SERVER_CERTIFICATE_FILE: &str = "server-cert.pem";
const SERVER_PRIVATE_KEY_FILE: &str = "server-key.pem";
const METADATA_FILE: &str = "metadata.json";
const ROOT_COMMON_NAME: &str = "AI Switch Route Proxy Root CA";
const SERVER_COMMON_NAME: &str = "AI Switch Route Proxy localhost";
const ROOT_VALIDITY_DAYS: i64 = 3650;
const LEAF_VALIDITY_DAYS: i64 = 825;

#[derive(Debug, Clone)]
pub struct RouteProxyHttpsMaterial {
    pub root_certificate_pem: PathBuf,
    pub root_fingerprint_sha256: String,
    pub root_thumbprint_sha1: String,
    pub server_certificate_pem: PathBuf,
    pub server_private_key_pem: PathBuf,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RouteProxyHttpsMetadata {
    pub root_fingerprint_sha256: String,
    pub root_thumbprint_sha1: String,
    pub expires_at: String,
    #[serde(default)]
    pub trust: RouteProxyTrustRecord,
}

pub struct RouteProxyHttpsService;

impl RouteProxyHttpsService {
    pub async fn load_config(paths: &AppPaths) -> Result<RouteProxyHttpsConfig, AppError> {
        paths.ensure().await?;
        if !paths.route_proxy_https_config_file.exists() {
            return Ok(RouteProxyHttpsConfig::default());
        }

        let contents = tokio::fs::read_to_string(&paths.route_proxy_https_config_file).await?;
        serde_json::from_str(&contents).map_err(|error| AppError::Validation {
            code: "validation.route_proxy_https_config",
            message: "Local route proxy HTTPS configuration is invalid".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })
    }

    pub async fn save_config(
        paths: &AppPaths,
        config: &RouteProxyHttpsConfig,
    ) -> Result<(), AppError> {
        let contents = serde_json::to_string_pretty(config)?;
        ConfigWriter::write_atomic(&paths.route_proxy_https_config_file, &contents).await?;
        Ok(())
    }

    pub async fn ensure_material(paths: &AppPaths) -> Result<RouteProxyHttpsMaterial, AppError> {
        if let Some(material) = Self::load_material(paths).await? {
            return Ok(material);
        }

        Self::generate_material(paths).await
    }

    pub async fn transport(paths: &AppPaths) -> Result<RouteProxyTransport, AppError> {
        let config = Self::load_config(paths).await?;
        if !config.enabled {
            return Ok(RouteProxyTransport::Http);
        }

        let material = Self::ensure_material(paths).await?;
        Ok(Self::tls_transport(&material))
    }

    pub async fn start_proxy(state: &AppState) -> Result<RouteProxyStatus, AppError> {
        let transport = Self::transport(&state.paths).await?;
        let previous = RouteProxyService::status(&state.route_proxy).await;
        let route_proxy =
            RouteProxyService::start(&state.route_proxy, state.pool.clone(), transport).await?;

        match Self::rewrite_existing_configs(state, &route_proxy).await {
            Ok(_) => {}
            Err(error) => {
                if !previous.running {
                    let _ = RouteProxyService::stop(&state.route_proxy).await;
                }
                return Err(error);
            }
        }

        Ok(route_proxy)
    }

    pub async fn status_for_state(state: &AppState) -> Result<RouteProxyHttpsStatus, AppError> {
        Self::status(
            &state.paths,
            RouteProxyService::status(&state.route_proxy).await.base_url,
        )
        .await
    }

    pub async fn enable(state: &AppState) -> Result<RouteProxyHttpsOperationOutcome, AppError> {
        Self::enable_with_trust(state, &SystemRouteProxyHttpsTrustExecutor).await
    }

    pub(crate) async fn enable_with_trust(
        state: &AppState,
        trust_executor: &dyn RouteProxyHttpsTrustExecutor,
    ) -> Result<RouteProxyHttpsOperationOutcome, AppError> {
        let material = Self::ensure_material(&state.paths).await?;
        let trust = trust_executor.install(&material).await;
        Self::save_trust_outcome(&state.paths, trust).await?;

        let previous = RouteProxyService::status(&state.route_proxy).await;
        if previous.running {
            RouteProxyService::stop(&state.route_proxy).await?;
        }

        let route_proxy = match RouteProxyService::start(
            &state.route_proxy,
            state.pool.clone(),
            Self::tls_transport(&material),
        )
        .await
        {
            Ok(status) => status,
            Err(error) => {
                // A failed HTTPS transition must not strand a previously active proxy.
                if previous.running {
                    let _ = RouteProxyService::start(
                        &state.route_proxy,
                        state.pool.clone(),
                        RouteProxyTransport::Http,
                    )
                    .await;
                }
                Self::save_config(&state.paths, &RouteProxyHttpsConfig { enabled: false }).await?;
                return Err(error);
            }
        };

        Self::save_config(&state.paths, &RouteProxyHttpsConfig { enabled: true }).await?;
        let config_writes = Self::rewrite_existing_configs(state, &route_proxy).await?;
        let https =
            Self::status_with_trust(&state.paths, route_proxy.base_url.clone(), trust_executor)
                .await?;

        Ok(RouteProxyHttpsOperationOutcome {
            https,
            route_proxy,
            config_writes,
        })
    }

    pub async fn disable(state: &AppState) -> Result<RouteProxyHttpsOperationOutcome, AppError> {
        let previous = RouteProxyService::status(&state.route_proxy).await;
        if previous.running {
            RouteProxyService::stop(&state.route_proxy).await?;
        }
        Self::save_config(&state.paths, &RouteProxyHttpsConfig { enabled: false }).await?;

        let route_proxy = RouteProxyService::start(
            &state.route_proxy,
            state.pool.clone(),
            RouteProxyTransport::Http,
        )
        .await?;
        let config_writes = Self::rewrite_existing_configs(state, &route_proxy).await?;
        let https = Self::status_for_state(state).await?;

        Ok(RouteProxyHttpsOperationOutcome {
            https,
            route_proxy,
            config_writes,
        })
    }

    pub async fn reimport_root_ca(
        state: &AppState,
    ) -> Result<RouteProxyHttpsOperationOutcome, AppError> {
        Self::reimport_root_ca_with_trust(state, &SystemRouteProxyHttpsTrustExecutor).await
    }

    pub(crate) async fn reimport_root_ca_with_trust(
        state: &AppState,
        trust_executor: &dyn RouteProxyHttpsTrustExecutor,
    ) -> Result<RouteProxyHttpsOperationOutcome, AppError> {
        let material = Self::ensure_material(&state.paths).await?;
        let trust = trust_executor.install(&material).await;
        Self::save_trust_outcome(&state.paths, trust).await?;
        let route_proxy = RouteProxyService::status(&state.route_proxy).await;
        let https =
            Self::status_with_trust(&state.paths, route_proxy.base_url.clone(), trust_executor)
                .await?;

        Ok(RouteProxyHttpsOperationOutcome {
            https,
            route_proxy,
            config_writes: Vec::new(),
        })
    }

    pub async fn uninstall_root_ca(
        state: &AppState,
    ) -> Result<RouteProxyHttpsOperationOutcome, AppError> {
        Self::uninstall_root_ca_with_trust(state, &SystemRouteProxyHttpsTrustExecutor).await
    }

    pub(crate) async fn uninstall_root_ca_with_trust(
        state: &AppState,
        trust_executor: &dyn RouteProxyHttpsTrustExecutor,
    ) -> Result<RouteProxyHttpsOperationOutcome, AppError> {
        let config = Self::load_config(&state.paths).await?;
        let material = Self::ensure_material(&state.paths).await?;
        let previous = RouteProxyService::status(&state.route_proxy).await;
        let was_tls = previous.running && status_uses_https(&previous);

        if was_tls {
            RouteProxyService::stop(&state.route_proxy).await?;
        }

        let trust = match trust_executor.uninstall(&material).await {
            Ok(outcome) => outcome,
            Err(error) => {
                if was_tls {
                    let _ = RouteProxyService::start(
                        &state.route_proxy,
                        state.pool.clone(),
                        Self::tls_transport(&material),
                    )
                    .await;
                }
                // Do not change the saved preference when removal cannot be verified.
                if config.enabled {
                    Self::save_config(&state.paths, &config).await?;
                }
                return Err(error);
            }
        };
        Self::save_trust_outcome(&state.paths, trust).await?;
        Self::save_config(&state.paths, &RouteProxyHttpsConfig { enabled: false }).await?;

        let route_proxy = if previous.running {
            RouteProxyService::start(
                &state.route_proxy,
                state.pool.clone(),
                RouteProxyTransport::Http,
            )
            .await?
        } else {
            RouteProxyService::status(&state.route_proxy).await
        };
        let config_writes = if route_proxy.running {
            Self::rewrite_existing_configs(state, &route_proxy).await?
        } else {
            Vec::new()
        };
        let https =
            Self::status_with_trust(&state.paths, route_proxy.base_url.clone(), trust_executor)
                .await?;

        Ok(RouteProxyHttpsOperationOutcome {
            https,
            route_proxy,
            config_writes,
        })
    }

    pub async fn regenerate_certificates(
        state: &AppState,
    ) -> Result<RouteProxyHttpsOperationOutcome, AppError> {
        Self::regenerate_certificates_with_trust(state, &SystemRouteProxyHttpsTrustExecutor).await
    }

    pub(crate) async fn regenerate_certificates_with_trust(
        state: &AppState,
        trust_executor: &dyn RouteProxyHttpsTrustExecutor,
    ) -> Result<RouteProxyHttpsOperationOutcome, AppError> {
        let config = Self::load_config(&state.paths).await?;
        let old_material = Self::ensure_material(&state.paths).await?;
        let old_trust = trust_executor.inspect(&old_material).await;
        let old_was_trusted = is_trusted(&old_trust.status);
        let previous = RouteProxyService::status(&state.route_proxy).await;
        let was_tls = previous.running && status_uses_https(&previous);

        if was_tls {
            RouteProxyService::stop(&state.route_proxy).await?;
        }
        if old_was_trusted {
            if let Err(error) = trust_executor.uninstall(&old_material).await {
                if was_tls {
                    let _ = RouteProxyService::start(
                        &state.route_proxy,
                        state.pool.clone(),
                        Self::tls_transport(&old_material),
                    )
                    .await;
                }
                return Err(error);
            }
        }

        let (replacement_dir, _) = Self::create_replacement_material(&state.paths).await?;
        let backup_dir = Self::promote_replacement_material(&state.paths, &replacement_dir).await?;
        let replacement =
            Self::load_material(&state.paths)
                .await?
                .ok_or_else(|| AppError::Validation {
                    code: "validation.route_proxy_https_certificate",
                    message: "Replacement local HTTPS certificate material is invalid".to_string(),
                    details: None,
                    recoverable: true,
                })?;
        let new_trust = trust_executor.install(&replacement).await;
        Self::save_trust_outcome(&state.paths, new_trust).await?;

        let route_proxy = if config.enabled {
            match RouteProxyService::start(
                &state.route_proxy,
                state.pool.clone(),
                Self::tls_transport(&replacement),
            )
            .await
            {
                Ok(status) => status,
                Err(error) => {
                    let rollback_error = Self::restore_replacement_after_failure(
                        state,
                        trust_executor,
                        &replacement,
                        &backup_dir,
                        old_was_trusted,
                        was_tls,
                    )
                    .await
                    .err();
                    return Err(with_restoration_details(error, rollback_error));
                }
            }
        } else {
            RouteProxyService::status(&state.route_proxy).await
        };

        let _ = tokio::fs::remove_dir_all(&backup_dir).await;
        let config_writes = if route_proxy.running {
            Self::rewrite_existing_configs(state, &route_proxy).await?
        } else {
            Vec::new()
        };
        let https =
            Self::status_with_trust(&state.paths, route_proxy.base_url.clone(), trust_executor)
                .await?;

        Ok(RouteProxyHttpsOperationOutcome {
            https,
            route_proxy,
            config_writes,
        })
    }

    pub async fn delete_certificates(state: &AppState) -> Result<RouteProxyHttpsStatus, AppError> {
        Self::delete_certificates_with_trust(state, &SystemRouteProxyHttpsTrustExecutor).await
    }

    pub(crate) async fn delete_certificates_with_trust(
        state: &AppState,
        trust_executor: &dyn RouteProxyHttpsTrustExecutor,
    ) -> Result<RouteProxyHttpsStatus, AppError> {
        let route_proxy = RouteProxyService::status(&state.route_proxy).await;
        if route_proxy.running && status_uses_https(&route_proxy) {
            return Err(AppError::Validation {
                code: "validation.route_proxy_https_stop_required",
                message: "Stop the HTTPS route proxy before deleting local certificate material"
                    .to_string(),
                details: None,
                recoverable: true,
            });
        }
        if let Some(material) = Self::load_material(&state.paths).await? {
            let trust = trust_executor.inspect(&material).await;
            if is_trusted(&trust.status) {
                return Err(AppError::Validation {
                    code: "validation.route_proxy_https_uninstall_required",
                    message:
                        "Uninstall the managed Root CA before deleting local certificate material"
                            .to_string(),
                    details: None,
                    recoverable: true,
                });
            }
        }
        Self::delete_material(&state.paths).await?;
        Self::status_with_trust(&state.paths, route_proxy.base_url, trust_executor).await
    }

    fn tls_transport(material: &RouteProxyHttpsMaterial) -> RouteProxyTransport {
        RouteProxyTransport::Https {
            certificate_pem_path: material.server_certificate_pem.clone(),
            private_key_pem_path: material.server_private_key_pem.clone(),
        }
    }

    async fn rewrite_existing_configs(
        state: &AppState,
        route_proxy: &RouteProxyStatus,
    ) -> Result<Vec<ConfigWriteOutcome>, AppError> {
        let Some(base_url) = route_proxy.base_url.as_deref() else {
            return Ok(Vec::new());
        };
        RouteConfigService::write_existing_configs(
            &state.paths,
            &state.pool,
            &state.config_writes,
            base_url,
        )
        .await
    }

    async fn create_replacement_material(
        paths: &AppPaths,
    ) -> Result<(PathBuf, RouteProxyHttpsMaterial), AppError> {
        let certificate_parent =
            paths
                .route_proxy_https_dir
                .parent()
                .ok_or_else(|| AppError::Filesystem {
                    code: "filesystem.route_proxy_https_parent",
                    message: "Local certificate directory has no parent".to_string(),
                    details: None,
                    recoverable: false,
                })?;
        tokio::fs::create_dir_all(certificate_parent).await?;
        let temporary_dir = certificate_parent.join(format!(".route-proxy-{}.tmp", Uuid::new_v4()));
        tokio::fs::create_dir(&temporary_dir).await?;
        let generated = match generate_certificate_files(&temporary_dir).await {
            Ok(generated) => generated,
            Err(error) => {
                let _ = tokio::fs::remove_dir_all(&temporary_dir).await;
                return Err(error);
            }
        };

        Ok((
            temporary_dir.clone(),
            RouteProxyHttpsMaterial {
                root_certificate_pem: temporary_dir.join(ROOT_CERTIFICATE_FILE),
                root_fingerprint_sha256: generated.root_fingerprint_sha256,
                root_thumbprint_sha1: generated.root_thumbprint_sha1,
                server_certificate_pem: temporary_dir.join(SERVER_CERTIFICATE_FILE),
                server_private_key_pem: temporary_dir.join(SERVER_PRIVATE_KEY_FILE),
                expires_at: generated.expires_at,
            },
        ))
    }

    async fn promote_replacement_material(
        paths: &AppPaths,
        replacement_dir: &Path,
    ) -> Result<PathBuf, AppError> {
        let target = &paths.route_proxy_https_dir;
        let target_metadata = tokio::fs::symlink_metadata(target).await?;
        if target_metadata.file_type().is_symlink() {
            return Err(AppError::Validation {
                code: "validation.route_proxy_https_symlink",
                message: "Refusing to replace a linked certificate directory".to_string(),
                details: Some(target.display().to_string()),
                recoverable: false,
            });
        }
        let certificate_parent = target.parent().ok_or_else(|| AppError::Filesystem {
            code: "filesystem.route_proxy_https_parent",
            message: "Local certificate directory has no parent".to_string(),
            details: None,
            recoverable: false,
        })?;
        let backup_dir = certificate_parent.join(format!(".route-proxy-{}.backup", Uuid::new_v4()));
        tokio::fs::rename(target, &backup_dir).await?;
        if let Err(error) = tokio::fs::rename(replacement_dir, target).await {
            let _ = tokio::fs::rename(&backup_dir, target).await;
            return Err(error.into());
        }
        Ok(backup_dir)
    }

    async fn restore_replacement_after_failure(
        state: &AppState,
        trust_executor: &dyn RouteProxyHttpsTrustExecutor,
        replacement: &RouteProxyHttpsMaterial,
        backup_dir: &Path,
        old_was_trusted: bool,
        restart_old_tls: bool,
    ) -> Result<(), AppError> {
        let mut restoration_errors = Vec::new();
        if let Err(error) = trust_executor.uninstall(replacement).await {
            restoration_errors.push(error.to_string());
        }
        if let Err(error) = tokio::fs::remove_dir_all(&state.paths.route_proxy_https_dir).await {
            restoration_errors.push(error.to_string());
        }
        if let Err(error) = tokio::fs::rename(backup_dir, &state.paths.route_proxy_https_dir).await
        {
            restoration_errors.push(error.to_string());
        }

        // The prior paths point at the managed directory. Reload them only
        // after the old directory is restored, so we never trust or serve the
        // replacement Root under the old material identity.
        let old_material = match Self::load_material(&state.paths).await {
            Ok(Some(material)) => Some(material),
            Ok(None) => {
                restoration_errors.push(
                    "Could not reload the prior local HTTPS certificate material".to_string(),
                );
                None
            }
            Err(error) => {
                restoration_errors.push(error.to_string());
                None
            }
        };
        if old_was_trusted {
            if let Some(material) = old_material.as_ref() {
                let trust = trust_executor.install(material).await;
                if !is_trusted(&trust.status) {
                    restoration_errors.push(
                        trust
                            .message
                            .unwrap_or_else(|| "Could not restore Root CA trust".to_string()),
                    );
                }
            }
        }
        if restart_old_tls {
            if let Some(material) = old_material.as_ref() {
                if let Err(error) = RouteProxyService::start(
                    &state.route_proxy,
                    state.pool.clone(),
                    Self::tls_transport(material),
                )
                .await
                {
                    restoration_errors.push(error.to_string());
                }
            }
        }

        if restoration_errors.is_empty() {
            Ok(())
        } else {
            Err(AppError::Validation {
                code: "validation.route_proxy_https_restore",
                message: "Could not fully restore the prior local HTTPS configuration".to_string(),
                details: Some(restoration_errors.join(" | ")),
                recoverable: true,
            })
        }
    }

    pub async fn status(
        paths: &AppPaths,
        proxy_base_url: Option<String>,
    ) -> Result<RouteProxyHttpsStatus, AppError> {
        Self::status_with_trust(paths, proxy_base_url, &SystemRouteProxyHttpsTrustExecutor).await
    }

    pub(crate) async fn status_with_trust(
        paths: &AppPaths,
        proxy_base_url: Option<String>,
        trust_executor: &dyn RouteProxyHttpsTrustExecutor,
    ) -> Result<RouteProxyHttpsStatus, AppError> {
        let config = Self::load_config(paths).await?;
        let material = Self::load_material(paths).await?;
        let metadata = Self::load_metadata(paths).await?;
        let trust = if let Some(material) = material.as_ref() {
            // Inspection is deliberately best-effort. A stale trust record is less accurate than
            // reporting an unknown state when the local trust store cannot be queried safely.
            let inspected = trust_executor.inspect(material).await;
            let trust = inspected.into_record();
            let _ = Self::save_trust_record(paths, trust.clone()).await;
            trust
        } else {
            metadata
                .as_ref()
                .map(|value| value.trust.clone())
                .unwrap_or_else(|| RouteProxyTrustRecord {
                    status: RouteProxyTrustStatus::Unknown,
                    ..RouteProxyTrustRecord::default()
                })
        };

        Ok(RouteProxyHttpsStatus {
            enabled: config.enabled,
            cert_ready: material.is_some(),
            trust_status: trust.status,
            trust_adapter: trust.adapter,
            root_fingerprint: material
                .as_ref()
                .map(|value| value.root_fingerprint_sha256.clone()),
            expires_at: material.as_ref().map(|value| value.expires_at.clone()),
            certificate_dir: paths.route_proxy_https_dir.display().to_string(),
            root_certificate_path: material
                .as_ref()
                .map(|value| value.root_certificate_pem.display().to_string()),
            proxy_base_url,
            message: trust.message,
            manual_instructions: trust.manual_instructions,
        })
    }

    pub async fn delete_material(paths: &AppPaths) -> Result<(), AppError> {
        let config = Self::load_config(paths).await?;
        if config.enabled {
            return Err(AppError::Validation {
                code: "validation.route_proxy_https_disable_required",
                message: "Disable HTTPS before deleting local certificate material".to_string(),
                details: None,
                recoverable: true,
            });
        }

        if !paths.route_proxy_https_dir.exists() {
            return Ok(());
        }

        let certificate_parent =
            paths
                .route_proxy_https_dir
                .parent()
                .ok_or_else(|| AppError::Filesystem {
                    code: "filesystem.route_proxy_https_parent",
                    message: "Local certificate directory has no parent".to_string(),
                    details: None,
                    recoverable: false,
                })?;
        let target_metadata = tokio::fs::symlink_metadata(&paths.route_proxy_https_dir).await?;
        if target_metadata.file_type().is_symlink() {
            return Err(AppError::Validation {
                code: "validation.route_proxy_https_symlink",
                message: "Refusing to delete a linked certificate directory".to_string(),
                details: Some(paths.route_proxy_https_dir.display().to_string()),
                recoverable: false,
            });
        }

        let canonical_parent = tokio::fs::canonicalize(certificate_parent).await?;
        let canonical_target = tokio::fs::canonicalize(&paths.route_proxy_https_dir).await?;
        let expected_target = canonical_parent.join("route-proxy");
        if canonical_target != expected_target {
            return Err(AppError::Validation {
                code: "validation.route_proxy_https_delete_target",
                message: "Refusing to delete certificate material outside its managed directory"
                    .to_string(),
                details: Some(canonical_target.display().to_string()),
                recoverable: false,
            });
        }

        tokio::fs::remove_dir_all(&paths.route_proxy_https_dir).await?;
        Ok(())
    }

    pub(crate) async fn load_metadata(
        paths: &AppPaths,
    ) -> Result<Option<RouteProxyHttpsMetadata>, AppError> {
        let metadata_path = paths.route_proxy_https_dir.join(METADATA_FILE);
        if !metadata_path.exists() {
            return Ok(None);
        }

        let contents = tokio::fs::read_to_string(&metadata_path).await?;
        let metadata = serde_json::from_str(&contents).map_err(|error| AppError::Validation {
            code: "validation.route_proxy_https_metadata",
            message: "Local route proxy HTTPS certificate metadata is invalid".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;
        Ok(Some(metadata))
    }

    pub(crate) async fn save_trust_outcome(
        paths: &AppPaths,
        outcome: RouteProxyTrustOutcome,
    ) -> Result<(), AppError> {
        Self::save_trust_record(paths, outcome.into_record()).await
    }

    async fn save_trust_record(
        paths: &AppPaths,
        trust: RouteProxyTrustRecord,
    ) -> Result<(), AppError> {
        let Some(mut metadata) = Self::load_metadata(paths).await? else {
            return Ok(());
        };
        metadata.trust = trust;
        let contents = serde_json::to_string_pretty(&metadata)?;
        ConfigWriter::write_atomic(&paths.route_proxy_https_dir.join(METADATA_FILE), &contents)
            .await
            .map(|_| ())
    }

    async fn load_material(paths: &AppPaths) -> Result<Option<RouteProxyHttpsMaterial>, AppError> {
        let root_certificate_pem = paths.route_proxy_https_dir.join(ROOT_CERTIFICATE_FILE);
        let root_private_key_pem = paths.route_proxy_https_dir.join(ROOT_PRIVATE_KEY_FILE);
        let server_certificate_pem = paths.route_proxy_https_dir.join(SERVER_CERTIFICATE_FILE);
        let server_private_key_pem = paths.route_proxy_https_dir.join(SERVER_PRIVATE_KEY_FILE);

        if !root_certificate_pem.exists()
            || !root_private_key_pem.exists()
            || !server_certificate_pem.exists()
            || !server_private_key_pem.exists()
        {
            return Ok(None);
        }

        let Some(metadata) = Self::load_metadata(paths).await? else {
            return Ok(None);
        };
        if is_expired(&metadata.expires_at)? {
            return Ok(None);
        }

        let root_pem = tokio::fs::read(&root_certificate_pem).await?;
        let leaf_pem = tokio::fs::read(&server_certificate_pem).await?;
        let root_der = parse_certificate_der(&root_pem)?;
        let root_fingerprint_sha256 = hex_sha256(&root_der);
        let root_thumbprint_sha1 = hex_sha1(&root_der);
        if metadata.root_fingerprint_sha256 != root_fingerprint_sha256
            || metadata.root_thumbprint_sha1 != root_thumbprint_sha1
            || !leaf_pem_contains_required_sans(&leaf_pem)?
        {
            return Ok(None);
        }

        Ok(Some(RouteProxyHttpsMaterial {
            root_certificate_pem,
            root_fingerprint_sha256,
            root_thumbprint_sha1,
            server_certificate_pem,
            server_private_key_pem,
            expires_at: metadata.expires_at,
        }))
    }

    async fn generate_material(paths: &AppPaths) -> Result<RouteProxyHttpsMaterial, AppError> {
        let certificate_parent =
            paths
                .route_proxy_https_dir
                .parent()
                .ok_or_else(|| AppError::Filesystem {
                    code: "filesystem.route_proxy_https_parent",
                    message: "Local certificate directory has no parent".to_string(),
                    details: None,
                    recoverable: false,
                })?;
        tokio::fs::create_dir_all(certificate_parent).await?;

        let temporary_dir = certificate_parent.join(format!(".route-proxy-{}.tmp", Uuid::new_v4()));
        tokio::fs::create_dir(&temporary_dir).await?;
        let generation = generate_certificate_files(&temporary_dir).await;
        let generated = match generation {
            Ok(value) => value,
            Err(error) => {
                let _ = tokio::fs::remove_dir_all(&temporary_dir).await;
                return Err(error);
            }
        };

        let target = &paths.route_proxy_https_dir;
        let backup_dir = certificate_parent.join(format!(".route-proxy-{}.backup", Uuid::new_v4()));
        let replaced_existing = target.exists();
        if replaced_existing {
            let metadata = tokio::fs::symlink_metadata(target).await?;
            if metadata.file_type().is_symlink() {
                let _ = tokio::fs::remove_dir_all(&temporary_dir).await;
                return Err(AppError::Validation {
                    code: "validation.route_proxy_https_symlink",
                    message: "Refusing to replace a linked certificate directory".to_string(),
                    details: Some(target.display().to_string()),
                    recoverable: false,
                });
            }
            tokio::fs::rename(target, &backup_dir).await?;
        }

        if let Err(error) = tokio::fs::rename(&temporary_dir, target).await {
            if replaced_existing {
                let _ = tokio::fs::rename(&backup_dir, target).await;
            }
            let _ = tokio::fs::remove_dir_all(&temporary_dir).await;
            return Err(error.into());
        }

        if replaced_existing {
            let _ = tokio::fs::remove_dir_all(&backup_dir).await;
        }

        Ok(RouteProxyHttpsMaterial {
            root_certificate_pem: target.join(ROOT_CERTIFICATE_FILE),
            root_fingerprint_sha256: generated.root_fingerprint_sha256,
            root_thumbprint_sha1: generated.root_thumbprint_sha1,
            server_certificate_pem: target.join(SERVER_CERTIFICATE_FILE),
            server_private_key_pem: target.join(SERVER_PRIVATE_KEY_FILE),
            expires_at: generated.expires_at,
        })
    }
}

struct GeneratedMaterial {
    root_fingerprint_sha256: String,
    root_thumbprint_sha1: String,
    expires_at: String,
}

async fn generate_certificate_files(directory: &Path) -> Result<GeneratedMaterial, AppError> {
    let now = OffsetDateTime::now_utc();
    let mut root_params = CertificateParams::default();
    root_params.not_before = now - Duration::days(1);
    root_params.not_after = now + Duration::days(ROOT_VALIDITY_DAYS);
    root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    root_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    root_params
        .distinguished_name
        .push(DnType::CommonName, ROOT_COMMON_NAME);

    let mut leaf_params =
        CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()])
            .map_err(certificate_generation_error)?;
    leaf_params.not_before = now - Duration::days(1);
    leaf_params.not_after = now + Duration::days(LEAF_VALIDITY_DAYS);
    leaf_params
        .distinguished_name
        .push(DnType::CommonName, SERVER_COMMON_NAME);
    leaf_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let leaf_expires_at = leaf_params.not_after;

    let root_key = KeyPair::generate().map_err(certificate_generation_error)?;
    let root_certificate = root_params
        .self_signed(&root_key)
        .map_err(certificate_generation_error)?;
    let leaf_key = KeyPair::generate().map_err(certificate_generation_error)?;
    let leaf_certificate = leaf_params
        .signed_by(&leaf_key, &root_certificate, &root_key)
        .map_err(certificate_generation_error)?;
    let root_der = root_certificate.der();
    let expires_at = leaf_expires_at
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| AppError::Validation {
            code: "validation.route_proxy_https_expiry",
            message: "Could not format local HTTPS certificate expiry".to_string(),
            details: Some(error.to_string()),
            recoverable: false,
        })?;
    let metadata = RouteProxyHttpsMetadata {
        root_fingerprint_sha256: hex_sha256(root_der.as_ref()),
        root_thumbprint_sha1: hex_sha1(root_der.as_ref()),
        expires_at: expires_at.clone(),
        trust: RouteProxyTrustRecord::default(),
    };

    write_file(
        directory.join(ROOT_CERTIFICATE_FILE),
        root_certificate.pem().as_bytes(),
        false,
    )
    .await?;
    write_file(
        directory.join(ROOT_PRIVATE_KEY_FILE),
        root_key.serialize_pem().as_bytes(),
        true,
    )
    .await?;
    write_file(
        directory.join(SERVER_CERTIFICATE_FILE),
        leaf_certificate.pem().as_bytes(),
        false,
    )
    .await?;
    write_file(
        directory.join(SERVER_PRIVATE_KEY_FILE),
        leaf_key.serialize_pem().as_bytes(),
        true,
    )
    .await?;
    let metadata_json = serde_json::to_vec_pretty(&metadata)?;
    write_file(directory.join(METADATA_FILE), &metadata_json, false).await?;

    Ok(GeneratedMaterial {
        root_fingerprint_sha256: metadata.root_fingerprint_sha256,
        root_thumbprint_sha1: metadata.root_thumbprint_sha1,
        expires_at,
    })
}

async fn write_file(path: PathBuf, contents: &[u8], private: bool) -> Result<(), AppError> {
    let mut file = tokio::fs::File::create(&path).await?;
    file.write_all(contents).await?;
    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    #[cfg(unix)]
    if private {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    }

    #[cfg(not(unix))]
    let _ = private;

    Ok(())
}

fn parse_certificate_der(pem: &[u8]) -> Result<Vec<u8>, AppError> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(pem).map_err(|error| AppError::Validation {
        code: "validation.route_proxy_https_certificate",
        message: "Local route proxy HTTPS certificate is invalid".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;
    Ok(pem.contents)
}

fn leaf_pem_contains_required_sans(pem: &[u8]) -> Result<bool, AppError> {
    let der = parse_certificate_der(pem)?;
    let (_, certificate) =
        x509_parser::parse_x509_certificate(&der).map_err(|error| AppError::Validation {
            code: "validation.route_proxy_https_certificate",
            message: "Local route proxy HTTPS certificate is invalid".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;
    let san = certificate
        .subject_alternative_name()
        .map_err(|error| AppError::Validation {
            code: "validation.route_proxy_https_certificate",
            message: "Local route proxy HTTPS certificate is invalid".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;
    let Some(san) = san else {
        return Ok(false);
    };
    let has_localhost = san
        .value
        .general_names
        .iter()
        .any(|name| matches!(name, GeneralName::DNSName(value) if *value == "localhost"));
    let has_loopback = san
        .value
        .general_names
        .iter()
        .any(|name| matches!(name, GeneralName::IPAddress(value) if *value == [127, 0, 0, 1]));
    Ok(has_localhost && has_loopback)
}

fn is_expired(expires_at: &str) -> Result<bool, AppError> {
    let expires_at =
        OffsetDateTime::parse(expires_at, &time::format_description::well_known::Rfc3339).map_err(
            |error| AppError::Validation {
                code: "validation.route_proxy_https_metadata",
                message: "Local route proxy HTTPS certificate expiry is invalid".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            },
        )?;
    Ok(expires_at <= OffsetDateTime::now_utc())
}

fn status_uses_https(status: &RouteProxyStatus) -> bool {
    status
        .base_url
        .as_deref()
        .is_some_and(|base_url| base_url.starts_with("https://"))
}

fn is_trusted(status: &RouteProxyTrustStatus) -> bool {
    matches!(
        status,
        RouteProxyTrustStatus::SystemTrusted
            | RouteProxyTrustStatus::NssTrusted
            | RouteProxyTrustStatus::PartiallyTrusted
    )
}

fn with_restoration_details(primary: AppError, restoration: Option<AppError>) -> AppError {
    let Some(restoration) = restoration else {
        return primary;
    };
    match primary {
        AppError::Validation {
            code,
            message,
            details,
            recoverable,
        } => AppError::Validation {
            code,
            message,
            details: Some(format!(
                "{} | restoration: {}",
                details.unwrap_or_default(),
                restoration
            )),
            recoverable,
        },
        AppError::Filesystem {
            code,
            message,
            details,
            recoverable,
        } => AppError::Filesystem {
            code,
            message,
            details: Some(format!(
                "{} | restoration: {}",
                details.unwrap_or_default(),
                restoration
            )),
            recoverable,
        },
        AppError::Database {
            code,
            message,
            details,
            recoverable,
        } => AppError::Database {
            code,
            message,
            details: Some(format!(
                "{} | restoration: {}",
                details.unwrap_or_default(),
                restoration
            )),
            recoverable,
        },
        AppError::Secret {
            code,
            message,
            details,
            recoverable,
        } => AppError::Secret {
            code,
            message,
            details: Some(format!(
                "{} | restoration: {}",
                details.unwrap_or_default(),
                restoration
            )),
            recoverable,
        },
    }
}

fn certificate_generation_error(error: rcgen::Error) -> AppError {
    AppError::Validation {
        code: "validation.route_proxy_https_certificate",
        message: "Could not generate local route proxy HTTPS certificates".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    Sha256Digest::update(&mut hasher, bytes);
    format!("{:x}", Sha256Digest::finalize(hasher))
}

fn hex_sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    Sha1Digest::update(&mut hasher, bytes);
    format!("{:x}", Sha1Digest::finalize(hasher))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::services::config_write_service::ConfigWriteRuntimeState;
    use crate::services::route_proxy_service::RouteProxyRuntimeState;
    use crate::services::tailscale_service::TailscaleRuntimeState;
    use crate::services::web_service::WebServiceRuntimeState;
    use crate::terminal_manager::TerminalManager;
    use crate::web::event_bridge::WebEventBroadcaster;
    use async_trait::async_trait;
    use std::sync::Arc;
    use tempfile::tempdir;

    struct FakeTrustExecutor {
        outcome: RouteProxyTrustOutcome,
        uninstall_error: Option<String>,
    }

    #[async_trait]
    impl RouteProxyHttpsTrustExecutor for FakeTrustExecutor {
        async fn install(&self, _material: &RouteProxyHttpsMaterial) -> RouteProxyTrustOutcome {
            self.outcome.clone()
        }

        async fn uninstall(
            &self,
            _material: &RouteProxyHttpsMaterial,
        ) -> Result<RouteProxyTrustOutcome, AppError> {
            if let Some(message) = &self.uninstall_error {
                return Err(AppError::Validation {
                    code: "validation.fake_route_proxy_https_uninstall",
                    message: message.clone(),
                    details: None,
                    recoverable: true,
                });
            }
            Ok(self.outcome.clone())
        }

        async fn inspect(&self, _material: &RouteProxyHttpsMaterial) -> RouteProxyTrustOutcome {
            self.outcome.clone()
        }
    }

    fn trusted_outcome() -> RouteProxyTrustOutcome {
        RouteProxyTrustOutcome {
            status: RouteProxyTrustStatus::SystemTrusted,
            adapter: Some("fake-system-store".to_string()),
            message: Some("Managed Root CA is installed in the fake trust store".to_string()),
            manual_instructions: Vec::new(),
        }
    }

    fn fake_trust() -> FakeTrustExecutor {
        FakeTrustExecutor {
            outcome: trusted_outcome(),
            uninstall_error: None,
        }
    }

    struct TestState {
        _temp: tempfile::TempDir,
        state: AppState,
    }

    async fn test_state() -> TestState {
        let temp = tempdir().expect("temp dir");
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        TestState {
            state: AppState {
                paths: AppPaths::from_data_dir(temp.path().join("app-data")),
                pool,
                config_writes: ConfigWriteRuntimeState::default(),
                deeplink_protocols: crate::services::deeplink_protocol_service::DeepLinkProtocolRuntime::default(),
                route_proxy: RouteProxyRuntimeState::default(),
                web_service: WebServiceRuntimeState::default(),
                tailscale: TailscaleRuntimeState::default(),
                terminals: TerminalManager::default(),
                event_broadcaster: Arc::new(WebEventBroadcaster::default()),
            },
            _temp: temp,
        }
    }

    #[tokio::test]
    async fn ensure_material_generates_root_and_loopback_leaf_without_exposing_private_key() {
        let temp = tempdir().expect("temp dir");
        let paths = AppPaths::from_data_dir(temp.path().to_path_buf());

        let material = RouteProxyHttpsService::ensure_material(&paths)
            .await
            .expect("certificate material");
        let leaf = tokio::fs::read(&material.server_certificate_pem)
            .await
            .expect("leaf pem");
        let (_, pem) = x509_parser::pem::parse_x509_pem(&leaf).expect("pem");
        let (_, certificate) =
            x509_parser::parse_x509_certificate(&pem.contents).expect("x509 certificate");
        let san = certificate
            .subject_alternative_name()
            .expect("san extension")
            .expect("san value");

        assert!(material.root_certificate_pem.exists());
        assert!(material.server_certificate_pem.exists());
        assert!(material.server_private_key_pem.exists());
        assert_eq!(material.root_fingerprint_sha256.len(), 64);
        assert_eq!(material.root_thumbprint_sha1.len(), 40);
        assert!(san
            .value
            .general_names
            .iter()
            .any(|name| matches!(name, GeneralName::DNSName(value) if *value == "localhost")));
        assert!(san
            .value
            .general_names
            .iter()
            .any(|name| matches!(name, GeneralName::IPAddress(value) if *value == [127, 0, 0, 1])));

        let status = RouteProxyHttpsService::status_with_trust(
            &paths,
            None,
            &FakeTrustExecutor {
                outcome: trusted_outcome(),
                uninstall_error: None,
            },
        )
        .await
        .expect("status");
        let status_json = serde_json::to_string(&status).expect("status json");
        assert!(status.cert_ready);
        assert!(status.root_fingerprint.is_some());
        assert!(status_json.contains("rootFingerprint"));
        assert!(!status_json.contains("PRIVATE KEY"));
    }

    #[tokio::test]
    async fn status_persists_best_effort_trust_inspection_without_exposing_private_key() {
        let temp = tempdir().expect("temp dir");
        let paths = AppPaths::from_data_dir(temp.path().to_path_buf());
        RouteProxyHttpsService::ensure_material(&paths)
            .await
            .expect("certificate material");

        let status = RouteProxyHttpsService::status_with_trust(
            &paths,
            None,
            &FakeTrustExecutor {
                outcome: trusted_outcome(),
                uninstall_error: None,
            },
        )
        .await
        .expect("status");
        let metadata = RouteProxyHttpsService::load_metadata(&paths)
            .await
            .expect("metadata")
            .expect("metadata exists");

        assert_eq!(status.trust_status, RouteProxyTrustStatus::SystemTrusted);
        assert_eq!(metadata.trust.status, RouteProxyTrustStatus::SystemTrusted);
        assert_eq!(metadata.trust.adapter.as_deref(), Some("fake-system-store"));
        let metadata_contents =
            tokio::fs::read_to_string(paths.route_proxy_https_dir.join(METADATA_FILE))
                .await
                .expect("metadata contents");
        assert!(!metadata_contents.contains("PRIVATE KEY"));
    }

    #[tokio::test]
    async fn delete_material_rejects_enabled_https_and_removes_only_the_managed_directory() {
        let temp = tempdir().expect("temp dir");
        let paths = AppPaths::from_data_dir(temp.path().to_path_buf());
        RouteProxyHttpsService::ensure_material(&paths)
            .await
            .expect("material");
        RouteProxyHttpsService::save_config(&paths, &RouteProxyHttpsConfig { enabled: true })
            .await
            .expect("save");

        let error = RouteProxyHttpsService::delete_material(&paths)
            .await
            .expect_err("enabled error");
        assert!(error.to_string().contains("Disable HTTPS"));

        RouteProxyHttpsService::save_config(&paths, &RouteProxyHttpsConfig { enabled: false })
            .await
            .expect("save disabled");
        RouteProxyHttpsService::delete_material(&paths)
            .await
            .expect("delete material");
        assert!(!paths.route_proxy_https_dir.exists());
    }

    #[tokio::test]
    async fn enable_https_restarts_a_running_http_proxy_and_rewrites_only_existing_platforms() {
        let fixture = test_state().await;
        let initial = RouteProxyService::start(
            &fixture.state.route_proxy,
            fixture.state.pool.clone(),
            RouteProxyTransport::Http,
        )
        .await
        .expect("start HTTP proxy");
        assert!(initial
            .base_url
            .as_deref()
            .is_some_and(|url| url.starts_with("http://")));

        let outcome = RouteProxyHttpsService::enable_with_trust(&fixture.state, &fake_trust())
            .await
            .expect("enable HTTPS");

        assert!(outcome.route_proxy.running);
        assert!(outcome
            .route_proxy
            .base_url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://")));
        assert!(outcome.https.enabled);
        assert!(outcome.config_writes.is_empty());
        assert!(
            RouteProxyHttpsService::load_config(&fixture.state.paths)
                .await
                .expect("HTTPS config")
                .enabled
        );

        RouteProxyService::stop(&fixture.state.route_proxy)
            .await
            .expect("stop HTTPS proxy");
    }

    #[tokio::test]
    async fn failed_root_uninstall_restarts_tls_and_keeps_https_enabled() {
        let fixture = test_state().await;
        RouteProxyHttpsService::enable_with_trust(&fixture.state, &fake_trust())
            .await
            .expect("enable HTTPS");
        let failing_trust = FakeTrustExecutor {
            outcome: trusted_outcome(),
            uninstall_error: Some("fake Root CA removal failure".to_string()),
        };

        let error =
            RouteProxyHttpsService::uninstall_root_ca_with_trust(&fixture.state, &failing_trust)
                .await
                .expect_err("root uninstall failure");
        assert!(error.to_string().contains("fake Root CA removal failure"));

        let status = RouteProxyService::status(&fixture.state.route_proxy).await;
        assert!(status.running);
        assert!(status
            .base_url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://")));
        assert!(
            RouteProxyHttpsService::load_config(&fixture.state.paths)
                .await
                .expect("HTTPS config")
                .enabled
        );

        RouteProxyService::stop(&fixture.state.route_proxy)
            .await
            .expect("stop HTTPS proxy");
    }
}
