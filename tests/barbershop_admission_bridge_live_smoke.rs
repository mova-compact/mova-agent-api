use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use mova_agent_api::http::router;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tower::util::ServiceExt;

fn package_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("deploy-template")
        .join("cloudflare_contract_operator_agent_template_pack_v0")
        .join("contracts")
        .join("barbershop.owner_report.daily.v0")
}

fn read_json(path: PathBuf) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn normalize_strict_contract_run_flow(mut flow: Value) -> Value {
    if let Some(steps) = flow.get_mut("steps").and_then(|value| value.as_array_mut()) {
        for step in steps {
            let Some(step_object) = step.as_object_mut() else {
                continue;
            };
            let step_id = step_object
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            match step_id {
                "resolve_runtime_contract" => {
                    step_object.insert("operation_id".to_string(), serde_json::json!("op_resolve_runtime_contract"));
                    step_object.insert("step_type".to_string(), serde_json::json!("deterministic_action"));
                }
                "fetch_daily_source_data" => {
                    step_object.insert("operation_id".to_string(), serde_json::json!("op_fetch_daily_source_data"));
                    step_object.insert("step_type".to_string(), serde_json::json!("connector_action"));
                    if let Some(connector) = step_object.get_mut("connector").and_then(|value| value.as_object_mut()) {
                        connector.insert("endpoint_ref".to_string(), serde_json::json!("webhook_site_test"));
                        connector.insert("method".to_string(), serde_json::json!("POST"));
                        connector.insert("side_effect_intent".to_string(), serde_json::json!("external_network"));
                    }
                    step_object.insert(
                        "next".to_string(),
                        serde_json::json!({
                            "error": {"step": "human_gate_escalation"},
                            "default": {"step": "validate_data_completeness"}
                        }),
                    );
                }
                "validate_data_completeness" => {
                    step_object.insert("operation_id".to_string(), serde_json::json!("op_validate_data_completeness"));
                    step_object.insert("step_type".to_string(), serde_json::json!("deterministic_action"));
                    step_object.insert(
                        "next".to_string(),
                        serde_json::json!({
                            "error": {"step": "human_gate_escalation"},
                            "default": {"step": "calculate_daily_metrics"}
                        }),
                    );
                }
                "calculate_daily_metrics" => {
                    step_object.insert("operation_id".to_string(), serde_json::json!("op_calculate_daily_metrics"));
                    step_object.insert("step_type".to_string(), serde_json::json!("deterministic_action"));
                }
                "check_anomalies_and_deviation" => {
                    step_object.insert("operation_id".to_string(), serde_json::json!("op_check_anomalies_and_deviation"));
                    step_object.insert("step_type".to_string(), serde_json::json!("deterministic_action"));
                    step_object.insert(
                        "next".to_string(),
                        serde_json::json!({
                            "default": {"step": "compose_owner_report_ai"}
                        }),
                    );
                }
                "compose_owner_report_ai" => {
                    step_object.insert("operation_id".to_string(), serde_json::json!("op_compose_owner_report_ai"));
                    step_object.insert("step_type".to_string(), serde_json::json!("deterministic_action"));
                    step_object.insert(
                        "next".to_string(),
                        serde_json::json!({
                            "error": {"step": "human_gate_escalation"},
                            "default": {"step": "deliver_telegram_report"}
                        }),
                    );
                }
                "human_gate_escalation" => {
                    step_object.insert("operation_id".to_string(), serde_json::json!("op_human_gate_escalation"));
                    step_object.insert("step_type".to_string(), serde_json::json!("human_gate"));
                    step_object.insert(
                        "next".to_string(),
                        serde_json::json!({
                            "approve": {"step": "deliver_telegram_report"},
                            "reject": {"terminal": "blocked"}
                        }),
                    );
                }
                "deliver_telegram_report" => {
                    step_object.insert("operation_id".to_string(), serde_json::json!("op_deliver_telegram_report"));
                    step_object.insert("step_type".to_string(), serde_json::json!("connector_action"));
                    if let Some(connector) = step_object.get_mut("connector").and_then(|value| value.as_object_mut()) {
                        connector.insert("endpoint_ref".to_string(), serde_json::json!("webhook_site_test"));
                        connector.insert("method".to_string(), serde_json::json!("POST"));
                        connector.insert("side_effect_intent".to_string(), serde_json::json!("external_network"));
                    }
                    step_object.insert(
                        "next".to_string(),
                        serde_json::json!({
                            "error": {"terminal": "failed"},
                            "default": {"step": "finalize_evidence_audit"}
                        }),
                    );
                }
                "finalize_evidence_audit" => {
                    step_object.insert("operation_id".to_string(), serde_json::json!("op_finalize_evidence_audit"));
                    step_object.insert("step_type".to_string(), serde_json::json!("deterministic_action"));
                }
                _ => {}
            }
        }
    }
    flow
}

