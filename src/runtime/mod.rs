//! Runtime configuration boundary for MOVA Agent API V0.

use crate::auth::AuthTrustConfig;
use crate::connectors::{
    ConnectorExecutionConfig, EndpointEvidencePolicy, EndpointRegistryEntry, ProviderConnectorRegistryEntry,
    SideEffectIntent,
};
use crate::secrets::{SecretBoundaryError, SecretRef, SecretRefKind};
use crate::storage::StorageConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeProviderCapabilities {
    pub provider_kind: String,
    pub supports_env_loading: bool,
    pub supports_secret_resolution: bool,
    pub supports_live_deploy_binding: bool,
}

pub trait SecretResolver: Send + Sync {
    fn resolve(&self, reference: &SecretRef) -> Result<Option<String>, SecretBoundaryError>;
}

pub trait RuntimeProvider: Send + Sync {
    fn capabilities(&self) -> RuntimeProviderCapabilities;
    fn load_runtime_config(&self) -> Result<RuntimeConfig, RuntimeConfigError>;
}

#[derive(Debug, Clone)]
pub struct LocalEnvRuntimeProvider {
    defaults: RuntimeConfig,
}

impl LocalEnvRuntimeProvider {
    pub fn new(defaults: RuntimeConfig) -> Self {
        Self { defaults }
    }
}

impl RuntimeProvider for LocalEnvRuntimeProvider {
    fn capabilities(&self) -> RuntimeProviderCapabilities {
        RuntimeProviderCapabilities {
            provider_kind: "local_env".to_string(),
            supports_env_loading: true,
            supports_secret_resolution: true,
            supports_live_deploy_binding: false,
        }
    }

