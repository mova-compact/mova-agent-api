use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use mova_agent_api::connectors::connector_side_effects_enabled;
use mova_agent_api::execution::FlatExecutionPlan;
use mova_agent_api::http::{auth_placeholder, router, AppState, RetentionMode};
use mova_agent_api::request::{parse_request_envelope, validate_request_envelope};
use tower::util::ServiceExt;

#[tokio::test]
async fn http_transport_cannot_bypass_request_validation() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"request_id":"bad"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_transport_does_not_enforce_production_auth() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "request_id": "req_auth_boundary_01",
                        "actor": {"actor_type": "ai_agent", "actor_id": "agent_001"},
                        "source": {"channel": "api", "client_id": "client_001"},
                        "action": {
                            "action_id": "act_auth_boundary_01",
                            "action_type": "validate_document",
                            "target_kind": "document",
                            "input_payload": {"document_id": "doc_01"},
                            "policy_context": {"policy_profile_ref": "policy.default.v0"},
                            "connector_context": {"connector_set": ["connector.docs.v1"]},
                            "trace_ref": "trace:req_auth_boundary_01"
                        },
                        "inputs": {"document_id": "doc_01"},
                        "context": {"tenant_id": "tenant_001"},
                        "correlation": {"trace_id": "trace_boundary"},
                        "timestamps": {"requested_at": "2026-05-23T11:00:00Z"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn actions_run_rejects_invalid_action_input_selector() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "request_id": "req_boundary_01",
                        "actor": {"actor_type": "ai_agent", "actor_id": "agent_001"},
                        "source": {"channel": "api", "client_id": "client_001"},
                        "action": {
                            "action_id": "act_boundary_01",
                            "action_type": "validate_document",
                            "target_kind": "document",
                            "input_ref": "input://doc/01",
                            "input_payload": {"document_id": "doc_01"},
                            "policy_context": {"policy_profile_ref": "policy.default.v0"},
                            "connector_context": {"connector_set": ["connector.docs.v1"]},
                            "trace_ref": "trace:req_boundary_01"
                        },
                        "inputs": {"document_id": "doc_01"},
                        "context": {"tenant_id": "tenant_001"},
                        "correlation": {"trace_id": "trace_boundary"},
                        "timestamps": {"requested_at": "2026-05-23T11:00:00Z"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn connector_boundary_is_explicitly_no_side_effect() {
    assert!(!connector_side_effects_enabled());
}

#[test]
fn run_evidence_retention_is_in_memory_only() {
    let state = AppState::new();
    assert_eq!(
        state.retention_mode(),
        RetentionMode::AdapterBoundaryInMemoryDefault
    );
}

#[test]
fn auth_contract_is_placeholder_not_enforced() {
    let auth = auth_placeholder();
    assert_eq!(auth.auth_mode, "placeholder");
    assert!(!auth.enforced);
}

#[test]
fn flat_execution_has_no_orchestration_or_dynamic_routing_steps() {
    let plan = FlatExecutionPlan::from_action("run_guard_01".to_string(), "act_guard_01".to_string());
    assert!(plan.steps.iter().all(|step| !step.step_kind.contains("orchestration")));
    assert!(plan.steps.iter().all(|step| !step.step_kind.contains("routing")));
}

#[test]
fn request_module_enforces_semantic_validation_locally() {
    let parsed = parse_request_envelope(serde_json::json!({
        "request_id": "req_guard_01",
        "actor": {"actor_type": "ai_agent", "actor_id": "agent_001"},
        "source": {"channel": "api", "client_id": "client_001"},
        "action": {
            "action_id": "act_guard_01",
            "action_type": "validate_document",
            "target_kind": "document",
            "policy_context": {"policy_profile_ref": "policy.default.v0"},
            "connector_context": {"connector_set": ["connector.docs.v1"]},
            "trace_ref": "trace:req_guard_01"
        },
        "inputs": {"document_id": "doc_01"},
        "context": {"tenant_id": "tenant_001"},
        "correlation": {"trace_id": "trace_guard"},
        "timestamps": {"requested_at": "2026-05-23T11:01:00Z"}
    }))
    .unwrap();

    let errors = validate_request_envelope(&parsed);
    assert!(errors.iter().any(|e| e.field == "action"));
}
