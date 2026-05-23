//! HTTP transport adapter skeleton for MOVA Agent API V0.
//!
//! This module is transport-only and delegates to existing module boundaries.

use crate::connectors::build_connector_call;
use crate::evidence::{build_evidence_response, RunStatus};
use crate::execution::FlatExecutionPlan;
use crate::observation::{ObservationJournal, ObservationRecord};
use crate::policy::{AdmissionDecision, PolicyAdmission};
use crate::request::{parse_request_envelope, validate_request_envelope, AuthContext, RequestValidationError};
use crate::auth::{create_auth_verifier, AuthTrustConfig, AuthVerifier};
use crate::storage::{create_run_store, RunSnapshot, RunStore, StorageConfig, StorageError};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionMode {
    AdapterBoundaryInMemoryDefault,
    AdapterBoundaryFileBackedLocal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthPlaceholder {
    pub auth_mode: String,
    pub enforced: bool,
}

#[derive(Clone)]
pub struct AppState {
    run_store: Arc<dyn RunStore>,
    auth_verifier: Arc<dyn AuthVerifier>,
}

#[derive(Debug, Clone, Serialize)]
struct CapabilitiesResponse {
    action_types: Vec<String>,
    policy_decisions: Vec<String>,
    execution_path: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ValidateResponse {
    valid: bool,
}

#[derive(Debug, Clone, Serialize)]
struct RunResponse {
    run_id: String,
    status: String,
    trace_ref: String,
    observation_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct RunStatusResponse {
    run_id: String,
    status: String,
    trace_ref: String,
    observation_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ErrorResponse {
    error: ApiError,
}

#[derive(Debug, Clone, Serialize)]
struct ApiError {
    code: String,
    message: String,
    details: Vec<String>,
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
        let config = AuthTrustConfig::default_v0();
        let verifier: Arc<dyn AuthVerifier> = Arc::from(create_auth_verifier(&config));
        let storage = StorageConfig::in_memory_default();
        let run_store: Arc<dyn RunStore> = Arc::from(create_run_store(&storage));
        Self {
            run_store,
            auth_verifier: verifier,
        }
    }

    pub fn with_auth_verifier(auth_verifier: Arc<dyn AuthVerifier>) -> Self {
        let storage = StorageConfig::in_memory_default();
        let run_store: Arc<dyn RunStore> = Arc::from(create_run_store(&storage));
        Self {
            run_store,
            auth_verifier,
        }
    }

    pub fn with_auth_and_store(auth_verifier: Arc<dyn AuthVerifier>, run_store: Arc<dyn RunStore>) -> Self {
        Self {
            run_store,
            auth_verifier,
        }
    }

    pub fn retention_mode(&self) -> RetentionMode {
        RetentionMode::AdapterBoundaryInMemoryDefault
    }
}

pub fn auth_placeholder() -> AuthPlaceholder {
    AuthPlaceholder {
        auth_mode: "placeholder".to_string(),
        enforced: false,
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
        execution_path: vec![
            "agent_request".to_string(),
            "action_model".to_string(),
            "policy_admission".to_string(),
            "flat_execution".to_string(),
            "connector_call".to_string(),
            "observation_write".to_string(),
            "evidence_response".to_string(),
        ],
    })
}

async fn post_actions_validate(headers: HeaderMap, Json(payload): Json<Value>) -> impl IntoResponse {
    let payload = with_auth_from_headers(payload, &headers);
    match parse_request_envelope(payload) {
        Ok(envelope) => {
            let semantic_errors = validate_request_envelope(&envelope);
            if semantic_errors.is_empty() {
                (
                    StatusCode::OK,
                    Json(ValidateResponse {
                        valid: true,
                    }),
                )
                    .into_response()
            } else {
                bad_request(
                    "validation_failed",
                    "request validation failed",
                    semantic_errors
                        .iter()
                        .map(render_validation_error)
                        .collect::<Vec<_>>(),
                )
                    .into_response()
            }
        }
        Err(err) => bad_request("validation_failed", "request parsing failed", vec![format!("payload: {err}")]).into_response(),
    }
}

fn bad_request(code: &str, message: &str, details: Vec<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: ApiError {
                code: code.to_string(),
                message: message.to_string(),
                details,
            },
        }),
    )
}

fn not_found_run(run_id: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: ApiError {
                code: "run_not_found".to_string(),
                message: "run was not found".to_string(),
                details: vec![format!("run_id: {run_id}")],
            },
        }),
    )
}

