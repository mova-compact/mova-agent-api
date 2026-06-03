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
                "step_type": "external_resource_call",
                "execution_mode": "DETERMINISTIC",
                "connector": {"name":"barbershop_daily_data_source"},
                "next": {
                    "source_error": {"step":"gate"},
                    "default": {"step":"deliver"}
                }
            },
            {
                "id": "gate",
                "step_type": "human_gate_escalation",
                "execution_mode": "HUMAN_GATE",
                "next": {
                    "approve": {"step":"deliver"},
                    "reject": {"terminal":"stopped_by_human_gate"}
                }
            },
            {
                "id": "deliver",
                "step_type": "telegram_delivery",
                "execution_mode": "DETERMINISTIC",
                "connector": {"name":"telegram"},
                "next": {"default":{"terminal":"completed"}}
            }
        ]
    })
}

#[tokio::test]
async fn contract_bridge_register_list_inspect_run_and_decision() {
    let app = router();

    let register_payload = json!({
        "contract_id": "barbershop.owner_report.daily.v0",
        "execution_type": "agent",
        "inline_flow_json": contract_flow_with_gate(),
        "connector_requirements": {
            "contract_id": "barbershop.owner_report.daily.v0",
            "connectors": [{"name":"barbershop_daily_data_source"},{"name":"telegram"}]
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
            .any(|item| item["contract_id"] == "barbershop.owner_report.daily.v0")
    );

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/contracts/barbershop.owner_report.daily.v0")
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
            "outcomes": {"fetch": "source_error"}
        }
    });
    let run_wait_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/barbershop.owner_report.daily.v0/run")
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
    assert_eq!(reject_json["status"], "stopped_by_human_gate");

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
                .uri("/contracts/barbershop.owner_report.daily.v0/run")
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
        "contract_id": "barbershop.owner_report.daily.v0",
        "execution_type": "agent",
        "source_url": "https://github.com/mova-compact/barbershop-contracts",
        "contract_path": "contracts/barbershop-owner-report-daily"
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
        "contract_id": "barbershop.owner_report.daily.v0",
        "execution_type": "agent",
        "source_url": "https://example.com/contracts",
        "commit_sha": "3caaaef",
        "contract_path": "contracts/barbershop-owner-report-daily"
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
        "contract_id": "barbershop.owner_report.daily.v0",
        "execution_type": "agent",
        "inline_flow_json": contract_flow_with_gate()
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
        "contract_id": "barbershop.owner_report.daily.v0",
        "execution_type": "agent",
        "source_url": "https://github.com/mova-compact/barbershop-contracts",
        "commit_sha": "3caaaef",
        "contract_path": "contracts/barbershop-owner-report-daily"
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
}
