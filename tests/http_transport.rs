use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use mova_agent_api::auth::{create_auth_verifier, AuthTrustConfig, AuthVerifier};
use mova_agent_api::http::{router, router_with_state, AppState};
use serde_json::Value;
use std::sync::Arc;
use tower::util::ServiceExt;

fn minimal_request_body() -> String {
    serde_json::json!({
        "request_id": "req_http_01",
        "actor": {"actor_type": "ai_agent", "actor_id": "agent_001"},
        "source": {"channel": "api", "client_id": "client_001"},
        "action": {
            "action_id": "act_01",
            "action_type": "validate_document",
            "target_kind": "document",
            "input_payload": {"document_id": "doc_123"},
            "policy_context": {"policy_profile_ref": "policy.default.v0"},
            "connector_context": {"connector_set": ["connector.docs.v1"]},
            "trace_ref": "trace:req_http_01"
        },
        "inputs": {"document_id": "doc_123"},
        "context": {"tenant_id": "tenant_001"},
        "correlation": {"trace_id": "trace_abc123"},
        "timestamps": {"requested_at": "2026-05-23T08:30:00Z"}
    })
    .to_string()
}

fn invalid_dual_input_request_body() -> String {
    serde_json::json!({
        "request_id": "req_http_dual_01",
        "actor": {"actor_type": "ai_agent", "actor_id": "agent_001"},
        "source": {"channel": "api", "client_id": "client_001"},
        "action": {
            "action_id": "act_dual_01",
            "action_type": "validate_document",
            "target_kind": "document",
            "input_ref": "input://doc/123",
            "input_payload": {"document_id": "doc_123"},
            "policy_context": {"policy_profile_ref": "policy.default.v0"},
            "connector_context": {"connector_set": ["connector.docs.v1"]},
            "trace_ref": "trace:req_http_dual_01"
        },
        "inputs": {"document_id": "doc_123"},
        "context": {"tenant_id": "tenant_001"},
        "correlation": {"trace_id": "trace_abc123"},
        "timestamps": {"requested_at": "2026-05-23T08:30:00Z"}
    })
    .to_string()
}

#[tokio::test]
async fn get_capabilities_returns_v0_metadata() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/capabilities")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["action_types"].is_array());
    assert!(json["policy_decisions"].is_array());
    assert_eq!(
        json["execution_path"],
        serde_json::json!([
            "agent_request",
            "action_model",
            "policy_admission",
            "flat_execution",
            "connector_call",
            "observation_write",
            "evidence_response"
        ])
    );
}

#[tokio::test]
async fn post_actions_validate_accepts_minimal_request() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/validate")
                .header("content-type", "application/json")
                .body(Body::from(minimal_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["valid"], true);
}

#[tokio::test]
async fn post_actions_validate_returns_clear_validation_error() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/validate")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"request_id":"bad"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
    assert!(json["error"]["details"].is_array());
}

#[tokio::test]
async fn post_actions_run_creates_deterministic_run() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .body(Body::from(minimal_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["run_id"], "run_req_http_01");
    assert_eq!(json["status"], "completed");
    assert_eq!(json["trace_ref"], "trace:req_http_01");
    assert_eq!(json["observation_count"], 1);
}

