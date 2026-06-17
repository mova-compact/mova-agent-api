use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use mova_agent_api::auth::{create_auth_verifier, AuthTrustConfig, AuthVerifier};
use mova_agent_api::connectors::{ConnectorExecutionError, ConnectorExecutor, FailingConnectorExecutor};
use mova_agent_api::http::{public_router, router, router_with_state, AppState};
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

fn public_contract_run_start_body() -> String {
    json!({
        "request_id": "req_contract_public_001",
        "actor": {"actor_type": "ai_agent", "actor_id": "agent_external"},
        "source": {"channel": "api", "client_id": "client_external"},
        "inputs": {},
        "context": {},
        "correlation": {
            "trace_id": "trace_contract_001",
            "correlation_id": "corr_contract_001"
        },
        "timestamps": {"requested_at": "2026-05-23T08:30:00Z"}
    })
    .to_string()
}

fn public_api_key() -> String {
    std::env::var("MOVA_API_KEY").unwrap_or_else(|_| "mova-dev-key".to_string())
}

fn public_admin_api_key() -> String {
    std::env::var("MOVA_ADMIN_API_KEY").unwrap_or_else(|_| "mova-admin-dev-key".to_string())
}

fn provider_execute_body(operation_id: &str, text: &str) -> String {
    json!({
        "operation_id": operation_id,
        "input_payload": {"text": text},
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

fn alpha_contract_flow() -> Value {
    json!({
        "version": "1.0",
        "description": "alpha flow",
        "entry": "alpha_start",
        "steps": [
            {
                "id": "alpha_start",
                "operation_id": "op_notify_webhook",
                "step_type": "connector_action",
                "execution_mode": "DETERMINISTIC",
                "connector": {
                    "name": "connector.http.generic.v1",
                    "endpoint_ref": "webhook_site_contract_run_test",
                    "method": "POST",
                    "side_effect_intent": "external_network"
                },
                "next": {"default": {"step": "alpha_gate"}}
            },
            {
                "id": "alpha_gate",
                "operation_id": "op_send_report",
                "step_type": "human_gate",
                "execution_mode": "HUMAN_GATE",
                "next": {
                    "approve": {"step": "alpha_terminal"},
                    "reject": {"terminal": "blocked"}
                }
            },
            {
                "id": "alpha_terminal",
                "step_type": "terminal",
                "execution_mode": "DETERMINISTIC",
                "next": {"default": {"terminal": "completed"}}
            }
        ]
    })
}

fn inline_admin_chat_contract_flow() -> Value {
    json!({
        "version": "1.0",
        "description": "inline admin chat flow",
        "entry": "deliver_admin_chat",
        "steps": [
            {
                "id": "deliver_admin_chat",
                "operation_id": "op_send_report",
                "step_type": "connector_action",
                "execution_mode": "DETERMINISTIC",
                "connector": {
                    "name": "provider.connector.v1",
                    "connector_ref": "telegram.admin_chat",
                    "operation": "send_message",
                    "side_effect_intent": "external_network"
                },
                "next": {"default": {"terminal": "completed"}}
            }
        ],
        "parallel_steps": []
    })
}

#[tokio::test]
async fn contract_run_corridor_happy_path_completes_with_evidence() {
    let app = router();

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_contract_run_alpha_v0/runs")
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
    assert_eq!(start_json["current_step_id"], "step_001");
    assert_eq!(start_json["next_allowed_operation_id"], "op_notify_webhook");

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
        .clone()
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
    assert_eq!(evidence_json["contract_id"], "fixture_contract_run_alpha_v0");
    assert_eq!(evidence_json["status"], "completed");
    assert!(evidence_json["evidence"]["steps"].as_array().unwrap().len() >= 1);
    assert!(evidence_json["evidence"]["gates"].as_array().unwrap().len() >= 1);
    assert!(evidence_json["evidence"]["transitions"].as_array().unwrap().len() >= 2);
    let connector_summary = &evidence_json["evidence"]["steps"][0]["connector_summary"];
    assert_eq!(connector_summary["connector_id"], "connector.http.generic.v1");
    assert_eq!(connector_summary["endpoint_ref"], "webhook_site_contract_run_test");
    assert_eq!(connector_summary["method"], "POST");
    assert_ne!(connector_summary["provider"], "contract_run_fixture");
    assert_eq!(connector_summary["provider"], "deterministic_local_http_generic");
    assert!(connector_summary.get("resolved_url").is_none());
    let admission = &evidence_json["evidence"]["steps"][0]["admission"];
    assert_eq!(admission["allowed_connector_id"], "connector.http.generic.v1");
    assert_eq!(admission["allowed_endpoint_ref"], "webhook_site_contract_run_test");
    assert_eq!(admission["allowed_method"], "POST");
    let first_transition = &evidence_json["evidence"]["transitions"][0];
    assert_eq!(first_transition["from_step_id"], "step_001");
    assert_eq!(first_transition["outcome"], "default");
    assert_eq!(first_transition["kind"], "next_step");
    assert_eq!(first_transition["target_step_id"], "step_002");
    let second_transition = &evidence_json["evidence"]["transitions"][1];
    assert_eq!(second_transition["from_step_id"], "step_002");
    assert_eq!(second_transition["outcome"], "approve");
}

#[tokio::test]
async fn contract_run_corridor_uses_contract_run_endpoint_scope() {
    let app = router();

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_contract_run_alpha_v0/runs")
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
    let admission = &next_json["operation_admission"];
    assert_eq!(admission["allowed_connector_id"], "connector.http.generic.v1");
    assert_eq!(
        admission["allowed_endpoint_ref"],
        "webhook_site_contract_run_test"
    );
    assert_eq!(admission["allowed_method"], "POST");
}

#[tokio::test]
async fn provider_connector_contract_admission_resolves_registry_constraints() {
    let app = router();

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_provider_connector_proxy_v0/runs")
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
    assert_eq!(start_json["current_step_id"], "step_send_provider_message");
    assert_eq!(start_json["next_allowed_operation_id"], "op_provider_send_message");

    let next = app
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
    let admission = &next_json["operation_admission"];
    assert_eq!(admission["allowed_connector_id"], "provider.connector.v1");
    assert!(admission["allowed_endpoint_ref"].is_null());
    assert_eq!(admission["allowed_method"], "POST");
    assert_eq!(
        admission["constraints"]["connector_ref"],
        "telegram.fixture_primary_channel"
    );
    assert_eq!(admission["constraints"]["provider"], "telegram");
    assert_eq!(admission["constraints"]["operation"], "send_message");
    assert_eq!(
        admission["constraints"]["side_effect_intent"],
        "external_network"
    );
}

#[tokio::test]
async fn provider_connector_contract_denies_nested_secret_and_connector_override() {
    let app = router();

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_provider_connector_proxy_v0/runs")
                .header("content-type", "application/json")
                .body(Body::from(contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    let start_body = to_bytes(start.into_body(), usize::MAX).await.unwrap();
    let start_json: Value = serde_json::from_slice(&start_body).unwrap();
    let run_id = start_json["run_id"].as_str().unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/contract-runs/{run_id}/steps/step_send_provider_message/execute"
                ))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "operation_id": "op_provider_send_message",
                        "input_payload": {
                            "text": "Owner report",
                            "chat_id": "evil",
                            "bot_token": "evil",
                            "connector_ref": "evil",
                            "provider_url": "https://evil.example"
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
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "connector_override_forbidden");
}

#[tokio::test]
async fn provider_connector_contract_executes_with_fake_adapter_and_redacted_evidence() {
    let app = router();

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_provider_connector_proxy_v0/runs")
                .header("content-type", "application/json")
                .body(Body::from(contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::ACCEPTED);
    let start_body = to_bytes(start.into_body(), usize::MAX).await.unwrap();
    let start_json: Value = serde_json::from_slice(&start_body).unwrap();
    let run_id = start_json["run_id"].as_str().unwrap();

    let execute = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/contract-runs/{run_id}/steps/step_send_provider_message/execute"
                ))
                .header("content-type", "application/json")
                .body(Body::from(provider_execute_body(
                    "op_provider_send_message",
                    "Owner report: revenue 1234 EUR",
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(execute.status(), StatusCode::ACCEPTED);

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
    assert_eq!(evidence_json["contract_id"], "fixture_provider_connector_proxy_v0");
    assert_eq!(evidence_json["status"], "completed");
    let connector_summary = &evidence_json["evidence"]["steps"][0]["connector_summary"];
    assert_eq!(connector_summary["connector_id"], "provider.connector.v1");
    assert!(connector_summary["endpoint_ref"].is_null());
    assert_eq!(connector_summary["method"], "POST");
    assert_eq!(connector_summary["provider"], "telegram");
    assert_eq!(connector_summary["connector_ref"], "telegram.fixture_primary_channel");
    assert_eq!(connector_summary["operation"], "send_message");
    assert_eq!(connector_summary["connector_mode"], "deterministic_fake_provider_connector");
    assert_eq!(connector_summary["response_preview"]["ok"], true);
    assert_eq!(connector_summary["response_preview"]["message_id"], 1001);
    let serialized = evidence_json.to_string();
    assert!(!serialized.contains("chat_id"));
    assert!(!serialized.contains("bot_token"));
    assert!(!serialized.contains("TELEGRAM_BOT_TOKEN"));
    assert!(!serialized.contains("api.telegram.org"));
}

#[tokio::test]
async fn public_contract_run_requires_api_key() {
    let app = public_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_contract_run_alpha_v0/runs")
                .header("content-type", "application/json")
                .body(Body::from(public_contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn public_contract_run_status_requires_api_key_before_not_found() {
    let app = public_router();
    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/contract-runs/run_missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    let invalid = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/contract-runs/run_missing")
                .header("x-mova-api-key", "wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn public_contract_run_rejects_invalid_api_key() {
    let app = public_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_contract_run_alpha_v0/runs")
                .header("content-type", "application/json")
                .header("x-mova-api-key", "wrong")
                .body(Body::from(public_contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn public_contract_run_accepts_valid_api_key_and_assigns_server_tenant() {
    let app = public_router();
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_contract_run_alpha_v0/runs")
                .header("content-type", "application/json")
                .header("x-mova-api-key", public_api_key())
                .body(Body::from(public_contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::ACCEPTED);
    let body = to_bytes(start.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["tenant_id"], "tenant_server_owned");

    let run_id = json["run_id"].as_str().unwrap();
    let status = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/contract-runs/{run_id}"))
                .header("x-mova-api-key", public_api_key())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let body = to_bytes(status.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["tenant_id"], "tenant_server_owned");
}

#[tokio::test]
async fn public_contract_run_start_replays_same_idempotency_key() {
    let app = public_router();
    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_contract_run_alpha_v0/runs")
                .header("content-type", "application/json")
                .header("x-mova-api-key", public_api_key())
                .header("idempotency-key", "release-start-001")
                .body(Body::from(public_contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let first_body = to_bytes(first.into_body(), usize::MAX).await.unwrap();
    let first_json: Value = serde_json::from_slice(&first_body).unwrap();
    assert_eq!(first_json["tenant_id"], "tenant_server_owned");
    assert_eq!(first_json["idempotency_key"], "release-start-001");

    let replay = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_contract_run_alpha_v0/runs")
                .header("content-type", "application/json")
                .header("x-mova-api-key", public_api_key())
                .header("idempotency-key", "release-start-001")
                .body(Body::from(public_contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::ACCEPTED);
    let replay_body = to_bytes(replay.into_body(), usize::MAX).await.unwrap();
    let replay_json: Value = serde_json::from_slice(&replay_body).unwrap();
    assert_eq!(replay_json["run_id"], first_json["run_id"]);
    assert_eq!(replay_json["tenant_id"], "tenant_server_owned");
    assert_eq!(replay_json["idempotent_replay"], true);
}

#[tokio::test]
async fn public_contract_run_rejects_client_tenant_override() {
    let app = public_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_contract_run_alpha_v0/runs")
                .header("content-type", "application/json")
                .header("x-mova-api-key", public_api_key())
                .body(Body::from(
                    json!({
                        "request_id": "req_contract_public_002",
                        "actor": {"actor_type": "ai_agent", "actor_id": "agent_external"},
                        "source": {"channel": "api", "client_id": "client_external"},
                        "inputs": {},
                        "context": {"tenant_id": "evil"},
                        "correlation": {
                            "trace_id": "trace_contract_001",
                            "correlation_id": "corr_contract_001"
                        },
                        "timestamps": {"requested_at": "2026-05-23T08:30:00Z"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn public_contract_run_step_execute_replays_same_idempotency_key_and_blocks_second_key() {
    let app = public_router();
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_provider_connector_proxy_v0/runs")
                .header("content-type", "application/json")
                .header("x-mova-api-key", public_api_key())
                .body(Body::from(public_contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::ACCEPTED);
    let start_body = to_bytes(start.into_body(), usize::MAX).await.unwrap();
    let start_json: Value = serde_json::from_slice(&start_body).unwrap();
    let run_id = start_json["run_id"].as_str().unwrap();

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_send_provider_message/execute"))
                .header("content-type", "application/json")
                .header("x-mova-api-key", public_api_key())
                .header("idempotency-key", "release-step-001")
                .body(Body::from(provider_execute_body(
                    "op_provider_send_message",
                    "release replay proof",
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let first_body = to_bytes(first.into_body(), usize::MAX).await.unwrap();
    let first_json: Value = serde_json::from_slice(&first_body).unwrap();
    assert_eq!(first_json["tenant_id"], "tenant_server_owned");
    assert_eq!(first_json["idempotency_key"], "release-step-001");

    let replay = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_send_provider_message/execute"))
                .header("content-type", "application/json")
                .header("x-mova-api-key", public_api_key())
                .header("idempotency-key", "release-step-001")
                .body(Body::from(provider_execute_body(
                    "op_provider_send_message",
                    "release replay proof",
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::ACCEPTED);
    let replay_body = to_bytes(replay.into_body(), usize::MAX).await.unwrap();
    let replay_json: Value = serde_json::from_slice(&replay_body).unwrap();
    assert_eq!(replay_json, first_json);

    let conflict = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_send_provider_message/execute"))
                .header("content-type", "application/json")
                .header("x-mova-api-key", public_api_key())
                .header("idempotency-key", "release-step-002")
                .body(Body::from(provider_execute_body(
                    "op_provider_send_message",
                    "release replay proof",
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    let conflict_body = to_bytes(conflict.into_body(), usize::MAX).await.unwrap();
    let conflict_json: Value = serde_json::from_slice(&conflict_body).unwrap();
    assert_eq!(conflict_json["error"]["code"], "step_already_executed");
}

#[tokio::test]
async fn public_contract_run_evidence_exposes_server_tenant_and_idempotency_without_secrets() {
    let app = public_router();
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_provider_connector_proxy_v0/runs")
                .header("content-type", "application/json")
                .header("x-mova-api-key", public_api_key())
                .body(Body::from(public_contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::ACCEPTED);
    let start_body = to_bytes(start.into_body(), usize::MAX).await.unwrap();
    let start_json: Value = serde_json::from_slice(&start_body).unwrap();
    let run_id = start_json["run_id"].as_str().unwrap();

    let execute = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/step_send_provider_message/execute"))
                .header("content-type", "application/json")
                .header("x-mova-api-key", public_api_key())
                .header("idempotency-key", "release-evidence-001")
                .body(Body::from(provider_execute_body(
                    "op_provider_send_message",
                    "release evidence proof",
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(execute.status(), StatusCode::ACCEPTED);

    let evidence = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/contract-runs/{run_id}/evidence"))
                .header("x-mova-api-key", public_api_key())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(evidence.status(), StatusCode::OK);
    let evidence_body = to_bytes(evidence.into_body(), usize::MAX).await.unwrap();
    let evidence_json: Value = serde_json::from_slice(&evidence_body).unwrap();
    assert_eq!(evidence_json["tenant_id"], "tenant_server_owned");
    assert_eq!(evidence_json["evidence"]["idempotency_key"], "release-evidence-001");
    let serialized = evidence_json.to_string();
    assert!(!serialized.contains("chat_id"));
    assert!(!serialized.contains("bot_token"));
    assert!(!serialized.contains("TELEGRAM_BOT_TOKEN"));
    assert!(!serialized.contains("api.telegram.org"));
}

#[tokio::test]
async fn public_contract_register_requires_admin_api_key() {
    let app = public_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "mode": "local_packaged",
                        "contract_id": "public_register_missing_admin_v0",
                        "execution_type": "agent",
                        "manifest": {
                            "contract_id": "public_register_missing_admin_v0"
                        },
                        "flow_json": alpha_contract_flow()
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn public_contract_register_rejects_tenant_key_and_accepts_admin_key() {
    let app = public_router();
    let payload = json!({
        "mode": "local_packaged",
        "contract_id": "public_register_admin_v0",
        "execution_type": "agent",
        "manifest": {
            "contract_id": "public_register_admin_v0"
        },
        "flow_json": alpha_contract_flow()
    });

    let tenant_key = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .header("x-mova-api-key", public_api_key())
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tenant_key.status(), StatusCode::UNAUTHORIZED);

    let register = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .header("x-mova-admin-api-key", public_admin_api_key())
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::CREATED);

    let start = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/public_register_admin_v0/runs")
                .header("content-type", "application/json")
                .header("x-mova-api-key", public_api_key())
                .body(Body::from(public_contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn public_router_does_not_expose_lab_or_internal_routes() {
    let app = public_router();
    let checks = vec![
        (Method::POST, "/actions/validate", Some("{}")),
        (Method::POST, "/actions/run", Some("{}")),
        (Method::GET, "/runs/run_001", None),
        (Method::GET, "/runs/run_001/evidence", None),
        (Method::GET, "/contracts", None),
        (Method::GET, "/contracts/fixture_contract_run_alpha_v0", None),
        (Method::POST, "/contracts/fixture_contract_run_alpha_v0/run", Some("{}")),
        (Method::POST, "/contracts/runs/run_001/decision", Some("{}")),
    ];

    for (method, uri, body) in checks {
        let mut builder = Request::builder().method(method).uri(uri);
        if body.is_some() {
            builder = builder.header("content-type", "application/json");
        }
        let response = app
            .clone()
            .oneshot(
                builder
                    .body(match body {
                        Some(value) => Body::from(value.to_string()),
                        None => Body::empty(),
                    })
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "unexpected public exposure for {uri}");
    }
}

#[tokio::test]
async fn contract_run_corridor_denies_wrong_step_operation_and_override() {
    let app = router();
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_contract_run_alpha_v0/runs")
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
                .uri("/contracts/fixture_contract_run_alpha_v0/runs")
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
        .clone()
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
    assert_eq!(status_json["gate"]["status"], "resolved");
    assert_eq!(status_json["gate"]["decision"], "reject");

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
    assert_eq!(evidence_json["evidence"]["gates"][0]["decision"], "reject");
    assert_eq!(evidence_json["evidence"]["gates"][0]["step_id"], "step_002");
    assert_eq!(evidence_json["evidence"]["gates"][0]["requested_operation_id"], "op_send_report");
    let transitions = evidence_json["evidence"]["transitions"].as_array().unwrap();
    assert!(transitions.iter().any(|transition| {
        transition["from_step_id"] == "step_002"
            && transition["outcome"] == "reject"
            && transition["terminal_status"] == "blocked"
    }));
}

#[tokio::test]
async fn contract_run_corridor_approve_response_exposes_current_step_and_next_operation() {
    let app = router();
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_contract_run_alpha_v0/runs")
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
                .uri("/contracts/fixture_contract_run_alpha_v0/runs")
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

#[tokio::test]
async fn contract_run_corridor_uses_flow_driven_non_fixture_step_ids() {
    let app = router();
    let register_payload = json!({
        "contract_id": "alpha_contract_v0",
        "execution_type": "agent",
        "inline_flow_json": alpha_contract_flow()
    });
    let register = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(register_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::CREATED);

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/alpha_contract_v0/runs")
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
    assert_eq!(start_json["current_step_id"], "alpha_start");
    assert_eq!(start_json["next_allowed_operation_id"], "op_notify_webhook");

    let execute_start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/alpha_start/execute"))
                .header("content-type", "application/json")
                .body(Body::from(execute_body("op_notify_webhook")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(execute_start.status(), StatusCode::ACCEPTED);

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
    assert_eq!(gate_json["step_id"], "alpha_gate");
    let gate_id = gate_json["gate_id"].as_str().unwrap();

    let approve = app
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
    assert_eq!(approve.status(), StatusCode::OK);

    let execute_gate = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/contract-runs/{run_id}/steps/alpha_gate/execute"))
                .header("content-type", "application/json")
                .body(Body::from(execute_body("op_send_report")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(execute_gate.status(), StatusCode::ACCEPTED);

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
    let evidence_body = to_bytes(evidence.into_body(), usize::MAX).await.unwrap();
    let evidence_json: Value = serde_json::from_slice(&evidence_body).unwrap();
    assert_eq!(evidence_json["status"], "completed");
    assert_eq!(evidence_json["evidence"]["transitions"][0]["target_step_id"], "alpha_gate");
    assert_eq!(evidence_json["evidence"]["gates"][0]["step_id"], "alpha_gate");
    assert_eq!(evidence_json["evidence"]["gates"][0]["requested_operation_id"], "op_send_report");
}

#[tokio::test]
async fn contract_run_corridor_resolves_runtime_chat_id_for_inline_connector_ref() {
    let app = router();
    let register_payload = json!({
        "contract_id": "inline_admin_chat_contract_v0",
        "execution_type": "agent",
        "inline_flow_json": inline_admin_chat_contract_flow()
    });
    let register = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(register_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::CREATED);

    let start_body = json!({
        "request_id": "req_inline_admin_chat_001",
        "actor": {"actor_type": "ai_agent", "actor_id": "agent_001"},
        "source": {"channel": "api", "client_id": "client_001"},
        "auth_context": {
            "mode": "placeholder",
            "scopes": ["contracts.run"],
            "source": "example",
            "verified": false
        },
        "inputs": {},
        "context": {
            "tenant_id": "tenant_001",
            "runtime_targets": {
                "admin": {"chat_id": "runtime-admin-chat"}
            }
        },
        "correlation": {
            "trace_id": "trace_inline_admin_chat_001",
            "correlation_id": "corr_inline_admin_chat_001"
        },
        "timestamps": {"requested_at": "2026-05-23T08:30:00Z"}
    })
    .to_string();

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/inline_admin_chat_contract_v0/runs")
                .header("content-type", "application/json")
                .body(Body::from(start_body))
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
    assert_eq!(
        next_json["operation_admission"]["constraints"]["connector_ref"],
        "telegram.admin_chat"
    );
    assert_eq!(
        next_json["operation_admission"]["constraints"]["target_ref"],
        "binding://telegram_admin_chat_send_message"
    );
    assert_eq!(
        next_json["operation_admission"]["constraints"]["resolved_chat_id"],
        "runtime-admin-chat"
    );
}

#[tokio::test]
async fn contract_run_corridor_registration_rejects_missing_operation_id() {
    let app = router();
    let register_payload = json!({
        "contract_id": "missing_operation_id_v0",
        "execution_type": "agent",
        "inline_flow_json": {
            "version": "1.0",
            "description": "broken flow",
            "entry": "broken_start",
            "steps": [
                {
                    "id": "broken_start",
                    "step_type": "connector_action",
                    "execution_mode": "DETERMINISTIC",
                    "connector": {
                        "name": "connector.http.generic.v1",
                        "endpoint_ref": "webhook_site_contract_run_test",
                        "method": "POST",
                        "side_effect_intent": "external_network"
                    },
                    "next": {"default": {"terminal": "completed"}}
                }
            ]
        }
    });
    let register = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(register_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::BAD_REQUEST);
    let register_body = to_bytes(register.into_body(), usize::MAX).await.unwrap();
    let register_json: Value = serde_json::from_slice(&register_body).unwrap();
    assert_eq!(register_json["error"]["code"], "contract_operation_id_missing");
}

#[tokio::test]
async fn contract_run_corridor_registration_rejects_duplicate_step_id() {
    let app = router();
    let register_payload = json!({
        "contract_id": "duplicate_step_id_v0",
        "execution_type": "agent",
        "inline_flow_json": {
            "version": "1.0",
            "description": "duplicate ids",
            "entry": "dup",
            "steps": [
                {
                    "id": "dup",
                    "operation_id": "op_notify_webhook",
                    "step_type": "connector_action",
                    "execution_mode": "DETERMINISTIC",
                    "connector": {
                        "name": "connector.http.generic.v1",
                        "endpoint_ref": "webhook_site_contract_run_test",
                        "method": "POST",
                        "side_effect_intent": "external_network"
                    },
                    "next": {"default": {"terminal": "completed"}}
                },
                {
                    "id": "dup",
                    "step_type": "terminal",
                    "execution_mode": "DETERMINISTIC",
                    "next": {"default": {"terminal": "completed"}}
                }
            ]
        }
    });
    let register = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(register_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::BAD_REQUEST);
    let register_body = to_bytes(register.into_body(), usize::MAX).await.unwrap();
    let register_json: Value = serde_json::from_slice(&register_body).unwrap();
    assert_eq!(register_json["error"]["code"], "contract_duplicate_step_id");
}

#[tokio::test]
async fn contract_run_corridor_registration_rejects_missing_entry() {
    let app = router();
    let register_payload = json!({
        "contract_id": "missing_entry_v0",
        "execution_type": "agent",
        "inline_flow_json": {
            "version": "1.0",
            "description": "missing entry",
            "entry": "missing",
            "steps": [
                {
                    "id": "start",
                    "operation_id": "op_notify_webhook",
                    "step_type": "connector_action",
                    "execution_mode": "DETERMINISTIC",
                    "connector": {
                        "name": "connector.http.generic.v1",
                        "endpoint_ref": "webhook_site_contract_run_test",
                        "method": "POST",
                        "side_effect_intent": "external_network"
                    },
                    "next": {"default": {"terminal": "completed"}}
                }
            ]
        }
    });
    let register = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(register_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::BAD_REQUEST);
    let register_body = to_bytes(register.into_body(), usize::MAX).await.unwrap();
    let register_json: Value = serde_json::from_slice(&register_body).unwrap();
    assert_eq!(register_json["error"]["code"], "contract_step_not_found");
}

#[tokio::test]
async fn contract_run_corridor_registration_rejects_missing_transition_target() {
    let app = router();
    let register_payload = json!({
        "contract_id": "missing_transition_target_v0",
        "execution_type": "agent",
        "inline_flow_json": {
            "version": "1.0",
            "description": "missing target",
            "entry": "start",
            "steps": [
                {
                    "id": "start",
                    "operation_id": "op_notify_webhook",
                    "step_type": "connector_action",
                    "execution_mode": "DETERMINISTIC",
                    "connector": {
                        "name": "connector.http.generic.v1",
                        "endpoint_ref": "webhook_site_contract_run_test",
                        "method": "POST",
                        "side_effect_intent": "external_network"
                    },
                    "next": {"default": {"step": "missing_step"}}
                }
            ]
        }
    });
    let register = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(register_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::BAD_REQUEST);
    let register_body = to_bytes(register.into_body(), usize::MAX).await.unwrap();
    let register_json: Value = serde_json::from_slice(&register_body).unwrap();
    assert_eq!(register_json["error"]["code"], "contract_transition_target_missing");
}

#[tokio::test]
async fn contract_run_corridor_registration_rejects_missing_connector_metadata() {
    let app = router();
    let register_payload = json!({
        "contract_id": "missing_connector_metadata_v0",
        "execution_type": "agent",
        "inline_flow_json": {
            "version": "1.0",
            "description": "missing connector metadata",
            "entry": "start",
            "steps": [
                {
                    "id": "start",
                    "operation_id": "op_notify_webhook",
                    "step_type": "connector_action",
                    "execution_mode": "DETERMINISTIC",
                    "next": {"default": {"terminal": "completed"}}
                }
            ]
        }
    });
    let register = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(register_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::BAD_REQUEST);
    let register_body = to_bytes(register.into_body(), usize::MAX).await.unwrap();
    let register_json: Value = serde_json::from_slice(&register_body).unwrap();
    assert_eq!(register_json["error"]["code"], "contract_connector_metadata_missing");
}

#[tokio::test]
async fn contract_run_corridor_registration_accepts_local_only_side_effect_intent() {
    let app = router();
    let register_payload = json!({
        "contract_id": "local_only_side_effect_v0",
        "execution_type": "agent",
        "inline_flow_json": {
            "version": "1.0",
            "description": "local only side effect",
            "entry": "local_start",
            "steps": [
                {
                    "id": "local_start",
                    "operation_id": "op_notify_webhook",
                    "step_type": "connector_action",
                    "execution_mode": "DETERMINISTIC",
                    "connector": {
                        "name": "connector.http.generic.v1",
                        "endpoint_ref": "webhook_site_contract_run_test",
                        "method": "POST",
                        "side_effect_intent": "local_only"
                    },
                    "next": {"default": {"terminal": "completed"}}
                }
            ]
        }
    });
    let register = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(register_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::CREATED);

    let start = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/local_only_side_effect_v0/runs")
                .header("content-type", "application/json")
                .body(Body::from(contract_run_start_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn contract_run_corridor_registration_rejects_destructive_side_effect_intent() {
    let app = router();
    let register_payload = json!({
        "contract_id": "destructive_side_effect_v0",
        "execution_type": "agent",
        "inline_flow_json": {
            "version": "1.0",
            "description": "destructive side effect",
            "entry": "start",
            "steps": [
                {
                    "id": "start",
                    "operation_id": "op_notify_webhook",
                    "step_type": "connector_action",
                    "execution_mode": "DETERMINISTIC",
                    "connector": {
                        "name": "connector.http.generic.v1",
                        "endpoint_ref": "webhook_site_contract_run_test",
                        "method": "POST",
                        "side_effect_intent": "destructive"
                    },
                    "next": {"default": {"terminal": "completed"}}
                }
            ]
        }
    });
    let register = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(register_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::BAD_REQUEST);
    let register_body = to_bytes(register.into_body(), usize::MAX).await.unwrap();
    let register_json: Value = serde_json::from_slice(&register_body).unwrap();
    assert_eq!(register_json["error"]["code"], "contract_connector_metadata_missing");
    assert!(register_json["error"]["details"][0]
        .as_str()
        .unwrap_or_default()
        .contains("allowed values: none, local_only, external_network"));
}

#[tokio::test]
async fn contract_run_corridor_registration_rejects_unknown_side_effect_intent() {
    let app = router();
    let register_payload = json!({
        "contract_id": "unknown_side_effect_v0",
        "execution_type": "agent",
        "inline_flow_json": {
            "version": "1.0",
            "description": "unknown side effect",
            "entry": "start",
            "steps": [
                {
                    "id": "start",
                    "operation_id": "op_notify_webhook",
                    "step_type": "connector_action",
                    "execution_mode": "DETERMINISTIC",
                    "connector": {
                        "name": "connector.http.generic.v1",
                        "endpoint_ref": "webhook_site_contract_run_test",
                        "method": "POST",
                        "side_effect_intent": "external_magic"
                    },
                    "next": {"default": {"terminal": "completed"}}
                }
            ]
        }
    });
    let register = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(register_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::BAD_REQUEST);
    let register_body = to_bytes(register.into_body(), usize::MAX).await.unwrap();
    let register_json: Value = serde_json::from_slice(&register_body).unwrap();
    assert_eq!(register_json["error"]["code"], "contract_connector_metadata_missing");
}
