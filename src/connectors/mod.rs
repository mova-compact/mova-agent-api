//! Connector proxy skeleton for MOVA Agent API V0.
//!
//! Phase 4 scope:
//! - connector-call boundary shape
//! - deterministic local adapter behavior only
//! - no network side effects

use crate::policy::PolicySummary;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorCallStatus {
    Pending,
    Allowed,
    Blocked,
    Failed,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorTiming {
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorCall {
    pub connector_id: String,
    pub call_id: String,
    pub request: Value,
    pub auth_context: Value,
    pub policy_result: PolicySummary,
    pub status: ConnectorCallStatus,
    pub response: Value,
    pub timing: ConnectorTiming,
}

pub fn build_connector_call(
    connector_id: String,
    call_id: String,
    policy_result: PolicySummary,
    started_at: String,
) -> ConnectorCall {
    ConnectorCall {
        connector_id,
        call_id,
        request: Value::Object(Default::default()),
        auth_context: Value::Object(Default::default()),
        policy_result,
        status: ConnectorCallStatus::Pending,
        response: Value::Object(Default::default()),
        timing: ConnectorTiming {
            started_at,
            finished_at: None,
            duration_ms: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::AdmissionDecision;

    #[test]
    fn connector_call_builder_creates_pending_shape() {
        let policy_result = PolicySummary {
            decision: AdmissionDecision::Allow,
            policy_version: "policy.default.v0".to_string(),
            reason_code: "ok".to_string(),
        };
        let call = build_connector_call(
            "connector.docs.v1".to_string(),
            "call_01".to_string(),
            policy_result,
            "2026-05-23T09:00:00Z".to_string(),
        );

        assert_eq!(call.connector_id, "connector.docs.v1");
        assert_eq!(call.call_id, "call_01");
        assert_eq!(call.status, ConnectorCallStatus::Pending);
        assert_eq!(call.timing.started_at, "2026-05-23T09:00:00Z");
        assert_eq!(call.timing.finished_at, None);
        assert_eq!(call.timing.duration_ms, None);
    }

    #[test]
    fn connector_status_serialization_is_snake_case() {
        let value = serde_json::to_string(&ConnectorCallStatus::Completed).unwrap();
        assert_eq!(value, "\"completed\"");
    }
}