    fn load_runtime_config(&self) -> Result<RuntimeConfig, RuntimeConfigError> {
        let mut cfg = self.defaults.clone();

        if let Ok(value) = std::env::var("MOVA_AUTH_VERIFIER_KIND") {
            cfg.auth.verifier_kind = value;
        }
        if let Ok(value) = std::env::var("MOVA_STORAGE_ADAPTER_KIND") {
            cfg.storage.adapter_kind = value;
        }
        if let Ok(value) = std::env::var("MOVA_STORAGE_BASE_PATH") {
            cfg.storage.base_path = Some(value);
        }
        if let Ok(value) = std::env::var("MOVA_CONNECTOR_ADAPTER_KIND") {
            cfg.connectors.adapter_kind = value;
        }
        if let Ok(value) = std::env::var("MOVA_WEBHOOK_SITE_ALLOWED_URL") {
            cfg.connectors.allowed_webhook_urls = vec![value];
        }
        if let Ok(value) = std::env::var("MOVA_HTTP_ENDPOINT_REF") {
            let url = std::env::var("MOVA_HTTP_ENDPOINT_URL").unwrap_or_default();
            let methods = std::env::var("MOVA_HTTP_ENDPOINT_ALLOWED_METHODS")
                .ok()
                .map(|v| {
                    v.split(',')
                        .map(|m| m.trim().to_ascii_uppercase())
                        .filter(|m| !m.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec!["POST".to_string()]);
            if !url.is_empty() {
                cfg.connectors.endpoint_registry = vec![EndpointRegistryEntry {
                    endpoint_ref: value,
                    url,
                    allowed_methods: methods,
                    allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
                    required_scopes: vec!["actions.run".to_string()],
                    timeout_ms: cfg.connectors.timeout_ms,
                    max_retries: cfg.connectors.max_retries,
                    evidence_policy: EndpointEvidencePolicy::SummaryOnly,
                    enabled: true,
                }];
            }
        }
        if let Ok(value) = std::env::var("MOVA_HTTP_ENDPOINT_REGISTRY_JSON") {
            let parsed: Vec<EndpointRegistryEntry> = serde_json::from_str(&value).map_err(|_| {
                RuntimeConfigError::new(
                    "connector_config_invalid",
                    "MOVA_HTTP_ENDPOINT_REGISTRY_JSON must be valid endpoint registry JSON array",
                )
            })?;
            if !parsed.is_empty() {
                cfg.connectors.endpoint_registry = parsed;
            }
        }
        if let Ok(value) = std::env::var("MOVA_PROVIDER_CONNECTOR_REGISTRY_JSON") {
            let parsed: Vec<ProviderConnectorRegistryEntry> = serde_json::from_str(&value).map_err(|_| {
                RuntimeConfigError::new(
                    "connector_config_invalid",
                    "MOVA_PROVIDER_CONNECTOR_REGISTRY_JSON must be valid provider connector registry JSON array",
                )
            })?;
            cfg.connectors.provider_connector_registry = parsed;
        }
        if cfg.connectors.adapter_kind == "webhook_site" {
            if cfg.connectors.allowed_connectors.is_empty() {
                cfg.connectors.allowed_connectors = vec!["connector.webhook_site.v1".to_string()];
            }
            if cfg
                .connectors
                .allowed_side_effect_intents
                .iter()
                .all(|i| *i != crate::connectors::SideEffectIntent::ExternalNetwork)
            {
                cfg.connectors
                    .allowed_side_effect_intents
                    .push(crate::connectors::SideEffectIntent::ExternalNetwork);
            }
        }
        if cfg.connectors.adapter_kind == "http_generic" {
            if cfg.connectors.allowed_connectors.is_empty() {
                cfg.connectors.allowed_connectors = vec!["connector.http.generic.v1".to_string()];
            }
            if !cfg
                .connectors
                .allowed_connectors
                .iter()
                .any(|value| value == "provider.connector.v1")
            {
                cfg.connectors
                    .allowed_connectors
                    .push("provider.connector.v1".to_string());
            }
            if cfg
                .connectors
                .allowed_side_effect_intents
                .iter()
                .all(|i| *i != SideEffectIntent::ExternalNetwork)
            {
                cfg.connectors
                    .allowed_side_effect_intents
                    .push(SideEffectIntent::ExternalNetwork);
            }
        }

        cfg.validate()?;
        Ok(cfg)
    }
}

#[derive(Debug, Clone, Default)]
pub struct LocalEnvSecretResolver {
    runtime_secrets: HashMap<String, String>,
}

impl LocalEnvSecretResolver {
    pub fn new(runtime_secrets: HashMap<String, String>) -> Self {
        Self { runtime_secrets }
    }
}

impl SecretResolver for LocalEnvSecretResolver {
    fn resolve(&self, reference: &SecretRef) -> Result<Option<String>, SecretBoundaryError> {
        reference.validate()?;
        match reference.kind {
            SecretRefKind::SecretRef => Ok(None),
            SecretRefKind::EnvRef => {
                let key = reference
                    .reference
                    .strip_prefix("env://")
                    .unwrap_or(&reference.reference);
                Ok(std::env::var(key).ok())
            }
            SecretRefKind::RuntimeSecret => {
                let key = reference
                    .reference
                    .strip_prefix("runtime://")
                    .unwrap_or(&reference.reference);
                Ok(self.runtime_secrets.get(key).cloned())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct FailingRuntimeProvider {
    err: RuntimeConfigError,
}

impl FailingRuntimeProvider {
    pub fn new(err: RuntimeConfigError) -> Self {
        Self { err }
    }
}

impl RuntimeProvider for FailingRuntimeProvider {
    fn capabilities(&self) -> RuntimeProviderCapabilities {
        RuntimeProviderCapabilities {
            provider_kind: "failing".to_string(),
            supports_env_loading: false,
            supports_secret_resolution: false,
            supports_live_deploy_binding: false,
        }
    }

    fn load_runtime_config(&self) -> Result<RuntimeConfig, RuntimeConfigError> {
        Err(self.err.clone())
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

    #[test]
    fn local_env_provider_capabilities_are_provider_safe() {
        let provider = LocalEnvRuntimeProvider::new(RuntimeConfig::deterministic_local_default());
        let capabilities = provider.capabilities();
        assert_eq!(capabilities.provider_kind, "local_env");
        assert!(!capabilities.supports_live_deploy_binding);
    }

    #[test]
    fn local_env_secret_resolver_supports_runtime_secret_kind() {
        let resolver = LocalEnvSecretResolver::new(HashMap::from([(
            "connector/docs".to_string(),
            "local-secret".to_string(),
        )]));
        let resolved = resolver
            .resolve(&SecretRef {
                kind: SecretRefKind::RuntimeSecret,
                reference: "runtime://connector/docs".to_string(),
            })
            .unwrap();
        assert_eq!(resolved.as_deref(), Some("local-secret"));
    }
}
