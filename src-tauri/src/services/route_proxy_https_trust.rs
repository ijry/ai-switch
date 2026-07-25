use crate::error::AppError;
use crate::models::route_proxy_https::{RouteProxyTrustRecord, RouteProxyTrustStatus};
use crate::services::route_proxy_https_service::RouteProxyHttpsMaterial;
use async_trait::async_trait;
use sha1::{Digest, Sha1};
use std::env;
use std::path::{Path, PathBuf};

const ROOT_COMMON_NAME: &str = "AI Switch Route Proxy Root CA";
const NSS_NICKNAME: &str = "AI Switch Route Proxy Root CA";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteProxyTrustOutcome {
    pub status: RouteProxyTrustStatus,
    pub adapter: Option<String>,
    pub message: Option<String>,
    pub manual_instructions: Vec<String>,
}

impl RouteProxyTrustOutcome {
    pub fn into_record(self) -> RouteProxyTrustRecord {
        RouteProxyTrustRecord {
            status: self.status,
            adapter: self.adapter,
            message: self.message,
            manual_instructions: self.manual_instructions,
        }
    }

    fn untrusted(material: &RouteProxyHttpsMaterial, message: impl Into<String>) -> Self {
        Self {
            status: RouteProxyTrustStatus::Untrusted,
            adapter: None,
            message: Some(message.into()),
            manual_instructions: manual_instructions(material),
        }
    }

    fn unknown(material: &RouteProxyHttpsMaterial, message: impl Into<String>) -> Self {
        Self {
            status: RouteProxyTrustStatus::Unknown,
            adapter: None,
            message: Some(message.into()),
            manual_instructions: manual_instructions(material),
        }
    }
}

#[async_trait]
pub trait RouteProxyHttpsTrustExecutor: Send + Sync {
    async fn install(&self, material: &RouteProxyHttpsMaterial) -> RouteProxyTrustOutcome;
    async fn uninstall(
        &self,
        material: &RouteProxyHttpsMaterial,
    ) -> Result<RouteProxyTrustOutcome, AppError>;
    async fn inspect(&self, material: &RouteProxyHttpsMaterial) -> RouteProxyTrustOutcome;
}

pub struct SystemRouteProxyHttpsTrustExecutor;

#[async_trait]
impl RouteProxyHttpsTrustExecutor for SystemRouteProxyHttpsTrustExecutor {
    async fn install(&self, material: &RouteProxyHttpsMaterial) -> RouteProxyTrustOutcome {
        let attempts = install_current_platform(material).await;
        let inspected = self.inspect(material).await;
        merge_install_outcome(material, inspected, attempts)
    }

    async fn uninstall(
        &self,
        material: &RouteProxyHttpsMaterial,
    ) -> Result<RouteProxyTrustOutcome, AppError> {
        let existing = self.inspect(material).await;
        match existing.status {
            RouteProxyTrustStatus::Untrusted => return Ok(existing),
            RouteProxyTrustStatus::Unknown => {
                return Err(AppError::Validation {
                    code: "validation.route_proxy_https_trust_uninstall",
                    message:
                        "Could not safely determine whether the managed HTTPS Root CA is installed"
                            .to_string(),
                    details: existing.message,
                    recoverable: true,
                });
            }
            RouteProxyTrustStatus::SystemTrusted
            | RouteProxyTrustStatus::NssTrusted
            | RouteProxyTrustStatus::PartiallyTrusted => {}
        }

        let attempts = uninstall_current_platform(material, existing.adapter.as_deref()).await;
        if attempts.is_empty() {
            return Err(AppError::Validation {
                code: "validation.route_proxy_https_trust_uninstall",
                message: "Could not identify a managed HTTPS Root CA trust store to remove"
                    .to_string(),
                details: existing.adapter,
                recoverable: true,
            });
        }
        let failures = attempts
            .iter()
            .filter_map(|attempt| attempt.result.as_ref().err())
            .cloned()
            .collect::<Vec<_>>();
        if !failures.is_empty() {
            return Err(AppError::Validation {
                code: "validation.route_proxy_https_trust_uninstall",
                message: "Could not remove the managed HTTPS Root CA from every trust store"
                    .to_string(),
                details: Some(failures.join(" | ")),
                recoverable: true,
            });
        }

        Ok(self.inspect(material).await)
    }

