//! Runtime configuration boundary for MOVA Agent API V0.

use crate::auth::AuthTrustConfig;
use crate::connectors::ConnectorExecutionConfig;
use crate::storage::StorageConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub auth: AuthTrustConfig,
    pub storage: StorageConfig,
    pub connectors: ConnectorExecutionConfig,
}

impl RuntimeConfig {
    pub fn deterministic_local_default() -> Self {
        Self {
            auth: AuthTrustConfig::default_v0(),
            storage: StorageConfig::in_memory_default(),
            connectors: ConnectorExecutionConfig::deterministic_local_default(),
        }
    }

    pub fn validate(&self) -> Result<(), RuntimeConfigError> {
        self.auth
            .validate()
            .map_err(|e| RuntimeConfigError::new("auth_config_invalid", &e))?;
        self.storage
            .validate()
            .map_err(|e| RuntimeConfigError::new(&e.code, &e.message))?;
        self.connectors
            .validate()
            .map_err(|e| RuntimeConfigError::new(&e.code, &e.message))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeConfigError {
    pub code: String,
    pub message: String,
}

impl RuntimeConfigError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
        }
    }
}

pub trait RuntimeConfigLoader: Send + Sync {
    fn load(&self) -> Result<RuntimeConfig, RuntimeConfigError>;
}

#[derive(Debug, Clone)]
pub struct StaticRuntimeConfigLoader {
    config: RuntimeConfig,
}

impl StaticRuntimeConfigLoader {
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }
}

impl RuntimeConfigLoader for StaticRuntimeConfigLoader {
    fn load(&self) -> Result<RuntimeConfig, RuntimeConfigError> {
        self.config.validate()?;
        Ok(self.config.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_default_runtime_config_validates() {
        let cfg = RuntimeConfig::deterministic_local_default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn invalid_runtime_config_fails_validation() {
        let mut cfg = RuntimeConfig::deterministic_local_default();
        cfg.storage.adapter_kind = "unsupported".to_string();
        let err = cfg.validate().unwrap_err();
        assert_eq!(err.code, "storage_config_invalid");
    }
}
