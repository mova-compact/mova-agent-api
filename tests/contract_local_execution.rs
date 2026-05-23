use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use mova_agent_api::contracts::{map_contract_to_agent_request, LocalContractRegistry};
use mova_agent_api::http::router;
use serde_json::Value;
use std::path::PathBuf;
use tower::util::ServiceExt;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("operator_terminal_smoke_v0")
}

#[tokio::test]
async fn operator_terminal_smoke_contract_executes_through_v0_http_path() {
    let registry = LocalContractRegistry;
    let contract = registry
        .load_fixture_dir("operator_terminal_smoke_v0", &fixture_dir())
        .expect("fixture contract must load");

    let request_json =
        map_contract_to_agent_request(&contract, "req_contract_local_01", "agent_contract_local_01");
    let request_body = serde_json::to_string(&request_json).unwrap();

    let app = router();

    let validate_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/validate")
                .header("content-type", "application/json")
                .body(Body::from(request_body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(validate_response.status(), StatusCode::OK);
    let validate_body = to_bytes(validate_response.into_body(), usize::MAX).await.unwrap();
    let validate_json: Value = serde_json::from_slice(&validate_body).unwrap();
    assert_eq!(validate_json["valid"], true);

    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/actions/run")
                .header("content-type", "application/json")
                .body(Body::from(request_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_response.status(), StatusCode::ACCEPTED);
    let run_body = to_bytes(run_response.into_body(), usize::MAX).await.unwrap();
    let run_json: Value = serde_json::from_slice(&run_body).unwrap();
    assert_eq!(run_json["run_id"], "run_req_contract_local_01");
    assert_eq!(run_json["status"], "completed");

    let status_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/run_req_contract_local_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status_response.status(), StatusCode::OK);

    let evidence_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/run_req_contract_local_01/evidence")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(evidence_response.status(), StatusCode::OK);
    let evidence_body = to_bytes(evidence_response.into_body(), usize::MAX).await.unwrap();
    let evidence_json: Value = serde_json::from_slice(&evidence_body).unwrap();

    assert_eq!(evidence_json["run_id"], "run_req_contract_local_01");
    assert_eq!(evidence_json["status"], "completed");
    assert_eq!(
        evidence_json["result"]["connector_status"],
        "completed"
    );
    assert_eq!(
        evidence_json["evidence"]["connector_result"]["side_effect_performed"],
        false
    );
    assert_eq!(
        evidence_json["policy_summary"]["decision"],
        "allow"
    );
}
