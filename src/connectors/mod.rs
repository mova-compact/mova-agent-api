//! Connector execution boundary for MOVA Agent API V0.
//!
//! This module provides controlled connector execution semantics with explicit
//! side-effect intent guardrails and provider-agnostic adapter contracts.

use crate::policy::PolicySummary;
use crate::secrets::SecretRef;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
pub struct ConnectorExecutionConfig {
    pub adapter_kind: String,
    pub allowed_connectors: Vec<String>,
    pub allowed_side_effect_intents: Vec<SideEffectIntent>,
    pub offline_stub_rules: Vec<OfflineStubRule>,
    pub allowed_webhook_urls: Vec<String>,
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
            allowed_connectors: vec!["connector.docs.v1".to_string()],
            allowed_side_effect_intents: vec![SideEffectIntent::None, SideEffectIntent::LocalOnly],
            offline_stub_rules: Vec::new(),
            allowed_webhook_urls: Vec::new(),
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

        let http_result = self.http_client.post_json(target_url, &outbound_payload).await?;
        if !(200..300).contains(&http_result.status) {
            return Err(ConnectorExecutionError::new(
                "connector_provider_http_failed",
                &format!("webhook responded with status {}", http_result.status),
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
                "connector_mode": "webhook_site",
                "side_effect_performed": true,
                "provider": "webhook.site",
                "http_status": http_result.status,
                "request_correlation_id": outbound_payload["correlation_id"],
                "run_id": outbound_payload["run_id"],
                "response_preview": http_result.body_preview
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
