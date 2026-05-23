use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use mova_agent_api::http::router;
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
}
