#![allow(dead_code)]

use crate::error::AppError;

pub trait SecretStore: Send + Sync {
    fn set_secret(&self, key: &str, value: &str) -> Result<(), AppError>;
    fn get_secret(&self, key: &str) -> Result<String, AppError>;
}

pub struct KeyringSecretStore {
    service: String,
}

impl KeyringSecretStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}

impl SecretStore for KeyringSecretStore {
    fn set_secret(&self, key: &str, value: &str) -> Result<(), AppError> {
        let entry = keyring::Entry::new(&self.service, key).map_err(|err| AppError::Secret {
            code: "secret.entry",
            message: "Could not create keyring entry".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        entry.set_password(value).map_err(|err| AppError::Secret {
            code: "secret.set",
            message: "Could not save secret to keyring".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    fn get_secret(&self, key: &str) -> Result<String, AppError> {
        let entry = keyring::Entry::new(&self.service, key).map_err(|err| AppError::Secret {
            code: "secret.entry",
            message: "Could not create keyring entry".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        entry.get_password().map_err(|err| AppError::Secret {
            code: "secret.get",
            message: "Could not read secret from keyring".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }
}
