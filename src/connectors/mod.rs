//! Connector execution boundary for MOVA Agent API V0.
//!
//! This module provides controlled connector execution semantics with explicit
//! side-effect intent guardrails and provider-agnostic adapter contracts.

use crate::policy::PolicySummary;
use crate::secrets::SecretRef;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use url::Url;

pub const CONNECTOR_SIDE_EFFECTS_ENABLED: bool = false;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorCallStatus {
    Pending,
    Allowed,
    Blocked,
    Failed,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectIntent {
    None,
    LocalOnly,
    ExternalNetwork,
    Destructive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorTiming {
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorCall {
    pub connector_id: String,
    pub call_id: String,
    pub side_effect_intent: SideEffectIntent,
    pub request: Value,
    pub auth_context: Value,
    pub policy_result: PolicySummary,
    pub status: ConnectorCallStatus,
    pub response: Value,
    pub timing: ConnectorTiming,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorExecutionRequest {
    pub connector_id: String,
    pub call_id: String,
    pub side_effect_intent: SideEffectIntent,
    pub request: Value,
    pub auth_context: Value,
    pub credential_refs: Vec<SecretRef>,
    pub policy_result: PolicySummary,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorExecutionResult {
    pub call: ConnectorCall,
    pub guard_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorExecutionError {
    pub code: String,
    pub message: String,
}

impl ConnectorExecutionError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointRegistryEntry {
    pub endpoint_ref: String,
    pub url: String,
    pub allowed_methods: Vec<String>,
    pub allowed_side_effect_intents: Vec<SideEffectIntent>,
    pub required_scopes: Vec<String>,
    pub timeout_ms: u64,
    pub max_retries: u8,
    pub evidence_policy: EndpointEvidencePolicy,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConnectorRegistryEntry {
    pub connector_ref: String,
    pub provider: String,
    pub operation: String,
    pub required_scopes: Vec<String>,
    pub allowed_operations: Vec<String>,
    pub secret_refs: HashMap<String, String>,
    #[serde(default)]
    pub target_resolver: Option<String>,
    pub evidence_policy: EndpointEvidencePolicy,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTargetRegistryEntry {
    pub target_ref: String,
    pub connector_id: String,
    #[serde(default)]
    pub endpoint_ref: Option<String>,
    #[serde(default)]
    pub connector_ref: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    pub side_effect_intent: SideEffectIntent,
    pub required_scopes: Vec<String>,
    pub allowed_operations: Vec<String>,
    #[serde(default)]
    pub target_resolver: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointEvidencePolicy {
    SummaryOnly,
    HeadersRedacted,
    BodyRedacted,
    StatusOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorExecutionConfig {
    pub adapter_kind: String,
    pub allowed_connectors: Vec<String>,
    pub allowed_side_effect_intents: Vec<SideEffectIntent>,
    pub offline_stub_rules: Vec<OfflineStubRule>,
    pub allowed_webhook_urls: Vec<String>,
    pub endpoint_registry: Vec<EndpointRegistryEntry>,
    pub provider_connector_registry: Vec<ProviderConnectorRegistryEntry>,
    pub target_registry: Vec<RuntimeTargetRegistryEntry>,
    pub timeout_ms: u64,
    pub max_retries: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflineStubRule {
    pub connector_id: String,
    pub call_status: ConnectorCallStatus,
    pub guard_reason: String,
}

impl ConnectorExecutionConfig {
    pub fn deterministic_local_default() -> Self {
        Self {
            adapter_kind: "deterministic_local".to_string(),
            allowed_connectors: vec![
                "connector.docs.v1".to_string(),
                "connector.http.generic.v1".to_string(),
                "provider.connector.v1".to_string(),
                "tenant.ledger.v1".to_string(),
            ],
            allowed_side_effect_intents: vec![SideEffectIntent::None, SideEffectIntent::LocalOnly],
            offline_stub_rules: Vec::new(),
            allowed_webhook_urls: Vec::new(),
            endpoint_registry: vec![
                EndpointRegistryEntry {
                    endpoint_ref: "webhook_site_test".to_string(),
                    url: "https://webhook.site/test-endpoint".to_string(),
                    allowed_methods: vec!["POST".to_string()],
                    allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
                    required_scopes: vec!["actions.run".to_string()],
                    timeout_ms: 10_000,
                    max_retries: 0,
                    evidence_policy: EndpointEvidencePolicy::SummaryOnly,
                    enabled: true,
                },
                EndpointRegistryEntry {
                    endpoint_ref: "webhook_site_contract_run_test".to_string(),
                    url: "https://webhook.site/test-endpoint".to_string(),
                    allowed_methods: vec!["POST".to_string()],
                    allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
                    required_scopes: vec!["contracts.run".to_string()],
                    timeout_ms: 10_000,
                    max_retries: 0,
                    evidence_policy: EndpointEvidencePolicy::SummaryOnly,
                    enabled: true,
                },
            ],
            provider_connector_registry: vec![ProviderConnectorRegistryEntry {
                connector_ref: "telegram.fixture_primary_channel".to_string(),
                provider: "telegram".to_string(),
                operation: "send_message".to_string(),
                required_scopes: vec!["contracts.run".to_string()],
                allowed_operations: vec!["op_provider_send_message".to_string()],
                secret_refs: HashMap::from([
                    ("bot_token".to_string(), "TELEGRAM_BOT_TOKEN".to_string()),
                    ("chat_id".to_string(), "TELEGRAM_OWNER_REPORT_CHAT_ID".to_string()),
                ]),
                target_resolver: None,
                evidence_policy: EndpointEvidencePolicy::SummaryOnly,
                enabled: true,
            },
            ProviderConnectorRegistryEntry {
                connector_ref: "telegram.runtime_chat".to_string(),
                provider: "telegram".to_string(),
                operation: "send_message".to_string(),
                required_scopes: vec!["contracts.run".to_string()],
                allowed_operations: vec!["*".to_string()],
                secret_refs: HashMap::from([
                    ("bot_token".to_string(), "TELEGRAM_BOT_TOKEN".to_string()),
                ]),
                target_resolver: Some("context:runtime_targets.current.chat_id".to_string()),
                evidence_policy: EndpointEvidencePolicy::SummaryOnly,
                enabled: true,
            },
            ProviderConnectorRegistryEntry {
                connector_ref: "telegram.admin_chat".to_string(),
                provider: "telegram".to_string(),
                operation: "send_message".to_string(),
                required_scopes: vec!["contracts.run".to_string()],
                allowed_operations: vec!["*".to_string()],
                secret_refs: HashMap::from([
                    ("bot_token".to_string(), "TELEGRAM_BOT_TOKEN".to_string()),
                ]),
                target_resolver: Some("context:runtime_targets.admin.chat_id".to_string()),
                evidence_policy: EndpointEvidencePolicy::SummaryOnly,
                enabled: true,
            }],
            target_registry: vec![
                RuntimeTargetRegistryEntry {
                    target_ref: "connector_target://telegram.fixture_primary_channel.send_message".to_string(),
                    connector_id: "provider.connector.v1".to_string(),
                    endpoint_ref: None,
                    connector_ref: Some("telegram.fixture_primary_channel".to_string()),
                    provider: Some("telegram".to_string()),
                    operation: Some("send_message".to_string()),
                    method: Some("POST".to_string()),
                    side_effect_intent: SideEffectIntent::ExternalNetwork,
                    required_scopes: vec!["contracts.run".to_string()],
                    allowed_operations: vec!["op_provider_send_message".to_string()],
                    target_resolver: None,
                    enabled: true,
                },
                RuntimeTargetRegistryEntry {
                    target_ref: "binding://telegram_current_chat_send_message".to_string(),
                    connector_id: "provider.connector.v1".to_string(),
                    endpoint_ref: None,
                    connector_ref: Some("telegram.runtime_chat".to_string()),
                    provider: Some("telegram".to_string()),
                    operation: Some("send_message".to_string()),
                    method: Some("POST".to_string()),
                    side_effect_intent: SideEffectIntent::ExternalNetwork,
                    required_scopes: vec!["contracts.run".to_string()],
                    allowed_operations: vec![],
                    target_resolver: Some("context:runtime_targets.current.chat_id".to_string()),
                    enabled: true,
                },
                RuntimeTargetRegistryEntry {
                    target_ref: "binding://telegram_admin_chat_send_message".to_string(),
                    connector_id: "provider.connector.v1".to_string(),
                    endpoint_ref: None,
                    connector_ref: Some("telegram.admin_chat".to_string()),
                    provider: Some("telegram".to_string()),
                    operation: Some("send_message".to_string()),
                    method: Some("POST".to_string()),
                    side_effect_intent: SideEffectIntent::ExternalNetwork,
                    required_scopes: vec!["contracts.run".to_string()],
                    allowed_operations: vec![],
                    target_resolver: Some("context:runtime_targets.admin.chat_id".to_string()),
                    enabled: true,
                },
                RuntimeTargetRegistryEntry {
                    target_ref: "binding://tenant_ledger_write".to_string(),
                    connector_id: "tenant.ledger.v1".to_string(),
                    endpoint_ref: None,
                    connector_ref: None,
                    provider: Some("tenant".to_string()),
                    operation: Some("write_record".to_string()),
                    method: Some("WRITE".to_string()),
                    side_effect_intent: SideEffectIntent::LocalOnly,
                    required_scopes: vec!["contracts.run".to_string()],
                    allowed_operations: vec![],
                    target_resolver: None,
                    enabled: true,
                },
            ],
            timeout_ms: 10_000,
            max_retries: 0,
        }
    }

    pub fn validate(&self) -> Result<(), ConnectorExecutionError> {
        if self.adapter_kind.trim().is_empty() {
            return Err(ConnectorExecutionError::new(
                "connector_config_invalid",
                "adapter_kind is required",
            ));
        }
        if self.adapter_kind != "deterministic_local"
            && self.adapter_kind != "offline_stub"
            && self.adapter_kind != "webhook_site"
            && self.adapter_kind != "http_generic"
        {
            return Err(ConnectorExecutionError::new(
                "connector_config_invalid",
                "unsupported adapter_kind",
            ));
        }
        if self.adapter_kind == "deterministic_local" && self.allowed_connectors.is_empty() {
            return Err(ConnectorExecutionError::new(
                "connector_config_invalid",
                "allowed_connectors is required for deterministic_local",
            ));
        }
        if self.adapter_kind == "offline_stub" && self.offline_stub_rules.is_empty() {
            return Err(ConnectorExecutionError::new(
                "connector_config_invalid",
                "offline_stub_rules is required for offline_stub",
            ));
        }
        if self.adapter_kind == "webhook_site" {
            if self.allowed_webhook_urls.len() != 1 {
                return Err(ConnectorExecutionError::new(
                    "connector_config_invalid",
                    "exactly one allowed_webhook_url is required for webhook_site",
                ));
            }
            let parsed = Url::parse(&self.allowed_webhook_urls[0]).map_err(|_| {
                ConnectorExecutionError::new(
                    "connector_config_invalid",
                    "allowed_webhook_url must be a valid absolute URL",
                )
            })?;
            if parsed.scheme() != "https" || parsed.host_str() != Some("webhook.site") {
                return Err(ConnectorExecutionError::new(
                    "connector_config_invalid",
                    "allowed_webhook_url must be https://webhook.site/*",
                ));
            }
            if !self
                .allowed_side_effect_intents
                .iter()
                .any(|intent| *intent == SideEffectIntent::ExternalNetwork)
            {
                return Err(ConnectorExecutionError::new(
                    "connector_config_invalid",
                    "webhook_site requires external_network side-effect intent allowance",
                ));
            }
        }
        if self.adapter_kind == "http_generic" || !self.endpoint_registry.is_empty() {
            if self.endpoint_registry.is_empty() {
                return Err(ConnectorExecutionError::new(
                    "connector_config_invalid",
                    "endpoint_registry is required for http_generic",
                ));
            }
            for endpoint in &self.endpoint_registry {
                if endpoint.endpoint_ref.trim().is_empty() {
                    return Err(ConnectorExecutionError::new(
                        "connector_config_invalid",
                        "endpoint_ref must not be empty",
                    ));
                }
                let parsed = Url::parse(&endpoint.url).map_err(|_| {
                    ConnectorExecutionError::new(
                        "connector_config_invalid",
                        "endpoint_registry url must be a valid absolute URL",
                    )
                })?;
                if parsed.scheme() != "https" {
                    return Err(ConnectorExecutionError::new(
                        "connector_config_invalid",
                        "http_generic endpoint URLs must use https",
                    ));
                }
                if endpoint.allowed_methods.is_empty() {
                    return Err(ConnectorExecutionError::new(
                        "connector_config_invalid",
                        "endpoint allowed_methods must not be empty",
                    ));
                }
                for method in &endpoint.allowed_methods {
                    if method != "GET" && method != "POST" {
                        return Err(ConnectorExecutionError::new(
                            "connector_config_invalid",
                            "endpoint allowed_methods supports only GET/POST in V0",
                        ));
                    }
                }
                if endpoint.allowed_side_effect_intents.is_empty() {
                    return Err(ConnectorExecutionError::new(
                        "endpoint_config_invalid",
                        "endpoint allowed_side_effect_intents must not be empty",
                    ));
                }
                if endpoint.timeout_ms == 0 || endpoint.timeout_ms > 120_000 {
                    return Err(ConnectorExecutionError::new(
                        "endpoint_config_invalid",
                        "endpoint timeout_ms must be in range 1..=120000",
                    ));
                }
                if endpoint.max_retries > 3 {
                    return Err(ConnectorExecutionError::new(
                        "endpoint_config_invalid",
                        "endpoint max_retries must be <= 3",
                    ));
                }
            }
        }
        for target in &self.provider_connector_registry {
            if target.connector_ref.trim().is_empty() {
                return Err(ConnectorExecutionError::new(
                    "provider_connector_config_invalid",
                    "connector_ref must not be empty",
                ));
            }
            if target.provider.trim().is_empty() {
                return Err(ConnectorExecutionError::new(
                    "provider_connector_config_invalid",
                    "provider must not be empty",
                ));
            }
            if target.operation.trim().is_empty() {
                return Err(ConnectorExecutionError::new(
                    "provider_connector_config_invalid",
                    "operation must not be empty",
                ));
            }
            if target.required_scopes.is_empty() {
                return Err(ConnectorExecutionError::new(
                    "provider_connector_config_invalid",
                    "required_scopes must not be empty",
                ));
            }
            if target.allowed_operations.is_empty() {
                return Err(ConnectorExecutionError::new(
                    "provider_connector_config_invalid",
                    "allowed_operations must not be empty",
                ));
            }
            if !target.secret_refs.contains_key("bot_token") {
                return Err(ConnectorExecutionError::new(
                    "provider_connector_config_invalid",
                    "telegram provider target requires bot_token secret ref",
                ));
            }
            if !target.secret_refs.contains_key("chat_id") && target.target_resolver.is_none() {
                return Err(ConnectorExecutionError::new(
                    "provider_connector_config_invalid",
                    "telegram provider target requires chat_id secret ref or target_resolver",
                ));
            }
        }
        for target in &self.target_registry {
            if target.target_ref.trim().is_empty() {
                return Err(ConnectorExecutionError::new(
                    "target_registry_invalid",
                    "target_ref must not be empty",
                ));
            }
            if target.connector_id.trim().is_empty() {
                return Err(ConnectorExecutionError::new(
                    "target_registry_invalid",
                    "connector_id must not be empty",
                ));
            }
            if target.required_scopes.is_empty() {
                return Err(ConnectorExecutionError::new(
                    "target_registry_invalid",
                    "required_scopes must not be empty",
                ));
            }
        }
        if self.timeout_ms == 0 || self.timeout_ms > 120_000 {
            return Err(ConnectorExecutionError::new(
                "connector_config_invalid",
                "timeout_ms must be in range 1..=120000",
            ));
        }
        if self.max_retries > 3 {
            return Err(ConnectorExecutionError::new(
                "connector_config_invalid",
                "max_retries must be <= 3",
            ));
        }
        Ok(())
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
pub trait ConnectorExecutor: Send + Sync {
    async fn execute(
        &self,
        request: ConnectorExecutionRequest,
    ) -> Result<ConnectorExecutionResult, ConnectorExecutionError>;
}

#[derive(Debug, Clone)]
pub struct DeterministicLocalConnectorExecutor {
    config: ConnectorExecutionConfig,
}

impl DeterministicLocalConnectorExecutor {
    pub fn new(config: ConnectorExecutionConfig) -> Self {
        Self { config }
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl ConnectorExecutor for DeterministicLocalConnectorExecutor {
    async fn execute(
        &self,
        request: ConnectorExecutionRequest,
    ) -> Result<ConnectorExecutionResult, ConnectorExecutionError> {
        if !self
            .config
            .allowed_connectors
            .iter()
            .any(|allowed| allowed == &request.connector_id)
        {
            return Err(ConnectorExecutionError::new(
                "connector_denied",
                "connector_id is not allowed",
            ));
        }
        if request.connector_id == "connector.http.generic.v1" {
            return self.execute_deterministic_http_generic(request).await;
        }
        if request.connector_id == "provider.connector.v1" {
            return self.execute_deterministic_provider_connector(request).await;
        }
        if !self
            .config
            .allowed_side_effect_intents
            .iter()
            .any(|intent| *intent == request.side_effect_intent)
        {
            return Err(ConnectorExecutionError::new(
                "connector_side_effect_denied",
                "side_effect_intent is not allowed",
            ));
        }

        let call = ConnectorCall {
            connector_id: request.connector_id,
            call_id: request.call_id,
            side_effect_intent: request.side_effect_intent,
            request: request.request,
            auth_context: request.auth_context,
            policy_result: request.policy_result,
            status: ConnectorCallStatus::Completed,
            response: json!({
                "connector_mode": "deterministic_local",
                "side_effect_performed": false,
                "outcome": "ok",
                "credential_ref_count": request.credential_refs.len()
            }),
            timing: ConnectorTiming {
                started_at: request.started_at,
                finished_at: Some("2026-05-23T10:30:01Z".to_string()),
                duration_ms: Some(1),
            },
        };

        Ok(ConnectorExecutionResult {
            call,
            guard_reason: "connector_guard_allow".to_string(),
        })
    }
}

impl DeterministicLocalConnectorExecutor {
    async fn execute_deterministic_http_generic(
        &self,
        request: ConnectorExecutionRequest,
    ) -> Result<ConnectorExecutionResult, ConnectorExecutionError> {
        if request.side_effect_intent != SideEffectIntent::ExternalNetwork {
            return Err(ConnectorExecutionError::new(
                "connector_side_effect_denied",
                "http_generic requires external_network side_effect_intent",
            ));
        }
        let endpoint_ref = request
            .request
            .get("endpoint_ref")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                ConnectorExecutionError::new(
                    "connector_request_invalid",
                    "endpoint_ref is required for deterministic local http_generic",
                )
            })?
            .to_string();
        let endpoint = self
            .config
            .endpoint_registry
            .iter()
            .find(|entry| entry.endpoint_ref == endpoint_ref)
            .ok_or_else(|| {
                ConnectorExecutionError::new(
                    "endpoint_unknown",
                    "endpoint_ref is not configured in runtime allowlist",
                )
            })?;
        if !endpoint.enabled {
            return Err(ConnectorExecutionError::new(
                "endpoint_disabled",
                "endpoint_ref is disabled",
            ));
        }
        if !endpoint
            .allowed_side_effect_intents
            .iter()
            .any(|intent| *intent == request.side_effect_intent)
        {
            return Err(ConnectorExecutionError::new(
                "endpoint_intent_denied",
                "endpoint_ref does not allow side_effect_intent",
            ));
        }

        let method = request
            .request
            .get("method")
            .and_then(|value| value.as_str())
            .unwrap_or("POST")
            .to_ascii_uppercase();
        if method != "GET" && method != "POST" {
            return Err(ConnectorExecutionError::new(
                "endpoint_method_denied",
                "http_generic supports only GET/POST in V0",
            ));
        }
        if !endpoint.allowed_methods.iter().any(|allowed| allowed == &method) {
            return Err(ConnectorExecutionError::new(
                "endpoint_method_denied",
                "method is not allowlisted for endpoint_ref",
            ));
        }
        let scopes = auth_scopes_from_context(&request.auth_context);
        if let Some(required) = endpoint
            .required_scopes
            .iter()
            .find(|required| !scopes.iter().any(|scope| scope == *required))
        {
            return Err(ConnectorExecutionError::new(
                "endpoint_scope_denied",
                &format!("required scope missing: {required}"),
            ));
        }

        let body_preview = request
            .request
            .get("body")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let call = ConnectorCall {
            connector_id: request.connector_id,
            call_id: request.call_id,
            side_effect_intent: request.side_effect_intent,
            request: request.request,
            auth_context: request.auth_context,
            policy_result: request.policy_result,
            status: ConnectorCallStatus::Completed,
            response: json!({
                "connector_mode": "deterministic_local_http_generic",
                "side_effect_performed": false,
                "provider": "deterministic_local_http_generic",
                "endpoint_ref": endpoint_ref,
                "resolved_url": endpoint.url,
                "method": method,
                "attempts": 1,
                "max_retries": endpoint.max_retries,
                "timeout_ms": endpoint.timeout_ms,
                "response_preview": body_preview,
                "credential_ref_count": request.credential_refs.len()
            }),
            timing: ConnectorTiming {
                started_at: request.started_at,
                finished_at: Some("2026-05-23T10:30:01Z".to_string()),
                duration_ms: Some(1),
            },
        };

        Ok(ConnectorExecutionResult {
            call,
            guard_reason: "connector_guard_allow_deterministic_http_generic".to_string(),
        })
    }

    async fn execute_deterministic_provider_connector(
        &self,
        request: ConnectorExecutionRequest,
    ) -> Result<ConnectorExecutionResult, ConnectorExecutionError> {
        if request.side_effect_intent != SideEffectIntent::ExternalNetwork {
            return Err(ConnectorExecutionError::new(
                "connector_side_effect_denied",
                "provider connector requires external_network side_effect_intent",
            ));
        }
        let connector_ref = request
            .request
            .get("connector_ref")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                ConnectorExecutionError::new(
                    "connector_ref_not_allowed",
                    "connector_ref is required for provider connector",
                )
            })?
            .to_string();
        let target = self
            .config
            .provider_connector_registry
            .iter()
            .find(|entry| entry.connector_ref == connector_ref)
            .ok_or_else(|| {
                ConnectorExecutionError::new(
                    "connector_ref_not_allowed",
                    "connector_ref is not configured in provider connector registry",
                )
            })?;
        if !target.enabled {
            return Err(ConnectorExecutionError::new(
                "connector_ref_disabled",
                "connector_ref is disabled",
            ));
        }
        let scopes = auth_scopes_from_context(&request.auth_context);
        if let Some(required) = target
            .required_scopes
            .iter()
            .find(|required| !scopes.iter().any(|scope| scope == *required))
        {
            return Err(ConnectorExecutionError::new(
                "connector_scope_denied",
                &format!("required scope missing: {required}"),
            ));
        }
        let operation_id = request
            .request
            .get("operation_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        if !target
            .allowed_operations
            .iter()
            .any(|allowed| allowed == "*" || allowed == &operation_id)
        {
            return Err(ConnectorExecutionError::new(
                "connector_operation_not_allowed",
                "operation_id is not allowlisted for connector_ref",
            ));
        }
        let provider = request
            .request
            .get("provider")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if provider != target.provider {
            return Err(ConnectorExecutionError::new(
                "provider_not_supported",
                "provider is not supported for connector_ref",
            ));
        }
        let operation = request
            .request
            .get("operation")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if operation != target.operation {
            return Err(ConnectorExecutionError::new(
                "provider_operation_not_supported",
                "provider operation is not supported for connector_ref",
            ));
        }

        let request_payload = request.request.clone();
        let text = request_payload
            .get("text")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("MOVA contract-run notification")
            .to_string();
        let call = ConnectorCall {
            connector_id: request.connector_id,
            call_id: request.call_id,
            side_effect_intent: request.side_effect_intent,
            request: request_payload,
            auth_context: request.auth_context,
            policy_result: request.policy_result,
            status: ConnectorCallStatus::Completed,
            response: json!({
                "connector_mode": "deterministic_fake_provider_connector",
                "provider": target.provider,
                "connector_ref": connector_ref,
                "operation": target.operation,
                "response_preview": {
                    "ok": true,
                    "message_id": 1001,
                    "text_preview": text
                },
                "attempts": 1,
                "timeout_ms": self.config.timeout_ms
            }),
            timing: ConnectorTiming {
                started_at: request.started_at,
                finished_at: Some("2026-05-23T10:30:01Z".to_string()),
                duration_ms: Some(1),
            },
        };

        Ok(ConnectorExecutionResult {
            call,
            guard_reason: "connector_guard_allow_deterministic_provider_connector".to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct OfflineStubConnectorExecutor {
    config: ConnectorExecutionConfig,
}

impl OfflineStubConnectorExecutor {
    pub fn new(config: ConnectorExecutionConfig) -> Self {
        Self { config }
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl ConnectorExecutor for OfflineStubConnectorExecutor {
    async fn execute(
        &self,
        request: ConnectorExecutionRequest,
    ) -> Result<ConnectorExecutionResult, ConnectorExecutionError> {
        let rule = self
            .config
            .offline_stub_rules
            .iter()
            .find(|rule| rule.connector_id == request.connector_id)
            .ok_or_else(|| {
                ConnectorExecutionError::new(
                    "connector_stub_rule_missing",
                    "offline stub rule missing for connector_id",
                )
            })?;

        let call = ConnectorCall {
            connector_id: request.connector_id,
            call_id: request.call_id,
            side_effect_intent: request.side_effect_intent,
            request: request.request,
            auth_context: request.auth_context,
            policy_result: request.policy_result,
            status: rule.call_status,
            response: json!({
                "connector_mode": "offline_stub",
                "side_effect_performed": false,
                "outcome": "stubbed",
                "credential_ref_count": request.credential_refs.len()
            }),
            timing: ConnectorTiming {
                started_at: request.started_at,
                finished_at: Some("2026-05-23T10:30:01Z".to_string()),
                duration_ms: Some(1),
            },
        };

        Ok(ConnectorExecutionResult {
            call,
            guard_reason: rule.guard_reason.clone(),
        })
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
pub trait WebhookHttpClient: Send + Sync {
    async fn post_json(
        &self,
        url: &str,
        body: &Value,
    ) -> Result<WebhookHttpResult, ConnectorExecutionError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookHttpResult {
    pub status: u16,
    pub body_preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Value>,
    pub timeout_ms: u64,
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
pub trait GenericHttpClient: Send + Sync {
    async fn execute(
        &self,
        request: GenericHttpRequest,
    ) -> Result<WebhookHttpResult, ConnectorExecutionError>;
}

pub trait ConnectorSecretResolver: Send + Sync {
    fn resolve(&self, secret_ref: &str) -> Result<Option<String>, ConnectorExecutionError>;
}

#[derive(Debug, Clone)]
pub struct DisabledWebhookHttpClient;

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl WebhookHttpClient for DisabledWebhookHttpClient {
    async fn post_json(
        &self,
        _url: &str,
        _body: &Value,
    ) -> Result<WebhookHttpResult, ConnectorExecutionError> {
        Err(ConnectorExecutionError::new(
            "connector_provider_unavailable",
            "webhook provider is unavailable in this runtime",
        ))
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl GenericHttpClient for DisabledWebhookHttpClient {
    async fn execute(
        &self,
        _request: GenericHttpRequest,
    ) -> Result<WebhookHttpResult, ConnectorExecutionError> {
        Err(ConnectorExecutionError::new(
            "connector_provider_unavailable",
            "generic HTTP provider is unavailable in this runtime",
        ))
    }
}

#[derive(Debug, Clone)]
pub struct DisabledConnectorSecretResolver;

impl ConnectorSecretResolver for DisabledConnectorSecretResolver {
    fn resolve(&self, _secret_ref: &str) -> Result<Option<String>, ConnectorExecutionError> {
        Ok(None)
    }
}

#[derive(Clone)]
pub struct WebhookSiteConnectorExecutor {
    config: ConnectorExecutionConfig,
    http_client: Arc<dyn WebhookHttpClient>,
}

impl WebhookSiteConnectorExecutor {
    pub fn new(config: ConnectorExecutionConfig, http_client: Arc<dyn WebhookHttpClient>) -> Self {
        Self { config, http_client }
    }
}

#[derive(Clone)]
pub struct GenericHttpConnectorExecutor {
    config: ConnectorExecutionConfig,
    http_client: Arc<dyn GenericHttpClient>,
    secret_resolver: Arc<dyn ConnectorSecretResolver>,
}

impl GenericHttpConnectorExecutor {
    pub fn new(config: ConnectorExecutionConfig, http_client: Arc<dyn GenericHttpClient>) -> Self {
        Self {
            config,
            http_client,
            secret_resolver: Arc::new(DisabledConnectorSecretResolver),
        }
    }

    pub fn with_secret_resolver(
        config: ConnectorExecutionConfig,
        http_client: Arc<dyn GenericHttpClient>,
        secret_resolver: Arc<dyn ConnectorSecretResolver>,
    ) -> Self {
        Self {
            config,
            http_client,
            secret_resolver,
        }
    }
}

fn auth_scopes_from_context(auth_context: &Value) -> Vec<String> {
    auth_context
        .get("scopes")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn build_evidence_response(
    policy: EndpointEvidencePolicy,
    status: u16,
    preview: String,
    method: &str,
    endpoint_ref: &str,
    resolved_url: &str,
    attempts: u8,
    max_retries: u8,
    timeout_ms: u64,
    credential_ref_count: usize,
    last_non_2xx_http_status: Option<u16>,
) -> Value {
    let mut response = json!({
        "connector_mode": "http_generic",
        "side_effect_performed": true,
        "provider": "http.generic.v1",
        "endpoint_ref": endpoint_ref,
        "resolved_url": resolved_url,
        "method": method,
        "http_status": status,
        "attempts": attempts,
        "max_retries": max_retries,
        "timeout_ms": timeout_ms,
        "credential_ref_count": credential_ref_count,
        "last_non_2xx_http_status": last_non_2xx_http_status,
        "evidence_policy": policy
    });
    match policy {
        EndpointEvidencePolicy::StatusOnly => {}
        EndpointEvidencePolicy::SummaryOnly | EndpointEvidencePolicy::HeadersRedacted => {
            response["response_preview"] = json!(preview);
        }
        EndpointEvidencePolicy::BodyRedacted => {
            response["response_preview"] = json!("[REDACTED]");
        }
    }
    response
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl ConnectorExecutor for WebhookSiteConnectorExecutor {
    async fn execute(
        &self,
        request: ConnectorExecutionRequest,
    ) -> Result<ConnectorExecutionResult, ConnectorExecutionError> {
        if !self
            .config
            .allowed_connectors
            .iter()
            .any(|allowed| allowed == &request.connector_id)
        {
            return Err(ConnectorExecutionError::new(
                "connector_denied",
                "connector_id is not allowed",
            ));
        }
        if !self
            .config
            .allowed_side_effect_intents
            .iter()
            .any(|intent| *intent == request.side_effect_intent)
        {
            return Err(ConnectorExecutionError::new(
                "connector_side_effect_denied",
                "side_effect_intent is not allowed",
            ));
        }
        if request.side_effect_intent != SideEffectIntent::ExternalNetwork {
            return Err(ConnectorExecutionError::new(
                "connector_side_effect_denied",
                "webhook_site requires external_network side_effect_intent",
            ));
        }

        let target_url = request
            .request
            .get("target_url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ConnectorExecutionError::new(
                    "connector_request_invalid",
                    "target_url is required for webhook_site connector",
                )
            })?;

        if target_url != self.config.allowed_webhook_urls[0] {
            return Err(ConnectorExecutionError::new(
                "connector_target_denied",
                "target_url is not in webhook allowlist",
            ));
        }
        let parsed = Url::parse(target_url).map_err(|_| {
            ConnectorExecutionError::new(
                "connector_request_invalid",
                "target_url must be a valid absolute URL",
            )
        })?;
        if parsed.scheme() != "https" || parsed.host_str() != Some("webhook.site") {
            return Err(ConnectorExecutionError::new(
                "connector_target_denied",
                "target_url must be https://webhook.site/*",
            ));
        }

        let outbound_payload = json!({
            "provider": "webhook_site",
            "run_id": request.request.get("run_id").and_then(|v| v.as_str()).unwrap_or_default(),
            "correlation_id": request.request.get("correlation_id").and_then(|v| v.as_str()).unwrap_or_default(),
            "call_id": request.call_id,
            "connector_id": request.connector_id,
            "trace_ref": request.request.get("trace_ref").and_then(|v| v.as_str()).unwrap_or_default(),
        });

        let mut attempt: u8 = 0;
        let mut last_http_status: Option<u16> = None;
        let http_result = loop {
            let result = self.http_client.post_json(target_url, &outbound_payload).await;
            match result {
                Ok(r) if (200..300).contains(&r.status) => break r,
                Ok(r) => {
                    last_http_status = Some(r.status);
                    if r.status >= 500 && attempt < self.config.max_retries {
                        attempt += 1;
                        continue;
                    }
                    return Err(ConnectorExecutionError::new(
                        "connector_provider_http_failed",
                        &format!("webhook responded with status {}", r.status),
                    ));
                }
                Err(err) => {
                    if (err.code == "connector_provider_network_failed"
                        || err.code == "connector_provider_timeout")
                        && attempt < self.config.max_retries
                    {
                        attempt += 1;
                        continue;
                    }
                    return Err(err);
                }
            }
        };

        let call = ConnectorCall {
            connector_id: request.connector_id,
            call_id: request.call_id,
            side_effect_intent: request.side_effect_intent,
            request: request.request,
            auth_context: request.auth_context,
            policy_result: request.policy_result,
            status: ConnectorCallStatus::Completed,
            response: json!({
                "connector_mode": "webhook_site",
                "side_effect_performed": true,
                "provider": "webhook.site",
                "http_status": http_result.status,
                "attempts": attempt + 1,
                "max_retries": self.config.max_retries,
                "timeout_ms": self.config.timeout_ms,
                "request_correlation_id": outbound_payload["correlation_id"],
                "run_id": outbound_payload["run_id"],
                "response_preview": http_result.body_preview,
                "last_non_2xx_http_status": last_http_status
            }),
            timing: ConnectorTiming {
                started_at: request.started_at,
                finished_at: Some("2026-05-23T10:30:01Z".to_string()),
                duration_ms: Some(1),
            },
        };

        Ok(ConnectorExecutionResult {
            call,
            guard_reason: "connector_guard_allow_webhook_site".to_string(),
        })
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl ConnectorExecutor for GenericHttpConnectorExecutor {
    async fn execute(
        &self,
        request: ConnectorExecutionRequest,
    ) -> Result<ConnectorExecutionResult, ConnectorExecutionError> {
        if !self
            .config
            .allowed_connectors
            .iter()
            .any(|allowed| allowed == &request.connector_id)
        {
            return Err(ConnectorExecutionError::new(
                "connector_denied",
                "connector_id is not allowed",
            ));
        }
        if !self
            .config
            .allowed_side_effect_intents
            .iter()
            .any(|intent| *intent == request.side_effect_intent)
        {
            return Err(ConnectorExecutionError::new(
                "connector_side_effect_denied",
                "side_effect_intent is not allowed",
            ));
        }
        if request.side_effect_intent != SideEffectIntent::ExternalNetwork {
            return Err(ConnectorExecutionError::new(
                "connector_side_effect_denied",
                "connector requires external_network side_effect_intent",
            ));
        }
        if request.connector_id == "provider.connector.v1" {
            return self.execute_provider_connector(request).await;
        }

        let endpoint_ref = request
            .request
            .get("endpoint_ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ConnectorExecutionError::new(
                    "connector_request_invalid",
                    "endpoint_ref is required for http_generic connector",
                )
            })?
            .to_string();
        let endpoint = self
            .config
            .endpoint_registry
            .iter()
            .find(|entry| entry.endpoint_ref == endpoint_ref)
            .ok_or_else(|| {
                ConnectorExecutionError::new(
                    "endpoint_unknown",
                    "endpoint_ref is not configured in runtime allowlist",
                )
            })?;
        if !endpoint.enabled {
            return Err(ConnectorExecutionError::new(
                "endpoint_disabled",
                "endpoint_ref is disabled",
            ));
        }

        if !endpoint
            .allowed_side_effect_intents
            .iter()
            .any(|i| *i == request.side_effect_intent)
        {
            return Err(ConnectorExecutionError::new(
                "endpoint_intent_denied",
                "endpoint_ref does not allow side_effect_intent",
            ));
        }

        let method = request
            .request
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("POST")
            .to_ascii_uppercase();
        if method != "GET" && method != "POST" {
            return Err(ConnectorExecutionError::new(
                "endpoint_method_denied",
                "http_generic supports only GET/POST in V0",
            ));
        }
        if !endpoint.allowed_methods.iter().any(|m| m == &method) {
            return Err(ConnectorExecutionError::new(
                "endpoint_method_denied",
                "method is not allowlisted for endpoint_ref",
            ));
        }
        let scopes = auth_scopes_from_context(&request.auth_context);
        let scope_denied = endpoint
            .required_scopes
            .iter()
            .find(|required| !scopes.iter().any(|scope| scope == *required));
        if let Some(required) = scope_denied {
            return Err(ConnectorExecutionError::new(
                "endpoint_scope_denied",
                &format!("required scope missing: {required}"),
            ));
        }

        let headers = request
            .request
            .get("headers")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.to_string(), s.to_string())))
                    .collect::<HashMap<String, String>>()
            })
            .unwrap_or_default();
        let body = request.request.get("body").cloned();

        let mut attempt: u8 = 0;
        let mut last_http_status: Option<u16> = None;
        let http_result = loop {
            let result = self
                .http_client
                .execute(GenericHttpRequest {
                    method: method.clone(),
                    url: endpoint.url.clone(),
                    headers: headers.clone(),
                    body: body.clone(),
                    timeout_ms: endpoint.timeout_ms,
                })
                .await;
            match result {
                Ok(r) if (200..300).contains(&r.status) => break r,
                Ok(r) => {
                    last_http_status = Some(r.status);
                    if r.status >= 500 && attempt < endpoint.max_retries {
                        attempt += 1;
                        continue;
                    }
                    return Err(ConnectorExecutionError::new(
                        "connector_provider_http_failed",
                        &format!("http_generic responded with status {}", r.status),
                    ));
                }
                Err(err) => {
                    if (err.code == "connector_provider_network_failed"
                        || err.code == "connector_provider_timeout")
                        && attempt < endpoint.max_retries
                    {
                        attempt += 1;
                        continue;
                    }
                    return Err(err);
                }
            }
        };

        let call = ConnectorCall {
            connector_id: request.connector_id,
            call_id: request.call_id,
            side_effect_intent: request.side_effect_intent,
            request: request.request,
            auth_context: request.auth_context,
            policy_result: request.policy_result,
            status: ConnectorCallStatus::Completed,
            response: build_evidence_response(
                endpoint.evidence_policy,
                http_result.status,
                http_result.body_preview,
                &method,
                &endpoint_ref,
                &endpoint.url,
                attempt + 1,
                endpoint.max_retries,
                endpoint.timeout_ms,
                request.credential_refs.len(),
                last_http_status,
            ),
            timing: ConnectorTiming {
                started_at: request.started_at,
                finished_at: Some("2026-05-23T10:30:01Z".to_string()),
                duration_ms: Some(1),
            },
        };

        Ok(ConnectorExecutionResult {
            call,
            guard_reason: "connector_guard_allow_http_generic".to_string(),
        })
    }
}

impl GenericHttpConnectorExecutor {
    async fn execute_provider_connector(
        &self,
        request: ConnectorExecutionRequest,
    ) -> Result<ConnectorExecutionResult, ConnectorExecutionError> {
        let connector_ref = request
            .request
            .get("connector_ref")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                ConnectorExecutionError::new(
                    "connector_ref_not_allowed",
                    "connector_ref is required for provider connector",
                )
            })?
            .to_string();
        let target = self
            .config
            .provider_connector_registry
            .iter()
            .find(|entry| entry.connector_ref == connector_ref)
            .ok_or_else(|| {
                ConnectorExecutionError::new(
                    "connector_ref_not_allowed",
                    "connector_ref is not configured in provider connector registry",
                )
            })?;
        if !target.enabled {
            return Err(ConnectorExecutionError::new(
                "connector_ref_disabled",
                "connector_ref is disabled",
            ));
        }
        let scopes = auth_scopes_from_context(&request.auth_context);
        if let Some(required) = target
            .required_scopes
            .iter()
            .find(|required| !scopes.iter().any(|scope| scope == *required))
        {
            return Err(ConnectorExecutionError::new(
                "connector_scope_denied",
                &format!("required scope missing: {required}"),
            ));
        }
        let operation_id = request
            .request
            .get("operation_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        if !target
            .allowed_operations
            .iter()
            .any(|allowed| allowed == "*" || allowed == &operation_id)
        {
            return Err(ConnectorExecutionError::new(
                "connector_operation_not_allowed",
                "operation_id is not allowlisted for connector_ref",
            ));
        }
        let provider = request
            .request
            .get("provider")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if provider != target.provider {
            return Err(ConnectorExecutionError::new(
                "provider_not_supported",
                "provider is not supported for connector_ref",
            ));
        }
        let operation = request
            .request
            .get("operation")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if operation != target.operation {
            return Err(ConnectorExecutionError::new(
                "provider_operation_not_supported",
                "provider operation is not supported for connector_ref",
            ));
        }
        match (provider, operation) {
            ("telegram", "send_message") => self.execute_telegram_send_message(target, request).await,
            ("telegram", _) => Err(ConnectorExecutionError::new(
                "provider_operation_not_supported",
                "telegram operation is not supported",
            )),
            _ => Err(ConnectorExecutionError::new(
                "provider_not_supported",
                "provider is not supported",
            )),
        }
    }

    async fn execute_telegram_send_message(
        &self,
        target: &ProviderConnectorRegistryEntry,
        request: ConnectorExecutionRequest,
    ) -> Result<ConnectorExecutionResult, ConnectorExecutionError> {
        let token_secret_ref = target
            .secret_refs
            .get("bot_token")
            .ok_or_else(|| ConnectorExecutionError::new("connector_secret_missing", "bot_token secret ref is missing"))?;
        let bot_token = self
            .secret_resolver
            .resolve(token_secret_ref)?
            .ok_or_else(|| ConnectorExecutionError::new("connector_secret_missing", "required secret missing: TELEGRAM_BOT_TOKEN"))?;
        let chat_id = request
            .request
            .get("resolved_chat_id")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.to_string())
            .or_else(|| {
                target
                    .secret_refs
                    .get("chat_id")
                    .and_then(|chat_id_secret_ref| self.secret_resolver.resolve(chat_id_secret_ref).ok().flatten())
            })
            .ok_or_else(|| ConnectorExecutionError::new("connector_secret_missing", "required chat target is missing"))?;
        let text = request
            .request
            .get("text")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("MOVA contract-run notification")
            .to_string();
        let payload = json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML",
            "disable_web_page_preview": true
        });
        let url = format!("https://api.telegram.org/bot{bot_token}/sendMessage");
        let http_result = self
            .http_client
            .execute(GenericHttpRequest {
                method: "POST".to_string(),
                url,
                headers: HashMap::new(),
                body: Some(payload),
                timeout_ms: self.config.timeout_ms,
            })
            .await?;
        if !(200..300).contains(&http_result.status) {
            let code = match http_result.status {
                400 => "connector_provider_request_failed",
                401 | 403 => "connector_provider_auth_failed",
                429 => "connector_provider_rate_limited",
                _ => "connector_provider_http_failed",
            };
            return Err(ConnectorExecutionError::new(
                code,
                &format!("telegram sendMessage responded with status {}", http_result.status),
            ));
        }
        let parsed = serde_json::from_str::<Value>(&http_result.body_preview).unwrap_or_else(|_| json!({}));
        let ok = parsed
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let message_id = parsed
            .get("result")
            .and_then(|value| value.get("message_id"))
            .cloned()
            .unwrap_or_else(|| json!(Value::Null));

        let call = ConnectorCall {
            connector_id: request.connector_id,
            call_id: request.call_id,
            side_effect_intent: request.side_effect_intent,
            request: request.request,
            auth_context: request.auth_context,
            policy_result: request.policy_result,
            status: ConnectorCallStatus::Completed,
            response: json!({
                "connector_mode": "provider_connector_proxy",
                "provider": target.provider,
                "connector_ref": target.connector_ref,
                "operation": target.operation,
                "response_preview": {
                    "ok": ok,
                    "message_id": message_id
                },
                "attempts": 1,
                "timeout_ms": self.config.timeout_ms
            }),
            timing: ConnectorTiming {
                started_at: request.started_at,
                finished_at: Some("2026-05-23T10:30:01Z".to_string()),
                duration_ms: Some(1),
            },
        };

        Ok(ConnectorExecutionResult {
            call,
            guard_reason: "connector_guard_allow_provider_connector".to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct FailingConnectorExecutor {
    err: ConnectorExecutionError,
}

impl FailingConnectorExecutor {
    pub fn new(err: ConnectorExecutionError) -> Self {
        Self { err }
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl ConnectorExecutor for FailingConnectorExecutor {
    async fn execute(
        &self,
        _request: ConnectorExecutionRequest,
    ) -> Result<ConnectorExecutionResult, ConnectorExecutionError> {
        Err(self.err.clone())
    }
}

pub fn create_connector_executor(config: &ConnectorExecutionConfig) -> Box<dyn ConnectorExecutor> {
    match config.validate() {
        Ok(()) if config.adapter_kind == "deterministic_local" => {
            Box::new(DeterministicLocalConnectorExecutor::new(config.clone()))
        }
        Ok(()) if config.adapter_kind == "offline_stub" => {
            Box::new(OfflineStubConnectorExecutor::new(config.clone()))
        }
        Ok(()) if config.adapter_kind == "webhook_site" => Box::new(
            WebhookSiteConnectorExecutor::new(config.clone(), Arc::new(DisabledWebhookHttpClient)),
        ),
        Ok(()) if config.adapter_kind == "http_generic" => Box::new(
            GenericHttpConnectorExecutor::new(config.clone(), Arc::new(DisabledWebhookHttpClient)),
        ),
        Ok(()) => Box::new(FailingConnectorExecutor::new(ConnectorExecutionError::new(
            "connector_config_invalid",
            "unsupported adapter_kind",
        ))),
        Err(err) => Box::new(FailingConnectorExecutor::new(err)),
    }
}

pub fn connector_side_effects_enabled() -> bool {
    CONNECTOR_SIDE_EFFECTS_ENABLED
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::AdmissionDecision;

    fn request(intent: SideEffectIntent, connector_id: &str) -> ConnectorExecutionRequest {
        ConnectorExecutionRequest {
            connector_id: connector_id.to_string(),
            call_id: "call_01".to_string(),
            side_effect_intent: intent,
            request: json!({"doc_id":"1"}),
            auth_context: json!({}),
            credential_refs: Vec::new(),
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T09:00:00Z".to_string(),
        }
    }

    #[tokio::test]
    async fn deterministic_local_executor_allows_none_intent() {
        let exec =
            DeterministicLocalConnectorExecutor::new(ConnectorExecutionConfig::deterministic_local_default());
        let result = exec
            .execute(request(SideEffectIntent::None, "connector.docs.v1"))
            .await
            .unwrap();
        assert_eq!(result.call.status, ConnectorCallStatus::Completed);
        assert_eq!(result.call.side_effect_intent, SideEffectIntent::None);
    }

    #[tokio::test]
    async fn deterministic_local_executor_denies_external_network() {
        let exec =
            DeterministicLocalConnectorExecutor::new(ConnectorExecutionConfig::deterministic_local_default());
        let err = exec
            .execute(request(
                SideEffectIntent::ExternalNetwork,
                "connector.docs.v1",
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code, "connector_side_effect_denied");
    }

    #[tokio::test]
    async fn deterministic_local_executor_allows_http_generic_with_allowlisted_endpoint() {
        let exec =
            DeterministicLocalConnectorExecutor::new(ConnectorExecutionConfig::deterministic_local_default());
        let result = exec
            .execute(ConnectorExecutionRequest {
                connector_id: "connector.http.generic.v1".to_string(),
                call_id: "call_http_generic_01".to_string(),
                side_effect_intent: SideEffectIntent::ExternalNetwork,
                request: json!({
                    "endpoint_ref": "webhook_site_test",
                    "method": "POST",
                    "body": {"message":"ok"}
                }),
                auth_context: json!({"scopes":["actions.run"]}),
                credential_refs: Vec::new(),
                policy_result: PolicySummary {
                    decision: AdmissionDecision::Allow,
                    policy_version: "policy.default.v0".to_string(),
                    reason_code: "authorized".to_string(),
                },
                started_at: "2026-05-23T09:00:00Z".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(result.call.status, ConnectorCallStatus::Completed);
        assert_eq!(result.call.response["provider"], "deterministic_local_http_generic");
        assert_eq!(result.call.response["endpoint_ref"], "webhook_site_test");
    }

    #[tokio::test]
    async fn deterministic_local_executor_allows_contract_run_endpoint_with_contracts_scope() {
        let exec =
            DeterministicLocalConnectorExecutor::new(ConnectorExecutionConfig::deterministic_local_default());
        let result = exec
            .execute(ConnectorExecutionRequest {
                connector_id: "connector.http.generic.v1".to_string(),
                call_id: "call_http_generic_contract_run_01".to_string(),
                side_effect_intent: SideEffectIntent::ExternalNetwork,
                request: json!({
                    "endpoint_ref": "webhook_site_contract_run_test",
                    "method": "POST",
                    "body": {"message":"ok"}
                }),
                auth_context: json!({"scopes":["contracts.run"]}),
                credential_refs: Vec::new(),
                policy_result: PolicySummary {
                    decision: AdmissionDecision::Allow,
                    policy_version: "policy.default.v0".to_string(),
                    reason_code: "authorized".to_string(),
                },
                started_at: "2026-05-23T09:00:00Z".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(result.call.status, ConnectorCallStatus::Completed);
        assert_eq!(result.call.response["provider"], "deterministic_local_http_generic");
        assert_eq!(
            result.call.response["endpoint_ref"],
            "webhook_site_contract_run_test"
        );
    }

    #[tokio::test]
    async fn deterministic_local_executor_denies_unknown_connector() {
        let exec =
            DeterministicLocalConnectorExecutor::new(ConnectorExecutionConfig::deterministic_local_default());
        let err = exec
            .execute(request(SideEffectIntent::None, "connector.unknown"))
            .await
            .unwrap_err();
        assert_eq!(err.code, "connector_denied");
    }

    #[tokio::test]
    async fn offline_stub_executor_uses_stub_rule() {
        let config = ConnectorExecutionConfig {
            adapter_kind: "offline_stub".to_string(),
            allowed_connectors: vec![],
            allowed_side_effect_intents: vec![],
            offline_stub_rules: vec![OfflineStubRule {
                connector_id: "connector.docs.v1".to_string(),
                call_status: ConnectorCallStatus::Completed,
                guard_reason: "stub_allow".to_string(),
            }],
            allowed_webhook_urls: vec![],
            endpoint_registry: vec![],
            provider_connector_registry: vec![],
            target_registry: vec![],
            timeout_ms: 10_000,
            max_retries: 0,
        };
        let exec = OfflineStubConnectorExecutor::new(config);
        let result = exec
            .execute(request(SideEffectIntent::LocalOnly, "connector.docs.v1"))
            .await
            .unwrap();
        assert_eq!(result.guard_reason, "stub_allow");
        assert_eq!(result.call.status, ConnectorCallStatus::Completed);
    }

    #[test]
    fn connector_status_serialization_is_snake_case() {
        let value = serde_json::to_string(&ConnectorCallStatus::Completed).unwrap();
        assert_eq!(value, "\"completed\"");
    }
}