#[tokio::test]
async fn barbershop_bridge_live_smoke_e2e() {
    let app = router();
    let root = package_root();
    let flow = normalize_strict_contract_run_flow(read_json(root.join("flow.json")));
    let manifest = read_json(root.join("manifest.json"));
    let policy = read_json(root.join("policy.json"));
    let connectors = read_json(root.join("connector_requirements.json"));
    let evidence = read_json(root.join("evidence_expectations.json"));
    let open_questions = read_json(root.join("open_questions.json"));

    let register_payload = serde_json::json!({
        "contract_id": "barbershop.owner_report.daily.v0",
        "execution_type": "agent",
        "inline_flow_json": flow,
        "manifest": manifest,
        "policy": policy,
        "connector_requirements": connectors,
        "evidence_expectations": evidence,
        "open_questions": open_questions
    });
    let register_response = app
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
    assert_eq!(register_response.status(), StatusCode::CREATED);

    let list_response = app
        .clone()
        .oneshot(Request::builder().method(Method::GET).uri("/contracts").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_json: Value = serde_json::from_slice(&to_bytes(list_response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert!(
        list_json["contracts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["contract_id"] == "barbershop.owner_report.daily.v0")
    );

    let inspect_response = app
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
    assert_eq!(inspect_response.status(), StatusCode::OK);

    let guarded_payload = serde_json::json!({
        "run_id": "ctrun_barber_guard_01",
        "trace_ref": "trace:ctrun_barber_guard_01",
                "input_payload": {
                    "outcomes": {
                "fetch_daily_source_data": "error"
                    }
                }
            });
    let guarded_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/barbershop.owner_report.daily.v0/run")
                .header("content-type", "application/json")
                .body(Body::from(guarded_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(guarded_response.status(), StatusCode::ACCEPTED);
    let guarded_json: Value = serde_json::from_slice(&to_bytes(guarded_response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(guarded_json["status"], "waiting_human");

    let pre_decision_evidence = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/ctrun_barber_guard_01/evidence")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pre_decision_evidence.status(), StatusCode::OK);

    let reject_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/runs/ctrun_barber_guard_01/decision")
                .header("content-type", "application/json")
                .body(Body::from("{\"decision\":\"reject\"}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reject_response.status(), StatusCode::OK);
    let reject_json: Value = serde_json::from_slice(&to_bytes(reject_response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(reject_json["status"], "blocked");

    let approve_payload = serde_json::json!({
        "run_id": "ctrun_barber_approve_01",
        "trace_ref": "trace:ctrun_barber_approve_01",
                "input_payload": {
                    "outcomes": {
                "fetch_daily_source_data": "error"
                    }
                }
            });
    let approve_run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/barbershop.owner_report.daily.v0/run")
                .header("content-type", "application/json")
                .body(Body::from(approve_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approve_run_response.status(), StatusCode::ACCEPTED);
    let approve_decision_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/runs/ctrun_barber_approve_01/decision")
                .header("content-type", "application/json")
                .body(Body::from("{\"decision\":\"approve\"}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approve_decision_response.status(), StatusCode::OK);
    let approve_json: Value = serde_json::from_slice(&to_bytes(approve_decision_response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(approve_json["status"], "completed");

    let safe_payload = serde_json::json!({
        "run_id": "ctrun_barber_safe_01",
        "trace_ref": "trace:ctrun_barber_safe_01",
        "input_payload": {
            "outcomes": {
                "fetch_daily_source_data": "default",
                "validate_data_completeness": "default",
                "check_anomalies_and_deviation": "default",
                "compose_owner_report_ai": "default",
                "deliver_telegram_report": "default"
            }
        }
    });
    let safe_run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/contracts/barbershop.owner_report.daily.v0/run")
                .header("content-type", "application/json")
                .body(Body::from(safe_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(safe_run_response.status(), StatusCode::ACCEPTED);
    let safe_json: Value = serde_json::from_slice(&to_bytes(safe_run_response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(safe_json["status"], "completed");

    let safe_evidence_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/runs/ctrun_barber_safe_01/evidence")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(safe_evidence_response.status(), StatusCode::OK);
    let safe_evidence_json: Value = serde_json::from_slice(&to_bytes(safe_evidence_response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let markers = safe_evidence_json["evidence"]["connector_result"]["evidence_markers"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    for expected in [
        "external_data_fetch_result",
        "deterministic_metrics_result",
        "anomaly_or_deviation_check_result",
        "ai_report_draft",
        "telegram_delivery_result",
        "final_run_status",
    ] {
        assert!(markers.contains(&expected), "missing marker {expected}");
    }
}
