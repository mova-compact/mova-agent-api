//! Connector execution boundary for MOVA Agent API V0.
//!
//! This module provides controlled connector execution semantics with explicit
//! side-effect intent guardrails and provider-agnostic adapter contracts.

use crate::policy::PolicySummary;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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
        }
    }

    pub fn validate(&self) -> Result<(), ConnectorExecutionError> {
        if self.adapter_kind.trim().is_empty() {
            return Err(ConnectorExecutionError::new(
                "connector_config_invalid",
                "adapter_kind is required",
            ));
        }
        if self.adapter_kind != "deterministic_local" && self.adapter_kind != "offline_stub" {
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
        Ok(())
    }
}

pub trait ConnectorExecutor: Send + Sync {
    fn execute(&self, request: ConnectorExecutionRequest) -> Result<ConnectorExecutionResult, ConnectorExecutionError>;
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

impl ConnectorExecutor for DeterministicLocalConnectorExecutor {
    fn execute(&self, request: ConnectorExecutionRequest) -> Result<ConnectorExecutionResult, ConnectorExecutionError> {
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
                "outcome": "ok"
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

impl ConnectorExecutor for OfflineStubConnectorExecutor {
    fn execute(&self, request: ConnectorExecutionRequest) -> Result<ConnectorExecutionResult, ConnectorExecutionError> {
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
                "outcome": "stubbed"
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

#[derive(Debug, Clone)]
pub struct FailingConnectorExecutor {
    err: ConnectorExecutionError,
}

impl FailingConnectorExecutor {
    pub fn new(err: ConnectorExecutionError) -> Self {
        Self { err }
    }
}

impl ConnectorExecutor for FailingConnectorExecutor {
    fn execute(&self, _request: ConnectorExecutionRequest) -> Result<ConnectorExecutionResult, ConnectorExecutionError> {
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
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T09:00:00Z".to_string(),
        }
    }

    #[test]
    fn deterministic_local_executor_allows_none_intent() {
        let exec = DeterministicLocalConnectorExecutor::new(
            ConnectorExecutionConfig::deterministic_local_default(),
        );
        let result = exec
            .execute(request(SideEffectIntent::None, "connector.docs.v1"))
            .unwrap();
        assert_eq!(result.call.status, ConnectorCallStatus::Completed);
        assert_eq!(result.call.side_effect_intent, SideEffectIntent::None);
    }

    #[test]
    fn deterministic_local_executor_denies_external_network() {
        let exec = DeterministicLocalConnectorExecutor::new(
            ConnectorExecutionConfig::deterministic_local_default(),
        );
        let err = exec
            .execute(request(
                SideEffectIntent::ExternalNetwork,
                "connector.docs.v1",
            ))
            .unwrap_err();
        assert_eq!(err.code, "connector_side_effect_denied");
    }

    #[test]
    fn deterministic_local_executor_denies_unknown_connector() {
        let exec = DeterministicLocalConnectorExecutor::new(
            ConnectorExecutionConfig::deterministic_local_default(),
        );
        let err = exec
            .execute(request(SideEffectIntent::None, "connector.unknown"))
            .unwrap_err();
        assert_eq!(err.code, "connector_denied");
    }

    #[test]
    fn offline_stub_executor_uses_stub_rule() {
        let config = ConnectorExecutionConfig {
            adapter_kind: "offline_stub".to_string(),
            allowed_connectors: vec![],
            allowed_side_effect_intents: vec![],
            offline_stub_rules: vec![OfflineStubRule {
                connector_id: "connector.docs.v1".to_string(),
                call_status: ConnectorCallStatus::Completed,
                guard_reason: "stub_allow".to_string(),
            }],
        };
        let exec = OfflineStubConnectorExecutor::new(config);
        let result = exec
            .execute(request(SideEffectIntent::LocalOnly, "connector.docs.v1"))
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
