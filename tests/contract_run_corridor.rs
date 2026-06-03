use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use mova_agent_api::auth::{create_auth_verifier, AuthTrustConfig, AuthVerifier};
use mova_agent_api::connectors::{ConnectorExecutionError, ConnectorExecutor, FailingConnectorExecutor};
use mova_agent_api::http::{router, router_with_state, AppState};
use mova_agent_api::storage::{create_run_store, RunStore, StorageConfig};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::util::ServiceExt;

fn contract_run_start_body() -> String {
    json!({
        "request_id": "req_contract_001",
        "actor": {"actor_type": "ai_agent", "actor_id": "agent_001"},
        "source": {"channel": "api", "client_id": "client_001"},
        "auth_context": {
            "mode": "placeholder",
            "scopes": ["contracts.run"],
            "source": "example",
            "verified": false
        },
        "inputs": {},
        "context": {"tenant_id": "tenant_001"},
        "correlation": {
            "trace_id": "trace_contract_001",
            "correlation_id": "corr_contract_001"
        },
        "timestamps": {"requested_at": "2026-05-23T08:30:00Z"}
    })
    .to_string()
}

fn execute_body(operation_id: &str) -> String {
    json!({
        "operation_id": operation_id,
        "input_payload": {"message": "contract-run smoke"},
        "correlation": {
            "trace_id": "trace_contract_001",
            "correlation_id": "corr_contract_001"
        }
    })
    .to_string()
}

fn resolve_body(decision: &str) -> String {
    json!({
        "decision": decision,
        "actor": {"actor_type": "human", "actor_id": "user_001"},
        "reason": "Approved for test smoke",
        "timestamps": {"decided_at": "2026-05-23T08:32:00Z"}
    })
    .to_string()
}