fn storage_unavailable(err: &StorageError) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: ApiError {
                code: "storage_unavailable".to_string(),
                message: "storage adapter failed".to_string(),
                details: vec![
                    format!("storage_code: {}", err.code),
                    format!("storage_message: {}", err.message),
                ],
            },
        }),
    )
}

fn render_validation_error(error: &RequestValidationError) -> String {
    format!("{}: {}", error.field, error.message)
}

async fn post_actions_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let payload = with_auth_from_headers(payload, &headers);
    let envelope = match parse_request_envelope(payload) {
        Ok(value) => value,
        Err(err) => return bad_request("validation_failed", "request parsing failed", vec![format!("payload: {err}")]).into_response(),
    };
    let semantic_errors = validate_request_envelope(&envelope);
    if !semantic_errors.is_empty() {
        return bad_request(
            "validation_failed",
            "request validation failed",
            semantic_errors
                .iter()
                .map(render_validation_error)
                .collect::<Vec<_>>(),
        )
        .into_response();
    }

    let run_id = format!("run_{}", envelope.request_id);
    let admission = PolicyAdmission::from_auth_context(
        format!("adm_{}", envelope.request_id),
        envelope.action.action_id.clone(),
        "policy.default.v0".to_string(),
        "actions.run",
        envelope.auth_context.clone(),
        state.auth_verifier.as_ref(),
    );
    if admission.decision != AdmissionDecision::Allow {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: ApiError {
                    code: "authorization_failed".to_string(),
                    message: "policy authorization failed".to_string(),
                    details: vec![
                        format!("reason_code: {}", admission.reason_code),
                        format!("decision: {:?}", admission.decision),
                    ],
                },
            }),
        )
            .into_response();
    }

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
        evidence,
        observations: journal.records().to_vec(),
    };
    let trace_ref = snapshot.evidence.trace_ref.clone();
    let observation_count = snapshot.evidence.observation_refs.len();
    if let Err(err) = state.run_store.put_snapshot(snapshot) {
        return storage_unavailable(&err).into_response();
    }

    (
        StatusCode::ACCEPTED,
        Json(RunResponse {
            run_id,
            status: RunStatus::Completed.as_str().to_string(),
            trace_ref,
            observation_count,
        }),
    )
        .into_response()
}

fn with_auth_from_headers(mut payload: Value, headers: &HeaderMap) -> Value {
    if let Some(obj) = payload.as_object_mut() {
        if obj.get("auth_context").is_none() {
            if let Some(auth_context) = auth_context_from_headers(headers) {
                obj.insert("auth_context".to_string(), serde_json::to_value(auth_context).expect("auth context to serialize"));
            }
        }
    }
    payload
}

fn auth_context_from_headers(headers: &HeaderMap) -> Option<AuthContext> {
    let mode = header_value(headers, "x-mova-auth-mode");
    let actor_id = header_value(headers, "x-mova-actor-id");
    let token_ref = header_value(headers, "x-mova-token-ref");
    let source = header_value(headers, "x-mova-auth-source").or_else(|| mode.as_ref().map(|_| "header".to_string()));
    let scopes = header_value(headers, "x-mova-scopes")
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if mode.is_none() && actor_id.is_none() && token_ref.is_none() && source.is_none() && scopes.is_empty() {
        return None;
    }

    Some(AuthContext {
        mode: mode.unwrap_or_else(|| "placeholder".to_string()),
        actor_id,
        token_ref,
        scopes,
        source,
        verified: false,
    })
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

async fn get_run(State(state): State<AppState>, Path(run_id): Path<String>) -> impl IntoResponse {
    match state.run_store.get_snapshot(&run_id) {
        Err(err) => storage_unavailable(&err).into_response(),
        Ok(None) => not_found_run(&run_id).into_response(),
        Ok(Some(snapshot)) => (
            StatusCode::OK,
            Json(RunStatusResponse {
                run_id: snapshot.run_id.clone(),
                status: snapshot.evidence.status.as_str().to_string(),
                trace_ref: snapshot.evidence.trace_ref.clone(),
                observation_count: snapshot.evidence.observation_refs.len(),
            }),
        )
            .into_response(),
    }
}

async fn get_run_evidence(State(state): State<AppState>, Path(run_id): Path<String>) -> impl IntoResponse {
    match state.run_store.get_snapshot(&run_id) {
        Err(err) => storage_unavailable(&err).into_response(),
        Ok(None) => not_found_run(&run_id).into_response(),
        Ok(Some(snapshot)) => (
            StatusCode::OK,
            Json(snapshot.evidence.clone()),
        )
            .into_response(),
    }
}
