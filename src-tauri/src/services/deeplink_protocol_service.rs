use crate::error::AppError;
use crate::models::settings::{AppSettings, AppSettingsView};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

pub const UNSUPPORTED_REASON: &str = "capability.deeplink_compat_unavailable";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepLinkProtocolStatus {
    pub supported: bool,
    pub ccswitch_registered: bool,
    pub reason: Option<String>,
}

pub trait DeepLinkProtocolRegistrar: Send + Sync {
    fn status(&self) -> DeepLinkProtocolStatus;
    fn set_ccswitch_enabled(&self, enabled: bool) -> Result<(), AppError>;
}

#[derive(Clone)]
pub struct DeepLinkProtocolRuntime {
    registrar: Arc<RwLock<Arc<dyn DeepLinkProtocolRegistrar>>>,
    enabled: Arc<AtomicBool>,
}

impl Default for DeepLinkProtocolRuntime {
    fn default() -> Self {
        Self::unavailable()
    }
}

impl DeepLinkProtocolRuntime {
    pub fn unavailable() -> Self {
        Self::with_registrar(Arc::new(UnavailableRegistrar))
    }

    pub fn with_registrar(registrar: Arc<dyn DeepLinkProtocolRegistrar>) -> Self {
        let enabled = registrar.status().ccswitch_registered;
        Self {
            registrar: Arc::new(RwLock::new(registrar)),
            enabled: Arc::new(AtomicBool::new(enabled)),
        }
    }

    pub fn attach_registrar(&self, registrar: Arc<dyn DeepLinkProtocolRegistrar>) {
        let enabled = registrar.status().ccswitch_registered;
        *self.registrar.write().expect("deep link registrar lock") = registrar;
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    pub fn status(&self) -> DeepLinkProtocolStatus {
        self.registrar
            .read()
            .expect("deep link registrar lock")
            .status()
    }

    pub fn ccswitch_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    pub fn set_ccswitch_enabled(&self, enabled: bool) -> Result<(), AppError> {
        let registrar = self
            .registrar
            .read()
            .expect("deep link registrar lock")
            .clone();
        registrar.set_ccswitch_enabled(enabled)?;
        self.enabled.store(enabled, Ordering::SeqCst);
        Ok(())
    }

    pub fn view(&self, settings: AppSettings) -> AppSettingsView {
        AppSettingsView::from_settings(settings, self.status().supported)
    }
}

struct UnavailableRegistrar;

impl DeepLinkProtocolRegistrar for UnavailableRegistrar {
    fn status(&self) -> DeepLinkProtocolStatus {
        DeepLinkProtocolStatus {
            supported: false,
            ccswitch_registered: false,
            reason: Some(UNSUPPORTED_REASON.to_string()),
        }
    }

    fn set_ccswitch_enabled(&self, enabled: bool) -> Result<(), AppError> {
        if enabled {
            Err(AppError::Validation {
                code: UNSUPPORTED_REASON,
                message: "cc-switch deep-link compatibility is unavailable on this runtime".into(),
                details: None,
                recoverable: true,
            })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeRegistrar {
        owns_scheme: AtomicBool,
        calls: Mutex<Vec<String>>,
    }

    impl FakeRegistrar {
        fn registered(owns_scheme: bool) -> Self {
            Self {
                owns_scheme: AtomicBool::new(owns_scheme),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl DeepLinkProtocolRegistrar for FakeRegistrar {
        fn status(&self) -> DeepLinkProtocolStatus {
            DeepLinkProtocolStatus {
                supported: true,
                ccswitch_registered: self.owns_scheme.load(Ordering::SeqCst),
                reason: None,
            }
        }

        fn set_ccswitch_enabled(&self, enabled: bool) -> Result<(), AppError> {
            self.calls.lock().unwrap().push(if enabled {
                "register:ccswitch".into()
            } else {
                "unregister:ccswitch".into()
            });
            if !enabled && !self.owns_scheme.load(Ordering::SeqCst) {
                return Ok(());
            }
            self.owns_scheme.store(enabled, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn disabling_unowned_scheme_is_a_noop() {
        let registrar = Arc::new(FakeRegistrar::registered(false));
        let runtime = DeepLinkProtocolRuntime::with_registrar(registrar.clone());
        runtime.set_ccswitch_enabled(false).unwrap();
        assert!(!runtime.status().ccswitch_registered);
        assert_eq!(registrar.calls.lock().unwrap().as_slice(), ["unregister:ccswitch"]);
    }
}
