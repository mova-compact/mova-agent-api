use mova_agent_api::connectors::{
    create_connector_executor, ConnectorExecutionConfig, ConnectorExecutionRequest, ConnectorCallStatus,
    OfflineStubRule, SideEffectIntent,
};
use mova_agent_api::policy::{AdmissionDecision, PolicySummary};
use serde_json::json;

fn request(connector_id: &str, intent: SideEffectIntent) -> ConnectorExecutionRequest {
    ConnectorExecutionRequest {
        connector_id: connector_id.to_string(),
        call_id: "call_conformance_01".to_string(),
        side_effect_intent: intent,
        request: json!({"document_id":"doc_123"}),
        auth_context: json!({}),
        policy_result: PolicySummary {
            decision: AdmissionDecision::Allow,
            policy_version: "policy.default.v0".to_string(),
            reason_code: "authorized".to_string(),
        },
        started_at: "2026-05-23T10:00:00Z".to_string(),
    }
}

#[test]
fn deterministic_local_allows_none_intent() {
    let cfg = ConnectorExecutionConfig::deterministic_local_default();
    let exec = create_connector_executor(&cfg);
    let result = exec
        .execute(request("connector.docs.v1", SideEffectIntent::None))
        .unwrap();
    assert_eq!(result.call.status, ConnectorCallStatus::Completed);
}

#[test]
fn deterministic_local_denies_external_network_intent() {
    let cfg = ConnectorExecutionConfig::deterministic_local_default();
    let exec = create_connector_executor(&cfg);
    let err = exec
        .execute(request(
            "connector.docs.v1",
            SideEffectIntent::ExternalNetwork,
        ))
        .unwrap_err();
    assert_eq!(err.code, "connector_side_effect_denied");
}

#[test]
fn deterministic_local_denies_unknown_connector() {
    let cfg = ConnectorExecutionConfig::deterministic_local_default();
    let exec = create_connector_executor(&cfg);
    let err = exec
        .execute(request("connector.unknown", SideEffectIntent::None))
        .unwrap_err();
    assert_eq!(err.code, "connector_denied");
}

#[test]
fn offline_stub_returns_rule_driven_result() {
    let cfg = ConnectorExecutionConfig {
        adapter_kind: "offline_stub".to_string(),
        allowed_connectors: vec![],
        allowed_side_effect_intents: vec![],
        offline_stub_rules: vec![OfflineStubRule {
            connector_id: "connector.docs.v1".to_string(),
            call_status: ConnectorCallStatus::Completed,
            guard_reason: "stubbed_allow".to_string(),
        }],
    };
    let exec = create_connector_executor(&cfg);
    let result = exec
        .execute(request("connector.docs.v1", SideEffectIntent::LocalOnly))
        .unwrap();
    assert_eq!(result.guard_reason, "stubbed_allow");
}

#[test]
fn invalid_config_maps_to_deterministic_failure() {
    let cfg = ConnectorExecutionConfig {
        adapter_kind: "unsupported".to_string(),
        allowed_connectors: vec![],
        allowed_side_effect_intents: vec![],
        offline_stub_rules: vec![],
    };
    let exec = create_connector_executor(&cfg);
    let err = exec
        .execute(request("connector.docs.v1", SideEffectIntent::None))
        .unwrap_err();
    assert_eq!(err.code, "connector_config_invalid");
}
