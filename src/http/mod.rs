//! HTTP transport adapter skeleton for MOVA Agent API V0.
//!
//! This module is transport-only and delegates to existing module boundaries.

use crate::connectors::build_connector_call;
use crate::evidence::{build_evidence_response, EvidenceResponse, RunStatus};
use crate::execution::FlatExecutionPlan;
use crate::observation::{ObservationJournal, ObservationRecord};
use crate::policy::{AdmissionDecision, PolicyAdmission};
use crate::request::parse_request_envelope;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct AppState {
    runs: Arc<Mutex<HashMap<String, RunSnapshot>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunSnapshot {
    run_id: String,
    status: RunStatus,
    evidence: EvidenceResponse,
}

#[derive(Debug, Clone, Serialize)]
struct CapabilitiesResponse {
    action_types: Vec<String>,
    policy_decisions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ValidateResponse {
    valid: bool,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RunResponse {
    run_id: String,
    status: String,
}

#[derive(Debug, Clone, Serialize)]
struct RunStatusResponse {
    run_id: String,
    status: String,
}

pub fn router() -> Router {
    router_with_state(AppState::new())
}

pub fn router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/capabilities", get(get_capabilities))
        .route("/actions/validate", post(post_actions_validate))
        .route("/actions/run", post(post_actions_run))
        .route("/runs/:run_id", get(get_run))
        .route("/runs/:run_id/evidence", get(get_run_evidence))
        .with_state(state)
}

impl AppState {
    pub fn new() -> Self {
        Self {
            runs: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

async fn get_capabilities() -> Json<CapabilitiesResponse> {
    Json(CapabilitiesResponse {
        action_types: vec!["validate_document".to_string()],
        policy_decisions: vec![
            "allow".to_string(),
            "deny".to_string(),
            "require_review".to_string(),
            "redact".to_string(),
        ],
    })
}

async fn post_actions_validate(Json(payload): Json<Value>) -> impl IntoResponse {
    match parse_request_envelope(payload) {
        Ok(_) => (
            StatusCode::OK,
            Json(ValidateResponse {
                valid: true,
                errors: Vec::new(),
            }),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(ValidateResponse {
                valid: false,
                errors: vec![err.to_string()],
            }),
        ),
    }
}

async fn post_actions_run(State(state): State<AppState>, Json(payload): Json<Value>) -> impl IntoResponse {
    let envelope = match parse_request_envelope(payload) {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "validation_failed",
                    "message": err.to_string()
                })),
            )
                .into_response();
        }
    };

    let run_id = format!("run_{}", envelope.request_id);
    let admission = PolicyAdmission::new(
        format!("adm_{}", envelope.request_id),
        envelope.action.action_id.clone(),
        AdmissionDecision::Allow,
        "ok".to_string(),
        "policy.default.v0".to_string(),
    );

    let _plan = FlatExecutionPlan::from_action(run_id.clone(), envelope.action.action_id.clone());
    let connector_call = build_connector_call(
        "connector.docs.v1".to_string(),
        format!("call_{}", envelope.request_id),
        admission.to_summary(),
        "2026-05-23T10:30:00Z".to_string(),
    );

    let mut journal = ObservationJournal::new();
    journal.append(ObservationRecord {
        run_id: run_id.clone(),
        step_id: "step_observation_write".to_string(),
        event_type: "observation.write".to_string(),
        timestamp: "2026-05-23T10:30:01Z".to_string(),
        subject: json!({"call_id": connector_call.call_id}),
        result: json!({"status": "ok"}),
        metadata: json!({}),
        evidence_ref: format!("ev_{}", envelope.request_id),
    });

    let evidence = build_evidence_response(
        run_id.clone(),
        RunStatus::Completed,
        envelope.action.trace_ref.clone(),
        journal.records(),
        admission.to_summary(),
    );

    let snapshot = RunSnapshot {
        run_id: run_id.clone(),
        status: RunStatus::Completed,
        evidence,
    };
    state.runs.lock().expect("state mutex poisoned").insert(run_id.clone(), snapshot);

    (
        StatusCode::ACCEPTED,
        Json(RunResponse {
            run_id,
            status: "completed".to_string(),
        }),
    )
        .into_response()
}

async fn get_run(State(state): State<AppState>, Path(run_id): Path<String>) -> impl IntoResponse {
    let runs = state.runs.lock().expect("state mutex poisoned");
    match runs.get(&run_id) {
        Some(snapshot) => (
            StatusCode::OK,
            Json(RunStatusResponse {
                run_id: snapshot.run_id.clone(),
                status: "completed".to_string(),
            }),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "run_not_found",
                "run_id": run_id
            })),
        )
            .into_response(),
    }
}

async fn get_run_evidence(State(state): State<AppState>, Path(run_id): Path<String>) -> impl IntoResponse {
    let runs = state.runs.lock().expect("state mutex poisoned");
    match runs.get(&run_id) {
        Some(snapshot) => (StatusCode::OK, Json(snapshot.evidence.clone())).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "run_not_found",
                "run_id": run_id
            })),
        )
            .into_response(),
    }
}