    async fn inspect(&self, material: &RouteProxyHttpsMaterial) -> RouteProxyTrustOutcome {
        if cfg!(target_os = "windows") {
            return inspect_windows(material).await;
        }
        if cfg!(target_os = "macos") {
            return inspect_macos(material).await;
        }
        if cfg!(target_os = "linux") {
            return inspect_linux(material).await;
        }

        RouteProxyTrustOutcome::unknown(
            material,
            "HTTPS Root CA trust inspection is not supported on this operating system",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrustPlatform {
    WindowsCurrentUser,
    MacOsLoginKeychain,
    LinuxP11Kit,
    LinuxDebian,
    LinuxRhel,
    LinuxNss,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrustCommand {
    program: String,
    args: Vec<String>,
}

#[derive(Debug)]
struct TrustAttempt {
    adapter: &'static str,
    kind: TrustAdapterKind,
    result: Result<(), String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrustAdapterKind {
    System,
    Nss,
}

fn install_commands(
    platform: TrustPlatform,
    material: &RouteProxyHttpsMaterial,
    escalation: Option<&str>,
) -> Vec<TrustCommand> {
    let root_path = material.root_certificate_pem.display().to_string();
    let command = match platform {
        TrustPlatform::WindowsCurrentUser => vec![TrustCommand {
            program: "certutil.exe".to_string(),
            args: vec![
                "-user".to_string(),
                "-addstore".to_string(),
                "Root".to_string(),
                root_path,
            ],
        }],
        TrustPlatform::MacOsLoginKeychain => vec![TrustCommand {
            program: "security".to_string(),
            args: vec![
                "add-trusted-cert".to_string(),
                "-d".to_string(),
                "-r".to_string(),
                "trustRoot".to_string(),
                "-k".to_string(),
                login_keychain_path().display().to_string(),
                root_path,
            ],
        }],
        TrustPlatform::LinuxP11Kit => vec![TrustCommand {
            program: "trust".to_string(),
            args: vec!["anchor".to_string(), root_path],
        }],
        TrustPlatform::LinuxDebian => with_escalation(
            vec![
                TrustCommand {
                    program: "install".to_string(),
                    args: vec![
                        "-Dm644".to_string(),
                        root_path,
                        linux_anchor_path(material, TrustPlatform::LinuxDebian)
                            .display()
                            .to_string(),
                    ],
                },
                TrustCommand {
                    program: "update-ca-certificates".to_string(),
                    args: Vec::new(),
                },
            ],
            escalation,
        ),
        TrustPlatform::LinuxRhel => with_escalation(
            vec![
                TrustCommand {
                    program: "install".to_string(),
                    args: vec![
                        "-Dm644".to_string(),
                        root_path,
                        linux_anchor_path(material, TrustPlatform::LinuxRhel)
                            .display()
                            .to_string(),
                    ],
                },
                TrustCommand {
                    program: "update-ca-trust".to_string(),
                    args: vec!["extract".to_string()],
                },
            ],
            escalation,
        ),
        TrustPlatform::LinuxNss => vec![TrustCommand {
            program: "certutil".to_string(),
            args: vec![
                "-A".to_string(),
                "-d".to_string(),
                nss_database_spec(),
                "-n".to_string(),
                NSS_NICKNAME.to_string(),
                "-t".to_string(),
                "C,,".to_string(),
                "-i".to_string(),
                root_path,
            ],
        }],
    };
    command
}

fn uninstall_commands(
    platform: TrustPlatform,
    material: &RouteProxyHttpsMaterial,
    escalation: Option<&str>,
) -> Vec<TrustCommand> {
    let command = match platform {
        TrustPlatform::WindowsCurrentUser => vec![TrustCommand {
            program: "certutil.exe".to_string(),
            args: vec![
                "-user".to_string(),
                "-delstore".to_string(),
                "Root".to_string(),
                material.root_thumbprint_sha1.clone(),
            ],
        }],
        TrustPlatform::MacOsLoginKeychain => vec![TrustCommand {
            program: "security".to_string(),
            args: vec![
                "delete-certificate".to_string(),
                "-Z".to_string(),
                material.root_thumbprint_sha1.clone(),
                login_keychain_path().display().to_string(),
            ],
        }],
        TrustPlatform::LinuxP11Kit => vec![TrustCommand {
            program: "trust".to_string(),
            args: vec![
                "anchor".to_string(),
                "--remove".to_string(),
                material.root_certificate_pem.display().to_string(),
            ],
        }],
        TrustPlatform::LinuxDebian => with_escalation(
            vec![
                TrustCommand {
                    program: "rm".to_string(),
                    args: vec![
                        "-f".to_string(),
                        linux_anchor_path(material, TrustPlatform::LinuxDebian)
                            .display()
                            .to_string(),
                    ],
                },
                TrustCommand {
                    program: "update-ca-certificates".to_string(),
                    args: vec!["--fresh".to_string()],
                },
            ],
            escalation,
        ),
        TrustPlatform::LinuxRhel => with_escalation(
            vec![
                TrustCommand {
                    program: "rm".to_string(),
                    args: vec![
                        "-f".to_string(),
                        linux_anchor_path(material, TrustPlatform::LinuxRhel)
                            .display()
                            .to_string(),
                    ],
                },
                TrustCommand {
                    program: "update-ca-trust".to_string(),
                    args: vec!["extract".to_string()],
                },
            ],
            escalation,
        ),
        TrustPlatform::LinuxNss => vec![TrustCommand {
            program: "certutil".to_string(),
            args: vec![
                "-D".to_string(),
                "-d".to_string(),
                nss_database_spec(),
                "-n".to_string(),
                NSS_NICKNAME.to_string(),
            ],
        }],
    };
    command
}

fn inspect_commands(
    platform: TrustPlatform,
    material: &RouteProxyHttpsMaterial,
) -> Vec<TrustCommand> {
    match platform {
        TrustPlatform::WindowsCurrentUser => vec![TrustCommand {
            program: "certutil.exe".to_string(),
            args: vec![
                "-user".to_string(),
                "-store".to_string(),
                "Root".to_string(),
                material.root_thumbprint_sha1.clone(),
            ],
        }],
        TrustPlatform::MacOsLoginKeychain => vec![TrustCommand {
            program: "security".to_string(),
            args: vec![
                "find-certificate".to_string(),
                "-Z".to_string(),
                "-c".to_string(),
                ROOT_COMMON_NAME.to_string(),
                login_keychain_path().display().to_string(),
            ],
        }],
        TrustPlatform::LinuxP11Kit => vec![TrustCommand {
            program: "trust".to_string(),
            args: vec![
                "list".to_string(),
                "--filter=ca-anchors".to_string(),
                "--format=pem".to_string(),
            ],
        }],
        TrustPlatform::LinuxDebian | TrustPlatform::LinuxRhel => Vec::new(),
        TrustPlatform::LinuxNss => vec![TrustCommand {
            program: "certutil".to_string(),
            args: vec![
                "-L".to_string(),
                "-d".to_string(),
                nss_database_spec(),
                "-n".to_string(),
                NSS_NICKNAME.to_string(),
                "-a".to_string(),
            ],
        }],
    }
}

fn with_escalation(commands: Vec<TrustCommand>, escalation: Option<&str>) -> Vec<TrustCommand> {
    let Some(program) = escalation else {
        return commands;
    };

    commands
        .into_iter()
        .map(|command| {
            let mut args = Vec::with_capacity(command.args.len() + 1);
            args.push(command.program);
            args.extend(command.args);
            TrustCommand {
                program: program.to_string(),
                args,
            }
        })
        .collect()
}

async fn install_current_platform(material: &RouteProxyHttpsMaterial) -> Vec<TrustAttempt> {
    if cfg!(target_os = "windows") {
        return vec![
            run_adapter(
                "windows-current-user",
                TrustAdapterKind::System,
                install_commands(TrustPlatform::WindowsCurrentUser, material, None),
            )
            .await,
        ];
    }
    if cfg!(target_os = "macos") {
        return vec![
            run_adapter(
                "macos-login-keychain",
                TrustAdapterKind::System,
                install_commands(TrustPlatform::MacOsLoginKeychain, material, None),
            )
            .await,
        ];
    }
    if cfg!(target_os = "linux") {
        return install_linux(material).await;
    }

    Vec::new()
}

async fn uninstall_current_platform(
    material: &RouteProxyHttpsMaterial,
    installed_adapters: Option<&str>,
) -> Vec<TrustAttempt> {
    if cfg!(target_os = "windows") {
        return vec![
            run_adapter(
                "windows-current-user",
                TrustAdapterKind::System,
                uninstall_commands(TrustPlatform::WindowsCurrentUser, material, None),
            )
            .await,
        ];
    }
    if cfg!(target_os = "macos") {
        return vec![
            run_adapter(
                "macos-login-keychain",
                TrustAdapterKind::System,
                uninstall_commands(TrustPlatform::MacOsLoginKeychain, material, None),
            )
            .await,
        ];
    }
    if cfg!(target_os = "linux") {
        return uninstall_linux(material, installed_adapters).await;
    }

    Vec::new()
}

async fn install_linux(material: &RouteProxyHttpsMaterial) -> Vec<TrustAttempt> {
    let mut attempts = Vec::new();
    let mut system_succeeded = false;

    if executable_available("trust") {
        let attempt = run_adapter(
            "linux-p11-kit",
            TrustAdapterKind::System,
            install_commands(TrustPlatform::LinuxP11Kit, material, None),
        )
        .await;
        system_succeeded = attempt.result.is_ok();
        attempts.push(attempt);
    }

    if !system_succeeded {
        if let Some(platform) = linux_distribution_platform() {
            attempts.push(
                run_linux_system_adapter("linux-system-store", platform, material, true).await,
            );
        }
    }

    if nss_available() {
        attempts.push(
            run_adapter(
                "linux-nss",
                TrustAdapterKind::Nss,
                install_commands(TrustPlatform::LinuxNss, material, None),
            )
            .await,
        );
    }
    attempts
}

async fn uninstall_linux(
    material: &RouteProxyHttpsMaterial,
    installed_adapters: Option<&str>,
) -> Vec<TrustAttempt> {
    let mut attempts = Vec::new();

    if adapter_is_installed(installed_adapters, "linux-p11-kit") && executable_available("trust") {
        attempts.push(
            run_adapter(
                "linux-p11-kit",
                TrustAdapterKind::System,
                uninstall_commands(TrustPlatform::LinuxP11Kit, material, None),
            )
            .await,
        );
    }
    if adapter_is_installed(installed_adapters, "linux-system-store") {
        if let Some(platform) = linux_distribution_platform() {
            attempts.push(
                run_linux_system_adapter("linux-system-store", platform, material, false).await,
            );
        }
    }
    if adapter_is_installed(installed_adapters, "linux-nss") && nss_available() {
        attempts.push(
            run_adapter(
                "linux-nss",
                TrustAdapterKind::Nss,
                uninstall_commands(TrustPlatform::LinuxNss, material, None),
            )
            .await,
        );
    }
    attempts
}

fn adapter_is_installed(installed_adapters: Option<&str>, adapter: &str) -> bool {
    installed_adapters.is_none_or(|adapters| {
        adapters
            .split(',')
            .any(|installed| installed.trim() == adapter)
    })
}

async fn run_linux_system_adapter(
    adapter: &'static str,
    platform: TrustPlatform,
    material: &RouteProxyHttpsMaterial,
    install: bool,
) -> TrustAttempt {
    let commands = if install {
        install_commands(platform, material, None)
    } else {
        uninstall_commands(platform, material, None)
    };
    let direct = run_adapter(adapter, TrustAdapterKind::System, commands).await;
    if direct.result.is_ok()
        || !direct
            .result
            .as_ref()
            .err()
            .is_some_and(|error| permission_denied(error))
        || !executable_available("pkexec")
    {
        return direct;
    }

    let commands = if install {
        install_commands(platform, material, Some("pkexec"))
    } else {
        uninstall_commands(platform, material, Some("pkexec"))
    };
    run_adapter(adapter, TrustAdapterKind::System, commands).await
}

async fn run_adapter(
    adapter: &'static str,
    kind: TrustAdapterKind,
    commands: Vec<TrustCommand>,
) -> TrustAttempt {
    let mut failures = Vec::new();
    for command in &commands {
        if let Err(error) = run_command(command).await {
            failures.push(error);
            break;
        }
    }
    TrustAttempt {
        adapter,
        kind,
        result: if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join(" | "))
        },
    }
}

async fn inspect_windows(material: &RouteProxyHttpsMaterial) -> RouteProxyTrustOutcome {
    inspect_command_output(
        material,
        "windows-current-user",
        TrustAdapterKind::System,
        inspect_commands(TrustPlatform::WindowsCurrentUser, material),
    )
    .await
}

async fn inspect_macos(material: &RouteProxyHttpsMaterial) -> RouteProxyTrustOutcome {
    inspect_command_output(
        material,
        "macos-login-keychain",
        TrustAdapterKind::System,
        inspect_commands(TrustPlatform::MacOsLoginKeychain, material),
    )
    .await
}

async fn inspect_linux(material: &RouteProxyHttpsMaterial) -> RouteProxyTrustOutcome {
    let mut system_verified = Vec::new();
    let mut nss_verified = false;
    let mut inspection_errors = Vec::new();

    if executable_available("trust") {
        match command_output_matches_material(
            material,
            &inspect_commands(TrustPlatform::LinuxP11Kit, material),
        )
        .await
        {
            Ok(true) => system_verified.push("linux-p11-kit"),
            Ok(false) => {}
            Err(error) => inspection_errors.push(error),
        }
    }

    if let Some(platform) = linux_distribution_platform() {
        match file_matches_material(&linux_anchor_path(material, platform), material).await {
            Ok(true) => system_verified.push("linux-system-store"),
            Ok(false) => {}
            Err(error) => inspection_errors.push(error),
        }
    }

    if nss_available() {
        match command_output_matches_material(
            material,
            &inspect_commands(TrustPlatform::LinuxNss, material),
        )
        .await
        {
            Ok(true) => nss_verified = true,
            Ok(false) => {}
            Err(error) => inspection_errors.push(error),
        }
    }

    if !system_verified.is_empty() && inspection_errors.is_empty() {
        return RouteProxyTrustOutcome {
            status: RouteProxyTrustStatus::SystemTrusted,
            adapter: Some(system_verified.join(",")),
            message: Some("Managed Root CA is trusted by the local system store".to_string()),
            manual_instructions: Vec::new(),
        };
    }
    if !system_verified.is_empty() && !inspection_errors.is_empty() {
        return RouteProxyTrustOutcome {
            status: RouteProxyTrustStatus::PartiallyTrusted,
            adapter: Some(system_verified.join(",")),
            message: Some(format!(
                "Managed Root CA is trusted by one local store, but another trust store could not be inspected: {}",
                inspection_errors.join(" | ")
            )),
            manual_instructions: manual_instructions(material),
        };
    }
    if nss_verified && inspection_errors.is_empty() {
        return RouteProxyTrustOutcome {
            status: RouteProxyTrustStatus::NssTrusted,
            adapter: Some("linux-nss".to_string()),
            message: Some("Managed Root CA is trusted by the local NSS database".to_string()),
            manual_instructions: Vec::new(),
        };
    }
    if nss_verified || !inspection_errors.is_empty() {
        return RouteProxyTrustOutcome::unknown(
            material,
            format!(
                "Could not safely determine every local HTTPS trust store: {}",
                inspection_errors.join(" | ")
            ),
        );
    }

    RouteProxyTrustOutcome::untrusted(
        material,
        "Managed Root CA is not installed in a local trust store",
    )
}

async fn inspect_command_output(
    material: &RouteProxyHttpsMaterial,
    adapter: &'static str,
    kind: TrustAdapterKind,
    commands: Vec<TrustCommand>,
) -> RouteProxyTrustOutcome {
    match command_output_matches_material(material, &commands).await {
        Ok(true) => RouteProxyTrustOutcome {
            status: match kind {
                TrustAdapterKind::System => RouteProxyTrustStatus::SystemTrusted,
                TrustAdapterKind::Nss => RouteProxyTrustStatus::NssTrusted,
            },
            adapter: Some(adapter.to_string()),
            message: Some("Managed Root CA is installed in the local trust store".to_string()),
            manual_instructions: Vec::new(),
        },
        Ok(false) => RouteProxyTrustOutcome::untrusted(
            material,
            "Managed Root CA is not installed in the local trust store",
        ),
        Err(error) => RouteProxyTrustOutcome::unknown(
            material,
            format!("Could not safely inspect the local trust store: {error}"),
        ),
    }
}

async fn command_output_matches_material(
    material: &RouteProxyHttpsMaterial,
    commands: &[TrustCommand],
) -> Result<bool, String> {
    let mut output = String::new();
    for command in commands {
        output.push_str(&run_command_output(command).await?);
    }
    Ok(
        certificate_bundle_contains_material(output.as_bytes(), material)
            || text_output_contains_material(&output, material),
    )
}

async fn file_matches_material(
    path: &Path,
    material: &RouteProxyHttpsMaterial,
) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let contents = tokio::fs::read(path)
        .await
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    Ok(certificate_bundle_contains_material(&contents, material))
}

fn certificate_bundle_contains_material(
    contents: &[u8],
    material: &RouteProxyHttpsMaterial,
) -> bool {
    let mut remainder = contents;
    while !remainder.is_empty() {
        let Ok((next, pem)) = x509_parser::pem::parse_x509_pem(remainder) else {
            break;
        };
        if certificate_der_matches_material(&pem.contents, material) {
            return true;
        }
        if next.len() >= remainder.len() {
            break;
        }
        remainder = next;
    }
    false
}

fn certificate_der_matches_material(der: &[u8], material: &RouteProxyHttpsMaterial) -> bool {
    let Ok((_, certificate)) = x509_parser::parse_x509_certificate(der) else {
        return false;
    };
    let mut hasher = Sha1::new();
    hasher.update(der);
    let thumbprint = format!("{:x}", hasher.finalize());
    let common_name_matches = certificate
        .subject()
        .iter_common_name()
        .filter_map(|common_name| common_name.as_str().ok())
        .any(|common_name| common_name == ROOT_COMMON_NAME);
    thumbprint == material.root_thumbprint_sha1 && common_name_matches
}

fn text_output_contains_material(output: &str, material: &RouteProxyHttpsMaterial) -> bool {
    normalize_thumbprint(output).contains(&normalize_thumbprint(&material.root_thumbprint_sha1))
        && output.contains(ROOT_COMMON_NAME)
}

fn normalize_thumbprint(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn merge_install_outcome(
    material: &RouteProxyHttpsMaterial,
    mut inspected: RouteProxyTrustOutcome,
    attempts: Vec<TrustAttempt>,
) -> RouteProxyTrustOutcome {
    if attempts.is_empty() {
        return RouteProxyTrustOutcome::unknown(
            material,
            "HTTPS Root CA installation is not supported on this operating system",
        );
    }

    let succeeded = attempts
        .iter()
        .filter(|attempt| attempt.result.is_ok())
        .collect::<Vec<_>>();
    let failures = attempts
        .iter()
        .filter_map(|attempt| {
            attempt
                .result
                .as_ref()
                .err()
                .map(|error| (attempt.adapter, error))
        })
        .collect::<Vec<_>>();
    if !succeeded.is_empty() && !failures.is_empty() {
        inspected.status = RouteProxyTrustStatus::PartiallyTrusted;
        inspected.adapter = Some(
            succeeded
                .iter()
                .map(|attempt| attempt.adapter)
                .collect::<Vec<_>>()
                .join(","),
        );
        inspected.message = Some(format!(
            "Managed Root CA installation partially succeeded; failed adapters: {}",
            failures
                .iter()
                .map(|(adapter, error)| format!("{adapter}: {error}"))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
        inspected.manual_instructions = manual_instructions(material);
        return inspected;
    }
    if succeeded.is_empty() {
        return RouteProxyTrustOutcome::untrusted(
            material,
            format!(
                "Could not install the managed Root CA: {}",
                failures
                    .iter()
                    .map(|(adapter, error)| format!("{adapter}: {error}"))
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
        );
    }

    if inspected.status == RouteProxyTrustStatus::Untrusted {
        inspected.status = RouteProxyTrustStatus::Unknown;
        inspected.message = Some(
            "Root CA installation completed, but the local trust store could not verify the managed certificate"
                .to_string(),
        );
        inspected.manual_instructions = manual_instructions(material);
    }
    inspected
}

async fn run_command(command: &TrustCommand) -> Result<(), String> {
    run_command_output(command).await.map(|_| ())
}

async fn run_command_output(command: &TrustCommand) -> Result<String, String> {
    let output = tokio::process::Command::new(&command.program)
        .args(&command.args)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|error| format!("Could not run {}: {error}", command.program))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        return Ok(if stdout.is_empty() { stderr } else { stdout });
    }
    Err(if stderr.is_empty() { stdout } else { stderr })
}

fn permission_denied(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("permission denied")
        || normalized.contains("operation not permitted")
        || normalized.contains("access is denied")
        || normalized.contains("eacces")
}

fn executable_available(program: &str) -> bool {
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    let extensions = if cfg!(target_os = "windows") {
        vec!["", ".exe", ".cmd", ".bat"]
    } else {
        vec![""]
    };
    env::split_paths(&paths).any(|directory| {
        extensions
            .iter()
            .any(|extension| directory.join(format!("{program}{extension}")).is_file())
    })
}

fn linux_distribution_platform() -> Option<TrustPlatform> {
    let contents = std::fs::read_to_string("/etc/os-release").ok()?;
    let value = contents
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(key, _)| *key == "ID" || *key == "ID_LIKE")
        .map(|(_, value)| value.trim_matches('"').to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    if ["debian", "ubuntu"]
        .iter()
        .any(|needle| value.contains(needle))
    {
        return Some(TrustPlatform::LinuxDebian);
    }
    if ["rhel", "fedora", "centos", "suse"]
        .iter()
        .any(|needle| value.contains(needle))
    {
        return Some(TrustPlatform::LinuxRhel);
    }
    None
}

fn linux_anchor_path(material: &RouteProxyHttpsMaterial, platform: TrustPlatform) -> PathBuf {
    let filename = format!(
        "ai-switch-route-proxy-{}.crt",
        material.root_fingerprint_sha256
    );
    match platform {
        TrustPlatform::LinuxDebian => {
            PathBuf::from("/usr/local/share/ca-certificates").join(filename)
        }
        TrustPlatform::LinuxRhel => {
            PathBuf::from("/etc/pki/ca-trust/source/anchors").join(filename)
        }
        _ => PathBuf::from(filename),
    }
}

fn nss_available() -> bool {
    executable_available("certutil") && nss_database_path().is_dir()
}

fn nss_database_path() -> PathBuf {
    home_dir().join(".pki").join("nssdb")
}

fn nss_database_spec() -> String {
    format!("sql:{}", nss_database_path().display())
}

fn login_keychain_path() -> PathBuf {
    home_dir()
        .join("Library")
        .join("Keychains")
        .join("login.keychain-db")
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_default()
}

fn manual_instructions(material: &RouteProxyHttpsMaterial) -> Vec<String> {
    let root = material.root_certificate_pem.display().to_string();
    if cfg!(target_os = "windows") {
        return vec![format!("certutil.exe -user -addstore Root {root}")];
    }
    if cfg!(target_os = "macos") {
        return vec![format!(
            "security add-trusted-cert -d -r trustRoot -k {} {root}",
            login_keychain_path().display()
        )];
    }
    if cfg!(target_os = "linux") {
        return vec![
            format!("trust anchor {root}"),
            format!(
                "install -Dm644 {root} {} && update-ca-certificates",
                linux_anchor_path(material, TrustPlatform::LinuxDebian).display()
            ),
            format!(
                "install -Dm644 {root} {} && update-ca-trust extract",
                linux_anchor_path(material, TrustPlatform::LinuxRhel).display()
            ),
            format!(
                "certutil -A -d {} -n \"{NSS_NICKNAME}\" -t C,, -i {root}",
                nss_database_spec()
            ),
        ];
    }
    vec![format!(
        "Trust the managed Root CA at {root} in your local certificate store"
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::route_proxy_https_service::RouteProxyHttpsMaterial;
    use std::path::PathBuf;

    fn fixture_material() -> RouteProxyHttpsMaterial {
        RouteProxyHttpsMaterial {
            root_certificate_pem: PathBuf::from("C:/tmp/ai-switch/root-ca.pem"),
            root_fingerprint_sha256: "a".repeat(64),
            root_thumbprint_sha1: "b".repeat(40),
            server_certificate_pem: PathBuf::from("C:/tmp/ai-switch/server-cert.pem"),
            server_private_key_pem: PathBuf::from("C:/tmp/ai-switch/server-key.pem"),
            expires_at: "2027-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn windows_commands_install_and_remove_only_the_managed_thumbprint() {
        let material = fixture_material();
        let install = install_commands(TrustPlatform::WindowsCurrentUser, &material, None);
        let uninstall = uninstall_commands(TrustPlatform::WindowsCurrentUser, &material, None);

        assert_eq!(
            install,
            vec![TrustCommand {
                program: "certutil.exe".to_string(),
                args: vec![
                    "-user".to_string(),
                    "-addstore".to_string(),
                    "Root".to_string(),
                    material.root_certificate_pem.display().to_string(),
                ],
            }]
        );
        assert_eq!(
            uninstall,
            vec![TrustCommand {
                program: "certutil.exe".to_string(),
                args: vec![
                    "-user".to_string(),
                    "-delstore".to_string(),
                    "Root".to_string(),
                    material.root_thumbprint_sha1,
                ],
            }]
        );
    }

    #[test]
    fn macos_commands_target_only_the_login_keychain_and_thumbprint() {
        let material = fixture_material();
        let uninstall = uninstall_commands(TrustPlatform::MacOsLoginKeychain, &material, None);

        assert_eq!(uninstall[0].program, "security");
        assert_eq!(
            uninstall[0].args,
            vec![
                "delete-certificate".to_string(),
                "-Z".to_string(),
                material.root_thumbprint_sha1,
                login_keychain_path().display().to_string(),
            ]
        );
    }

    #[test]
    fn linux_uninstall_targets_only_previously_verified_adapters() {
        assert!(adapter_is_installed(
            Some("linux-p11-kit,linux-system-store"),
            "linux-system-store"
        ));
        assert!(!adapter_is_installed(
            Some("linux-nss"),
            "linux-system-store"
        ));
        assert!(adapter_is_installed(None, "linux-system-store"));
    }

    #[test]
    fn linux_commands_use_fixed_store_paths_and_never_shell_syntax() {
        let material = fixture_material();
        let commands = install_commands(TrustPlatform::LinuxDebian, &material, Some("pkexec"));

        assert_eq!(commands[0].program, "pkexec");
        assert_eq!(commands[0].args[0], "install");
        assert!(commands
            .iter()
            .all(|command| !command.args.iter().any(|arg| arg.contains("sh -c"))));
        assert!(commands
            .iter()
            .all(|command| !command.args.iter().any(|arg| arg.contains(';'))));
        assert!(commands.iter().any(|command| command
            .args
            .iter()
            .any(|arg| arg == "update-ca-certificates")));
    }

    #[test]
    fn nss_commands_use_the_fixed_ai_switch_nickname() {
        let material = fixture_material();
        let commands = install_commands(TrustPlatform::LinuxNss, &material, None);

        assert_eq!(commands[0].program, "certutil");
        assert!(commands[0]
            .args
            .windows(2)
            .any(|args| args == ["-n", "AI Switch Route Proxy Root CA"]));
        assert!(commands[0]
            .args
            .windows(2)
            .any(|args| args == ["-t", "C,,"]));
        assert!(commands[0]
            .args
            .windows(2)
            .any(|args| args[0] == "-d" && args[1].starts_with("sql:")));
    }
}