#[tokio::test]
async fn contract_run_corridor_happy_path_completes_with_evidence() {
    let app = router();

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/daily_owner_report_v0/runs")
                .header("content-type", "application/json")
                .body(Body::from(contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::ACCEPTED);
    let start_body = to_bytes(start.into_body(), usize::MAX).await.unwrap();
    let start_json: Value = serde_json::from_slice(&start_body).unwrap();
    let run_id = start_json["run_id"].as_str().unwrap().to_string();

    let next = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/contract-runs/{run_id}/next"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(next.status(), StatusCode::OK);
    let next_body = to_bytes(next.into_body(), usize::MAX).await.unwrap();
    let next_json: Value = serde_json::from_slice(&next_body).unwrap();
    assert_eq!(next_json["operation_admission"]["decision"], "allow");
    assert_eq!(next_json["step"]["step_id"], "step_001");

    let execute = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_001/execute"))
                .header("content-type", "application/json")
                .body(Body::from(execute_body("op_notify_webhook")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(execute.status(), StatusCode::ACCEPTED);

    let gate = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/contract-runs/{run_id}/gates/current"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(gate.status(), StatusCode::OK);
    let gate_body = to_bytes(gate.into_body(), usize::MAX).await.unwrap();
    let gate_json: Value = serde_json::from_slice(&gate_body).unwrap();
    let gate_id = gate_json["gate_id"].as_str().unwrap();

    let resolve = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/gates/{gate_id}/resolve"))
                .header("content-type", "application/json")
                .body(Body::from(resolve_body("approve")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resolve.status(), StatusCode::OK);

    let next_after_gate = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/contract-runs/{run_id}/next"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(next_after_gate.status(), StatusCode::OK);
    let next_after_gate_body = to_bytes(next_after_gate.into_body(), usize::MAX).await.unwrap();
    let next_after_gate_json: Value = serde_json::from_slice(&next_after_gate_body).unwrap();
    assert_eq!(next_after_gate_json["step"]["step_id"], "step_002");
    assert_eq!(next_after_gate_json["operation_admission"]["decision"], "allow");
    assert_eq!(next_after_gate_json["operation_admission"]["reason_code"], "HUMAN_GATE_APPROVED");

    let execute_gated = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_002/execute"))
                .header("content-type", "application/json")
                .body(Body::from(execute_body("op_send_report")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(execute_gated.status(), StatusCode::ACCEPTED);

    let evidence = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/contract-runs/{run_id}/evidence"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(evidence.status(), StatusCode::OK);
    let evidence_body = to_bytes(evidence.into_body(), usize::MAX).await.unwrap();
    let evidence_json: Value = serde_json::from_slice(&evidence_body).unwrap();
    assert_eq!(evidence_json["contract_id"], "daily_owner_report_v0");
    assert_eq!(evidence_json["status"], "completed");
    assert!(evidence_json["evidence"]["steps"].as_array().unwrap().len() >= 1);
    assert!(evidence_json["evidence"]["gates"].as_array().unwrap().len() >= 1);
    let connector_summary = &evidence_json["evidence"]["steps"][0]["connector_summary"];
    assert_eq!(connector_summary["connector_id"], "connector.http.generic.v1");
    assert_eq!(connector_summary["endpoint_ref"], "webhook_site_test");
    assert_eq!(connector_summary["method"], "POST");
    assert_ne!(connector_summary["provider"], "contract_run_fixture");
    assert_eq!(connector_summary["provider"], "deterministic_local_http_generic");
    let admission = &evidence_json["evidence"]["steps"][0]["admission"];
    assert_eq!(admission["allowed_connector_id"], "connector.http.generic.v1");
    assert_eq!(admission["allowed_endpoint_ref"], "webhook_site_test");
    assert_eq!(admission["allowed_method"], "POST");
}

#[tokio::test]
async fn contract_run_corridor_denies_wrong_step_operation_and_override() {
    let app = router();
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/daily_owner_report_v0/runs")
                .header("content-type", "application/json")
                .body(Body::from(contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(start.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let run_id = json["run_id"].as_str().unwrap();

    let wrong_step = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_999/execute"))
                .header("content-type", "application/json")
                .body(Body::from(execute_body("op_notify_webhook")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_step.status(), StatusCode::CONFLICT);

    let wrong_operation = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_001/execute"))
                .header("content-type", "application/json")
                .body(Body::from(execute_body("op_wrong")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_operation.status(), StatusCode::FORBIDDEN);

    let override_attempt = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_001/execute"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "operation_id": "op_notify_webhook",
                        "connector_id": "connector.override.v1",
                        "input_payload": {"message": "x"},
                        "correlation": {
                            "trace_id": "trace_contract_001",
                            "correlation_id": "corr_contract_001"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(override_attempt.status(), StatusCode::FORBIDDEN);

    let nested_override_attempt = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_001/execute"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "operation_id": "op_notify_webhook",
                        "input_payload": {
                            "message": "x",
                            "endpoint_ref": "evil",
                            "method": "DELETE",
                            "target_url": "https://example.com"
                        },
                        "correlation": {
                            "trace_id": "trace_contract_001",
                            "correlation_id": "corr_contract_001"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(nested_override_attempt.status(), StatusCode::FORBIDDEN);
    let nested_body = to_bytes(nested_override_attempt.into_body(), usize::MAX).await.unwrap();
    let nested_json: Value = serde_json::from_slice(&nested_body).unwrap();
    assert_eq!(nested_json["error"]["code"], "connector_override_forbidden");
}

#[tokio::test]
async fn contract_run_corridor_reject_gate_blocks_run() {
    let app = router();
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/daily_owner_report_v0/runs")
                .header("content-type", "application/json")
                .body(Body::from(contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(start.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let run_id = json["run_id"].as_str().unwrap();

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_001/execute"))
                .header("content-type", "application/json")
                .body(Body::from(execute_body("op_notify_webhook")))
                .unwrap(),
        )
        .await
        .unwrap();

    let gate = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/contract-runs/{run_id}/gates/current"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let gate_body = to_bytes(gate.into_body(), usize::MAX).await.unwrap();
    let gate_json: Value = serde_json::from_slice(&gate_body).unwrap();
    let gate_id = gate_json["gate_id"].as_str().unwrap();

    let reject = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/gates/{gate_id}/resolve"))
                .header("content-type", "application/json")
                .body(Body::from(resolve_body("reject")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reject.status(), StatusCode::OK);

    let status = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/contract-runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status_body = to_bytes(status.into_body(), usize::MAX).await.unwrap();
    let status_json: Value = serde_json::from_slice(&status_body).unwrap();
    assert_eq!(status_json["status"], "blocked");
}

#[tokio::test]
async fn contract_run_corridor_approve_response_exposes_current_step_and_next_operation() {
    let app = router();
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/daily_owner_report_v0/runs")
                .header("content-type", "application/json")
                .body(Body::from(contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(start.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let run_id = json["run_id"].as_str().unwrap();

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_001/execute"))
                .header("content-type", "application/json")
                .body(Body::from(execute_body("op_notify_webhook")))
                .unwrap(),
        )
        .await
        .unwrap();

    let gate = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/contract-runs/{run_id}/gates/current"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let gate_body = to_bytes(gate.into_body(), usize::MAX).await.unwrap();
    let gate_json: Value = serde_json::from_slice(&gate_body).unwrap();
    let gate_id = gate_json["gate_id"].as_str().unwrap();

    let approve = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/gates/{gate_id}/resolve"))
                .header("content-type", "application/json")
                .body(Body::from(resolve_body("approve")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK);
    let approve_body = to_bytes(approve.into_body(), usize::MAX).await.unwrap();
    let approve_json: Value = serde_json::from_slice(&approve_body).unwrap();
    assert_eq!(approve_json["current_step_id"], "step_002");
    assert_eq!(approve_json["next_allowed_operation_id"], "op_send_report");
}

#[tokio::test]
async fn contract_run_corridor_returns_bad_gateway_when_connector_executor_fails() {
    let auth_verifier: Arc<dyn AuthVerifier> = Arc::from(create_auth_verifier(&AuthTrustConfig::default_v0()));
    let run_store: Arc<dyn RunStore> = Arc::from(create_run_store(&StorageConfig::in_memory_default()));
    let failing_connector: Arc<dyn ConnectorExecutor> = Arc::new(FailingConnectorExecutor::new(
        ConnectorExecutionError::new("connector_runtime_failed", "simulated contract-run connector failure"),
    ));
    let app = router_with_state(AppState::with_boundaries(
        auth_verifier,
        run_store,
        failing_connector,
    ));

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/daily_owner_report_v0/runs")
                .header("content-type", "application/json")
                .body(Body::from(contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(start.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let run_id = json["run_id"].as_str().unwrap();

    let execute = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_001/execute"))
                .header("content-type", "application/json")
                .body(Body::from(execute_body("op_notify_webhook")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(execute.status(), StatusCode::BAD_GATEWAY);
}