#[tokio::test]
async fn post_actions_run_without_auth_headers_stays_placeholder_deterministic() {
    let app = router();
    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .body(Body::from(minimal_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_response.status(), StatusCode::ACCEPTED);

    let evidence_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/run_req_http_01/evidence")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(evidence_response.status(), StatusCode::OK);
    let evidence_body = to_bytes(evidence_response.into_body(), usize::MAX).await.unwrap();
    let evidence_json: Value = serde_json::from_slice(&evidence_body).unwrap();
    assert_eq!(
        evidence_json["policy_summary"]["reason_code"],
        "ok_auth_placeholder_none"
    );
}

#[tokio::test]
async fn post_actions_run_passes_header_auth_metadata_into_policy_input() {
    let app = router();
    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .header("x-mova-auth-mode", "placeholder")
                .header("x-mova-actor-id", "agent_header_01")
                .header("x-mova-token-ref", "token:header:01")
                .header("x-mova-scopes", "actions.run,actions.validate")
                .body(Body::from(minimal_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_response.status(), StatusCode::ACCEPTED);

    let evidence_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/run_req_http_01/evidence")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(evidence_response.status(), StatusCode::OK);
    let evidence_body = to_bytes(evidence_response.into_body(), usize::MAX).await.unwrap();
    let evidence_json: Value = serde_json::from_slice(&evidence_body).unwrap();
    assert_eq!(
        evidence_json["policy_summary"]["reason_code"],
        "ok_auth_placeholder_header"
    );
}

#[tokio::test]
async fn post_actions_run_rejects_unverified_production_auth() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .header("x-mova-auth-mode", "production")
                .header("x-mova-token-ref", "token://untrusted/agent_001")
                .header("x-mova-scopes", "actions.run")
                .body(Body::from(minimal_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "authorization_failed");
    assert!(
        json["error"]["details"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d.as_str().unwrap().contains("auth_unverified"))
    );
}

#[tokio::test]
async fn post_actions_run_rejects_production_auth_without_required_scope() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .header("x-mova-auth-mode", "production")
                .header("x-mova-token-ref", "token://mova-trusted/agent_001?aud=mova-agent-api")
                .header("x-mova-scopes", "actions.validate")
                .body(Body::from(minimal_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "authorization_failed");
    assert!(
        json["error"]["details"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d.as_str().unwrap().contains("scope_denied"))
    );
}

#[tokio::test]
async fn post_actions_run_allows_verified_production_auth() {
    let app = router();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .header("x-mova-auth-mode", "production")
                .header("x-mova-token-ref", "token://mova-trusted/agent_001?aud=mova-agent-api")
                .header("x-mova-scopes", "actions.run")
                .body(Body::from(minimal_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let evidence_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/run_req_http_01/evidence")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(evidence_response.status(), StatusCode::OK);
    let evidence_body = to_bytes(evidence_response.into_body(), usize::MAX).await.unwrap();
    let evidence_json: Value = serde_json::from_slice(&evidence_body).unwrap();
    assert_eq!(evidence_json["policy_summary"]["reason_code"], "authorized");
}

#[tokio::test]
async fn post_actions_run_uses_explicit_invalid_verifier_config_path() {
    let invalid_config = AuthTrustConfig {
        verifier_kind: "unsupported".to_string(),
        trusted_issuers: vec!["mova-trusted".to_string()],
        trusted_audiences: vec!["mova-agent-api".to_string()],
        allowed_scopes: vec!["actions.run".to_string()],
    };
    let verifier: Arc<dyn AuthVerifier> = Arc::from(create_auth_verifier(&invalid_config));
    let app = router_with_state(AppState::with_auth_verifier(verifier));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .header("x-mova-auth-mode", "production")
                .header("x-mova-token-ref", "token://mova-trusted/agent_001?aud=mova-agent-api")
                .header("x-mova-scopes", "actions.run")
                .body(Body::from(minimal_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "authorization_failed");
    assert!(
        json["error"]["details"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d.as_str().unwrap().contains("auth_unverified"))
    );
}

#[tokio::test]
async fn get_run_returns_status_for_created_run() {
    let app = router();
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .body(Body::from(minimal_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/run_req_http_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["run_id"], "run_req_http_01");
    assert_eq!(json["status"], "completed");
    assert_eq!(json["trace_ref"], "trace:req_http_01");
    assert_eq!(json["observation_count"], 1);
}

#[tokio::test]
async fn get_run_evidence_returns_evidence_for_created_run() {
    let app = router();
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .body(Body::from(minimal_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/run_req_http_01/evidence")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["run_id"], "run_req_http_01");
    assert_eq!(json["trace_ref"], "trace:req_http_01");
}

#[tokio::test]
async fn get_run_and_evidence_are_status_consistent() {
    let app = router();
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .body(Body::from(minimal_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();

    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/run_req_http_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_response.status(), StatusCode::OK);
    let run_body = to_bytes(run_response.into_body(), usize::MAX).await.unwrap();
    let run_json: Value = serde_json::from_slice(&run_body).unwrap();

    let evidence_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/run_req_http_01/evidence")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(evidence_response.status(), StatusCode::OK);
    let evidence_body = to_bytes(evidence_response.into_body(), usize::MAX).await.unwrap();
    let evidence_json: Value = serde_json::from_slice(&evidence_body).unwrap();

    assert_eq!(run_json["status"], evidence_json["status"]);
    assert_eq!(
        run_json["observation_count"],
        serde_json::json!(evidence_json["observation_refs"].as_array().unwrap().len())
    );
}

#[tokio::test]
async fn get_run_evidence_returns_not_found_error_shape() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/run_missing/evidence")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "run_not_found");
}

#[tokio::test]
async fn get_run_returns_not_found_error_shape() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/run_missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "run_not_found");
}

#[tokio::test]
async fn post_actions_run_returns_validation_error_shape() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"request_id":"req_bad"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn post_actions_run_validation_failure_does_not_create_run_snapshot() {
    let app = router();
    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .body(Body::from(invalid_dual_input_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_response.status(), StatusCode::BAD_REQUEST);

    let get_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/run_req_http_dual_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
    let body = to_bytes(get_response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "run_not_found");
}

#[tokio::test]
async fn post_actions_validate_returns_semantic_validation_error_shape() {
    let app = router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/validate")
                .header("content-type", "application/json")
                .body(Body::from(invalid_dual_input_request_body()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
}
