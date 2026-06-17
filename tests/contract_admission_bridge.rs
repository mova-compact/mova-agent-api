use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use mova_agent_api::http::router;
use serde_json::{json, Value};
use tower::util::ServiceExt;

fn contract_flow_with_gate() -> Value {
    json!({
        "version": "1.0",
        "description": "bridge test flow",
        "entry": "fetch",
        "steps": [
            {
                "id": "fetch",
                "operation_id": "op_fetch",
                "step_type": "connector_action",
                "execution_mode": "DETERMINISTIC",
                "connector": {
                    "name":"fixture_data_source",
                    "endpoint_ref":"webhook_site_test",
                    "method":"POST",
                    "side_effect_intent":"external_network"
                },
                "next": {
                    "error": {"step":"gate"},
                    "default": {"step":"deliver"}
                }
            },
            {
                "id": "gate",
                "operation_id": "op_gate",
                "step_type": "human_gate",
                "execution_mode": "HUMAN_GATE",
                "next": {
                    "approve": {"step":"deliver"},
                    "reject": {"terminal":"blocked"}
                }
            },
            {
                "id": "deliver",
                "operation_id": "op_deliver",
                "step_type": "connector_action",
                "execution_mode": "DETERMINISTIC",
                "connector": {
                    "name":"telegram",
                    "endpoint_ref":"webhook_site_test",
                    "method":"POST",
                    "side_effect_intent":"external_network"
                },
                "next": {"default":{"terminal":"completed"}}
            }
        ]
    })
}

fn contract_manifest() -> Value {
    json!({
        "contract_id": "fixture_contract_admission_bridge_v0",
        "contract_name": "Fixture Contract Admission Bridge",
        "version": "1.0.0",
        "layer": "operator",
        "skill_id": "fixture_contract_admission_bridge",
        "description": "Fixture contract package for package-native admission tests.",
        "author": "tests",
        "spec_version": "mova-contract-spec@1.0",
        "execution_runtime": "mova-agent-api",
        "execution_type": "agent",
        "primary_outputs": ["delivery_result"],
        "terminal_outcomes": ["completed", "blocked"],
        "step_count": 3,
        "ai_atomic_steps": 0,
        "deterministic_steps": 2,
        "human_gate_steps": 1,
        "estimated_cost": "Low",
        "files": {
            "flow": "flow.json",
            "policy": "policy.json",
            "connectors": "connector_requirements.json",
            "evidence": "evidence_expectations.json"
        }
    })
}

#[tokio::test]
async fn contract_bridge_register_list_inspect_run_and_decision() {
    let app = router();

    let register_payload = json!({
        "contract_id": "fixture_contract_admission_bridge_v0",
        "execution_type": "agent",
        "mode": "local_packaged",
        "manifest": contract_manifest(),
        "flow_json": contract_flow_with_gate(),
        "connector_requirements": {
            "contract_id": "fixture_contract_admission_bridge_v0",
            "connectors": [{"name":"fixture_data_source"},{"name":"telegram"}]
        },
        "policy": {"allowed_actions":["x"],"forbidden_actions":["y"]},
        "evidence_expectations": {"audit_expectations":["final_run_status"]}
    });
    let response = app
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
    assert_eq!(response.status(), StatusCode::CREATED);

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/contracts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = to_bytes(list_response.into_body(), usize::MAX).await.unwrap();
    let list_json: Value = serde_json::from_slice(&list_body).unwrap();
    assert!(
        list_json["contracts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["contract_id"] == "fixture_contract_admission_bridge_v0")
    );

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/contracts/fixture_contract_admission_bridge_v0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let run_wait_payload = json!({
        "run_id": "ctrun_bridge_wait_01",
        "trace_ref": "trace:ctrun_bridge_wait_01",
        "input_payload": {
            "outcomes": {"fetch": "error"}
        }
    });
    let run_wait_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_contract_admission_bridge_v0/run")
                .header("content-type", "application/json")
                .body(Body::from(run_wait_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_wait_response.status(), StatusCode::ACCEPTED);
    let run_wait_body = to_bytes(run_wait_response.into_body(), usize::MAX).await.unwrap();
    let run_wait_json: Value = serde_json::from_slice(&run_wait_body).unwrap();
    assert_eq!(run_wait_json["status"], "waiting_human");
    assert_eq!(run_wait_json["waiting_for_human"], true);

    let decision_reject = json!({"decision":"reject"});
    let reject_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/runs/ctrun_bridge_wait_01/decision")
                .header("content-type", "application/json")
                .body(Body::from(decision_reject.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reject_response.status(), StatusCode::OK);
    let reject_body = to_bytes(reject_response.into_body(), usize::MAX).await.unwrap();
    let reject_json: Value = serde_json::from_slice(&reject_body).unwrap();
    assert_eq!(reject_json["status"], "blocked");

    let run_success_payload = json!({
        "run_id": "ctrun_bridge_success_01",
        "trace_ref": "trace:ctrun_bridge_success_01",
        "input_payload": {
            "outcomes": {"fetch": "default"}
        }
    });
    let run_success_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/fixture_contract_admission_bridge_v0/run")
                .header("content-type", "application/json")
                .body(Body::from(run_success_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_success_response.status(), StatusCode::ACCEPTED);
    let run_success_body = to_bytes(run_success_response.into_body(), usize::MAX).await.unwrap();
    let run_success_json: Value = serde_json::from_slice(&run_success_body).unwrap();
    assert_eq!(run_success_json["status"], "completed");
}

#[tokio::test]
async fn contract_register_rejects_github_source_without_commit_sha() {
    let app = router();
    let payload = json!({
        "mode": "github_source",
        "contract_id": "fixture_contract_admission_bridge_v0",
        "execution_type": "agent",
        "source_url": "https://github.com/mova-compact/fixture-contracts",
        "contract_path": "contracts/admission-bridge-fixture"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn contract_register_rejects_non_github_source() {
    let app = router();
    let payload = json!({
        "mode": "github_source",
        "contract_id": "fixture_contract_admission_bridge_v0",
        "execution_type": "agent",
        "source_url": "https://example.com/contracts",
        "commit_sha": "3caaaef",
        "contract_path": "contracts/admission-bridge-fixture"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn contract_register_accepts_local_packaged_inline_mode() {
    let app = router();
    let payload = json!({
        "mode": "local_packaged",
        "contract_id": "fixture_contract_admission_bridge_v0",
        "execution_type": "agent",
        "manifest": contract_manifest(),
        "flow_json": contract_flow_with_gate()
    });
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["mode"], "local_packaged");
}

#[tokio::test]
async fn contract_register_accepts_github_source_with_commit_pin() {
    let app = router();
    let payload = json!({
        "mode": "github_source",
        "contract_id": "fixture_contract_admission_bridge_v0",
        "execution_type": "agent",
        "source_url": "https://github.com/mova-compact/fixture-contracts",
        "commit_sha": "3caaaef",
        "contract_path": "contracts/admission-bridge-fixture"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/register")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
