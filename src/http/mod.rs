//! HTTP transport adapter skeleton for MOVA Agent API V0.
//!
//! This module is transport-only and delegates to existing module boundaries.

use crate::connectors::{
    create_connector_executor, ConnectorExecutionConfig, ConnectorExecutionError, ConnectorExecutionRequest,
    ConnectorExecutor, SideEffectIntent,
};
use crate::contract_run::{
    contract_steps_from_flow, gate_for_step, resolve_flow_transition, ContractRunRequest,
    ContractRunState, ContractRunStatus, ContractStepOutcome, ContractTransition,
    ContractRunStatusResponse as CorridorRunStatusResponse,
};
use crate::contract_step::{ContractStepStatus, ContractStepType};
use crate::contracts::{
    extract_outcomes_map, flow_step_by_id, parse_contract_connector_requirements, parse_inline_flow_json,
    pick_next_target, validate_admitted_contract, AdmittedContract, ContractFlowStep, ContractRegistryError,
};
use crate::evidence::{build_contract_run_evidence_response, build_evidence_response, RunStatus};
use crate::execution::FlatExecutionPlan;
use crate::gate::{HumanGate, HumanGateDecision, HumanGateResolutionRequest, HumanGateStatus};
use crate::github_file_bridge::run_barbershop_file_e2e;
use crate::observation::{ObservationJournal, ObservationRecord};
use crate::operation_admission::OperationAdmission;
use crate::policy::{AdmissionDecision, PolicyAdmission};
use crate::request::{parse_request_envelope, validate_request_envelope, AuthContext, RequestValidationError};
use crate::runtime::{
    LocalEnvRuntimeProvider, LocalEnvSecretResolver, RuntimeProvider, RuntimeProviderCapabilities,
    SecretResolver,
};
use crate::secrets::{redact_json, redact_text, SecretRef, SecretRefKind};
use crate::auth::{create_auth_verifier, AuthVerifier};
use crate::storage::{create_run_store, RunSnapshot, RunStore, StorageConfig, StorageError};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use url::Url;

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
    connector_executor: Arc<dyn ConnectorExecutor>,
    secret_resolver: Arc<dyn SecretResolver>,
    runtime_provider_capabilities: RuntimeProviderCapabilities,
    admitted_contracts: Arc<Mutex<HashMap<String, AdmittedContract>>>,
    legacy_contract_runs: Arc<Mutex<HashMap<String, LegacyContractRunState>>>,
    contract_runs: Arc<Mutex<HashMap<String, ContractRunState>>>,
    contract_run_observations: Arc<Mutex<HashMap<String, Vec<ObservationRecord>>>>,
}

#[derive(Debug, Clone, Serialize)]
struct LegacyContractRunState {
    run_id: String,
    contract_id: String,
    current_step_id: String,
    status: String,
    waiting_for_human: bool,
    trace_ref: String,
    outcomes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
struct CapabilitiesResponse {
    action_types: Vec<String>,
    policy_decisions: Vec<String>,
    execution_path: Vec<String>,
    runtime_provider: RuntimeProviderCapabilities,
    contract_run: Value,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegisterContractRequest {
    contract_id: String,
    execution_type: String,
    mode: Option<String>,
    source_url: Option<String>,
    commit_sha: Option<String>,
    contract_path: Option<String>,
    inline_flow_json: Option<Value>,
    manifest: Option<Value>,
    policy: Option<Value>,
    connector_requirements: Option<Value>,
    evidence_expectations: Option<Value>,
    open_questions: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
struct ContractRegisterResponse {
    contract_id: String,
    admitted: bool,
    mode: String,
}

#[derive(Debug, Clone)]
struct ResolvedRegisterSource {
    mode: String,
    source_url: Option<String>,
    commit_sha: Option<String>,
    contract_path: Option<String>,
    flow: Value,
    manifest: Option<Value>,
    policy: Option<Value>,
    evidence_expectations: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
struct ContractSummary {
    contract_id: String,
    execution_type: String,
    has_source_url: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ContractRunResponse {
    run_id: String,
    contract_id: String,
    status: String,
    current_step_id: String,
    waiting_for_human: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContractDecisionRequest {
    decision: String,
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

#[derive(Debug, Clone)]
struct ContractStepConnectorMetadata {
    connector_id: String,
    endpoint_ref: Option<String>,
    method: Option<String>,
    side_effect_intent: Option<SideEffectIntent>,
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
        .route("/contracts/register", post(post_contract_register))
        .route("/contracts", get(get_contracts))
        .route("/contracts/:contract_id", get(get_contract))
        .route("/contracts/:contract_id/run", post(post_contract_run))
        .route("/contracts/:contract_id/runs", post(post_contract_run_start))
        .route("/contracts/runs/:run_id/decision", post(post_contract_run_decision))
        .route("/contract-runs/:run_id", get(get_contract_run_status))
        .route("/contract-runs/:run_id/next", get(get_contract_run_next))
        .route("/contract-runs/:run_id/steps/:step_id/execute", post(post_contract_run_step_execute))
        .route("/contract-runs/:run_id/gates/current", get(get_contract_run_current_gate))
        .route("/contract-runs/:run_id/gates/:gate_id/resolve", post(post_contract_run_gate_resolve))
        .route("/contract-runs/:run_id/evidence", get(get_contract_run_evidence))
        .with_state(state)
}

impl AppState {
    pub fn new() -> Self {
        let provider = LocalEnvRuntimeProvider::new(crate::runtime::RuntimeConfig::deterministic_local_default());
        let runtime = provider
            .load_runtime_config()
            .unwrap_or_else(|_| crate::runtime::RuntimeConfig::deterministic_local_default());
        let verifier: Arc<dyn AuthVerifier> = Arc::from(create_auth_verifier(&runtime.auth));
        let run_store: Arc<dyn RunStore> = Arc::from(create_run_store(&runtime.storage));
        let connector_executor: Arc<dyn ConnectorExecutor> =
            Arc::from(create_connector_executor(&runtime.connectors));
        let secret_resolver: Arc<dyn SecretResolver> = Arc::new(LocalEnvSecretResolver::default());
        let admitted_contracts = Arc::new(Mutex::new(HashMap::new()));
        seed_default_contracts(&admitted_contracts);
        Self {
            run_store,
            auth_verifier: verifier,
            connector_executor,
            secret_resolver,
            runtime_provider_capabilities: provider.capabilities(),
            admitted_contracts,
            legacy_contract_runs: Arc::new(Mutex::new(HashMap::new())),
            contract_runs: Arc::new(Mutex::new(HashMap::new())),
            contract_run_observations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_auth_verifier(auth_verifier: Arc<dyn AuthVerifier>) -> Self {
        let storage = StorageConfig::in_memory_default();
        let run_store: Arc<dyn RunStore> = Arc::from(create_run_store(&storage));
        let connector_config = ConnectorExecutionConfig::deterministic_local_default();
        let connector_executor: Arc<dyn ConnectorExecutor> =
            Arc::from(create_connector_executor(&connector_config));
        let secret_resolver: Arc<dyn SecretResolver> = Arc::new(LocalEnvSecretResolver::default());
        let admitted_contracts = Arc::new(Mutex::new(HashMap::new()));
        seed_default_contracts(&admitted_contracts);
        Self {
            run_store,
            auth_verifier,
            connector_executor,
            secret_resolver,
            runtime_provider_capabilities: RuntimeProviderCapabilities {
                provider_kind: "local_test".to_string(),
                supports_env_loading: false,
                supports_secret_resolution: true,
                supports_live_deploy_binding: false,
            },
            admitted_contracts,
            legacy_contract_runs: Arc::new(Mutex::new(HashMap::new())),
            contract_runs: Arc::new(Mutex::new(HashMap::new())),
            contract_run_observations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_auth_and_store(auth_verifier: Arc<dyn AuthVerifier>, run_store: Arc<dyn RunStore>) -> Self {
        let connector_config = ConnectorExecutionConfig::deterministic_local_default();
        let connector_executor: Arc<dyn ConnectorExecutor> =
            Arc::from(create_connector_executor(&connector_config));
        let secret_resolver: Arc<dyn SecretResolver> = Arc::new(LocalEnvSecretResolver::default());
        let admitted_contracts = Arc::new(Mutex::new(HashMap::new()));
        seed_default_contracts(&admitted_contracts);
        Self {
            run_store,
            auth_verifier,
            connector_executor,
            secret_resolver,
            runtime_provider_capabilities: RuntimeProviderCapabilities {
                provider_kind: "local_test".to_string(),
                supports_env_loading: false,
                supports_secret_resolution: true,
                supports_live_deploy_binding: false,
            },
            admitted_contracts,
            legacy_contract_runs: Arc::new(Mutex::new(HashMap::new())),
            contract_runs: Arc::new(Mutex::new(HashMap::new())),
            contract_run_observations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_boundaries(
        auth_verifier: Arc<dyn AuthVerifier>,
        run_store: Arc<dyn RunStore>,
        connector_executor: Arc<dyn ConnectorExecutor>,
    ) -> Self {
        let secret_resolver: Arc<dyn SecretResolver> = Arc::new(LocalEnvSecretResolver::default());
        let admitted_contracts = Arc::new(Mutex::new(HashMap::new()));
        seed_default_contracts(&admitted_contracts);
        Self {
            run_store,
            auth_verifier,
            connector_executor,
            secret_resolver,
            runtime_provider_capabilities: RuntimeProviderCapabilities {
                provider_kind: "custom".to_string(),
                supports_env_loading: false,
                supports_secret_resolution: true,
                supports_live_deploy_binding: false,
            },
            admitted_contracts,
            legacy_contract_runs: Arc::new(Mutex::new(HashMap::new())),
            contract_runs: Arc::new(Mutex::new(HashMap::new())),
            contract_run_observations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_runtime_boundaries(
        auth_verifier: Arc<dyn AuthVerifier>,
        run_store: Arc<dyn RunStore>,
        connector_executor: Arc<dyn ConnectorExecutor>,
        secret_resolver: Arc<dyn SecretResolver>,
    ) -> Self {
        let admitted_contracts = Arc::new(Mutex::new(HashMap::new()));
        seed_default_contracts(&admitted_contracts);
        Self {
            run_store,
            auth_verifier,
            connector_executor,
            secret_resolver,
            runtime_provider_capabilities: RuntimeProviderCapabilities {
                provider_kind: "custom".to_string(),
                supports_env_loading: false,
                supports_secret_resolution: true,
                supports_live_deploy_binding: false,
            },
            admitted_contracts,
            legacy_contract_runs: Arc::new(Mutex::new(HashMap::new())),
            contract_runs: Arc::new(Mutex::new(HashMap::new())),
            contract_run_observations: Arc::new(Mutex::new(HashMap::new())),
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

async fn get_capabilities(State(state): State<AppState>) -> Json<CapabilitiesResponse> {
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
        runtime_provider: state.runtime_provider_capabilities.clone(),
        contract_run: json!({
            "supported": true,
            "supports_next_step": true,
            "supports_human_gate": true,
            "supports_terminal_evidence": true
        }),
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
                    format!("storage_message: {}", redact_text(&err.message)),
                ],
            },
        }),
    )
}

fn connector_unavailable(err: &ConnectorExecutionError) -> (StatusCode, Json<ErrorResponse>) {
    let status = if err.code == "connector_side_effect_denied" || err.code == "connector_denied" {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::BAD_GATEWAY
    };
    (
        status,
        Json(ErrorResponse {
            error: ApiError {
                code: "connector_execution_failed".to_string(),
                message: "connector execution failed".to_string(),
                details: vec![
                    format!("connector_code: {}", err.code),
                    format!("connector_message: {}", redact_text(&err.message)),
                ],
            },
        }),
    )
}

fn runtime_secret_unavailable(message: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse {
            error: ApiError {
                code: "runtime_secret_resolution_failed".to_string(),
                message: "runtime secret resolution failed".to_string(),
                details: vec![format!("resolution: {}", redact_text(message))],
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
    let connector_id = envelope
        .action
        .connector_context
        .get("connector_id")
        .and_then(|v| v.as_str())
        .unwrap_or("connector.docs.v1")
        .to_string();
    let side_effect_intent = parse_side_effect_intent(&envelope.action.connector_context)
        .unwrap_or(SideEffectIntent::None);
    let credential_refs = extract_credential_refs(&envelope.action.connector_context);
    let mut resolved_ref_count: u64 = 0;
    for secret_ref in &credential_refs {
        match state.secret_resolver.resolve(secret_ref) {
            Ok(Some(_)) => resolved_ref_count += 1,
            Ok(None) => {}
            Err(err) => {
                return runtime_secret_unavailable(&err.message).into_response();
            }
        }
    }
    let connector_request = envelope.action.input_payload.clone().unwrap_or_else(|| json!({}));
    let endpoint_ref = envelope
        .action
        .connector_context
        .get("endpoint_ref")
        .cloned()
        .unwrap_or(Value::Null);
    let method = envelope
        .action
        .connector_context
        .get("method")
        .cloned()
        .unwrap_or(json!("POST"));
    let headers = envelope
        .action
        .connector_context
        .get("headers")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let connector_call = match state.connector_executor.execute(ConnectorExecutionRequest {
        connector_id,
        call_id: format!("call_{}", envelope.request_id),
        side_effect_intent,
        request: redact_json(&json!({
            "target_url": connector_request.get("target_url").cloned().unwrap_or(Value::Null),
            "endpoint_ref": endpoint_ref,
            "method": method,
            "headers": headers,
            "body": connector_request,
            "run_id": run_id.clone(),
            "correlation_id": envelope
                .correlation
                .get("correlation_id")
                .cloned()
                .unwrap_or(Value::Null),
            "trace_ref": envelope.action.trace_ref.clone(),
            "input": connector_request
        })),
        auth_context: redact_json(&serde_json::to_value(&envelope.auth_context).unwrap_or_else(|_| json!({}))),
        credential_refs,
        policy_result: admission.to_summary(),
        started_at: "2026-05-23T10:30:00Z".to_string(),
    }).await {
        Ok(result) => result.call,
        Err(err) => return connector_unavailable(&err).into_response(),
    };

    let mut journal = ObservationJournal::new();
    journal.append(ObservationRecord {
        run_id: run_id.clone(),
        step_id: "step_observation_write".to_string(),
        event_type: "observation.write".to_string(),
        timestamp: "2026-05-23T10:30:01Z".to_string(),
        subject: json!({
            "call_id": connector_call.call_id,
            "connector_id": connector_call.connector_id
        }),
        result: json!({
            "status": "ok",
            "connector_status": connector_call.status,
            "connector_response": redact_json(&connector_call.response)
        }),
        metadata: json!({
            "side_effect_intent": connector_call.side_effect_intent,
            "credential_ref_count": connector_call
                .response
                .get("credential_ref_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            "resolved_ref_count": resolved_ref_count
        }),
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
    if let Err(err) = state.run_store.put_snapshot(snapshot).await {
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

fn parse_side_effect_intent(connector_context: &Value) -> Option<SideEffectIntent> {
    let value = connector_context.get("side_effect_intent")?.as_str()?;
    match value {
        "none" => Some(SideEffectIntent::None),
        "local_only" => Some(SideEffectIntent::LocalOnly),
        "external_network" => Some(SideEffectIntent::ExternalNetwork),
        "destructive" => Some(SideEffectIntent::Destructive),
        _ => None,
    }
}

fn extract_credential_refs(connector_context: &Value) -> Vec<SecretRef> {
    connector_context
        .get("credential_refs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let kind = item.get("kind")?.as_str()?;
                    let reference = item.get("reference")?.as_str()?.to_string();
                    let kind = match kind {
                        "secret_ref" => SecretRefKind::SecretRef,
                        "env_ref" => SecretRefKind::EnvRef,
                        "runtime_secret" => SecretRefKind::RuntimeSecret,
                        _ => return None,
                    };
                    let secret = SecretRef { kind, reference };
                    if secret.validate().is_ok() {
                        Some(secret)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
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
    match state.run_store.get_snapshot(&run_id).await {
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
    match state.run_store.get_snapshot(&run_id).await {
        Err(err) => storage_unavailable(&err).into_response(),
        Ok(None) => not_found_run(&run_id).into_response(),
        Ok(Some(snapshot)) => (
            StatusCode::OK,
            Json(snapshot.evidence.clone()),
        )
            .into_response(),
    }
}

async fn post_contract_register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterContractRequest>,
) -> impl IntoResponse {
    if payload.contract_id.trim().is_empty() {
        return bad_request(
            "contract_registration_invalid",
            "contract registration payload is invalid",
            vec!["contract_id must not be empty".to_string()],
        )
        .into_response();
    }
    let resolved = match resolve_contract_register_source(&payload).await {
        Ok(v) => v,
        Err((code, message, details)) => return bad_request(&code, &message, details).into_response(),
    };

    if let Some(secret_field) = detect_secret_like_json(&resolved.flow, "flow") {
        return bad_request(
            "contract_registration_secret_like_payload",
            "contract package contains secret-looking value",
            vec![secret_field],
        )
        .into_response();
    }
    if let Some(policy) = resolved.policy.as_ref() {
        if let Some(secret_field) = detect_secret_like_json(policy, "policy") {
            return bad_request(
                "contract_registration_secret_like_payload",
                "contract package contains secret-looking value",
                vec![secret_field],
            )
            .into_response();
        }
    }

    let flow = match parse_inline_flow_json(&resolved.flow) {
        Ok(v) => v,
        Err(err) => {
            return bad_request(
                "contract_flow_parse_failed",
                "inline flow parse failed",
                vec![err.message],
            )
            .into_response()
        }
    };

    let connector_requirements = match parse_contract_connector_requirements(payload.connector_requirements.clone()) {
        Ok(v) => v,
        Err(err) => {
            return bad_request(
                &err.code,
                "connector requirements are invalid",
                vec![err.message],
            )
            .into_response()
        }
    };

    let admitted = AdmittedContract {
        contract_id: payload.contract_id.clone(),
        execution_type: payload.execution_type.clone(),
        source_type: Some(resolved.mode.clone()),
        source_url: resolved.source_url.clone(),
        commit_sha: resolved.commit_sha.clone(),
        contract_path: resolved.contract_path.clone(),
        registered_at: Some("2026-05-26T00:00:00Z".to_string()),
        admitted: Some(true),
        manifest: resolved.manifest.clone(),
        flow,
        policy: resolved.policy.clone(),
        connector_requirements,
        evidence_expectations: resolved.evidence_expectations.clone(),
        open_questions: payload.open_questions.clone(),
    };
    if let Err(err) = validate_admitted_contract(&admitted) {
        return bad_request(
            &err.code,
            "contract admission validation failed",
            vec![err.message],
        )
        .into_response();
    }

    {
        let mut contracts = state
            .admitted_contracts
            .lock()
            .expect("contract registry lock poisoned");
        contracts.insert(admitted.contract_id.clone(), admitted);
    }

    (
        StatusCode::CREATED,
        Json(ContractRegisterResponse {
            contract_id: payload.contract_id,
            admitted: true,
            mode: resolved.mode,
        }),
    )
        .into_response()
}

async fn resolve_contract_register_source(
    payload: &RegisterContractRequest,
) -> Result<ResolvedRegisterSource, (String, String, Vec<String>)> {
    let requested_mode = payload
        .mode
        .clone()
        .unwrap_or_else(|| {
            if payload.inline_flow_json.is_some() {
                "inline_flow_json".to_string()
            } else {
                "github_source".to_string()
            }
        })
        .trim()
        .to_string();
    if requested_mode == "inline_flow_json" || requested_mode == "local_packaged" {
        let flow = payload.inline_flow_json.clone().ok_or_else(|| {
            (
                "contract_register_missing_flow".to_string(),
                "inline_flow_json is required".to_string(),
                vec![format!("mode: {requested_mode}")],
            )
        })?;
        return Ok(ResolvedRegisterSource {
            mode: requested_mode,
            source_url: payload.source_url.clone(),
            commit_sha: payload.commit_sha.clone(),
            contract_path: payload.contract_path.clone(),
            flow,
            manifest: payload.manifest.clone(),
            policy: payload.policy.clone(),
            evidence_expectations: payload.evidence_expectations.clone(),
        });
    }
    if requested_mode != "github_source" {
        return Err((
            "contract_registration_invalid".to_string(),
            "unsupported registration mode".to_string(),
            vec![format!("mode: {requested_mode}")],
        ));
    }

    let source_url = payload
        .source_url
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            (
                "contract_registration_invalid".to_string(),
                "github_source requires source_url".to_string(),
                vec!["source_url is missing".to_string()],
            )
        })?;
    if !is_allowed_github_source_url(&source_url) {
        return Err((
            "contract_registration_invalid".to_string(),
            "only github.com and raw.githubusercontent.com are allowed".to_string(),
            vec![format!("source_url: {source_url}")],
        ));
    }
    let commit_sha = payload
        .commit_sha
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            (
                "contract_registration_invalid".to_string(),
                "github_source requires commit_sha pin".to_string(),
                vec!["floating branch is not allowed".to_string()],
            )
        })?;
    if !is_valid_commit_sha(&commit_sha) {
        return Err((
            "contract_registration_invalid".to_string(),
            "commit_sha format is invalid".to_string(),
            vec!["expected hex sha length 7..64".to_string()],
        ));
    }
    let contract_path = payload
        .contract_path
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            (
                "contract_registration_invalid".to_string(),
                "github_source requires contract_path".to_string(),
                vec!["contract_path is missing".to_string()],
            )
        })?;
    if !is_safe_contract_path(&contract_path) {
        return Err((
            "contract_registration_invalid".to_string(),
            "contract_path contains unsafe traversal".to_string(),
            vec![contract_path],
        ));
    }

    let fetched = fetch_github_contract_package(&source_url, &commit_sha, &contract_path).await?;
    Ok(ResolvedRegisterSource {
        mode: requested_mode,
        source_url: Some(source_url),
        commit_sha: Some(commit_sha),
        contract_path: Some(contract_path),
        flow: fetched.flow,
        manifest: Some(fetched.manifest),
        policy: Some(fetched.policy),
        evidence_expectations: Some(fetched.evidence_requirements),
    })
}

#[derive(Debug, Clone)]
struct FetchedGithubPackage {
    manifest: Value,
    flow: Value,
    policy: Value,
    _bindings_example: Value,
    evidence_requirements: Value,
}

async fn fetch_github_contract_package(
    source_url: &str,
    commit_sha: &str,
    contract_path: &str,
) -> Result<FetchedGithubPackage, (String, String, Vec<String>)> {
    let (owner, repo) = parse_github_owner_repo(source_url)?;
    let base = format!(
        "https://raw.githubusercontent.com/{owner}/{repo}/{commit_sha}/{}",
        contract_path.trim_matches('/')
    );
    let manifest = fetch_json_from_url(&format!("{base}/manifest.json")).await?;
    let flow = fetch_json_from_url(&format!("{base}/flow.json")).await?;
    let policy = fetch_json_from_url(&format!("{base}/policy.json")).await?;
    let bindings_example = fetch_json_from_url(&format!("{base}/bindings.example.json")).await?;
    let evidence_requirements = fetch_json_from_url(&format!("{base}/evidence_requirements.json")).await?;
    Ok(FetchedGithubPackage {
        manifest,
        flow,
        policy,
        _bindings_example: bindings_example,
        evidence_requirements,
    })
}

async fn fetch_json_from_url(url: &str) -> Result<Value, (String, String, Vec<String>)> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| {
            (
                "contract_source_fetch_failed".to_string(),
                "failed to fetch contract source".to_string(),
                vec![format!("{url}: {err}")],
            )
        })?;
    if !response.status().is_success() {
        return Err((
            "contract_source_fetch_failed".to_string(),
            "failed to fetch contract source".to_string(),
            vec![format!("{url}: status {}", response.status())],
        ));
    }
    response.json::<Value>().await.map_err(|err| {
        (
            "contract_source_parse_failed".to_string(),
            "fetched contract file is not valid json".to_string(),
            vec![format!("{url}: {err}")],
        )
    })
}

fn parse_github_owner_repo(source_url: &str) -> Result<(String, String), (String, String, Vec<String>)> {
    let parsed = Url::parse(source_url).map_err(|err| {
        (
            "contract_registration_invalid".to_string(),
            "invalid source_url".to_string(),
            vec![err.to_string()],
        )
    })?;
    let host = parsed.host_str().unwrap_or_default();
    let segments = parsed
        .path_segments()
        .map(|s| s.collect::<Vec<_>>())
        .unwrap_or_default();
    if host == "github.com" {
        if segments.len() < 2 {
            return Err((
                "contract_registration_invalid".to_string(),
                "source_url must include owner/repo".to_string(),
                vec![source_url.to_string()],
            ));
        }
        return Ok((segments[0].to_string(), segments[1].to_string()));
    }
    if host == "raw.githubusercontent.com" {
        if segments.len() < 2 {
            return Err((
                "contract_registration_invalid".to_string(),
                "raw source_url must include owner/repo".to_string(),
                vec![source_url.to_string()],
            ));
        }
        return Ok((segments[0].to_string(), segments[1].to_string()));
    }
    Err((
        "contract_registration_invalid".to_string(),
        "unsupported source_url host".to_string(),
        vec![host.to_string()],
    ))
}

fn is_allowed_github_source_url(source_url: &str) -> bool {
    if let Ok(parsed) = Url::parse(source_url) {
        if parsed.scheme() != "https" {
            return false;
        }
        if let Some(host) = parsed.host_str() {
            return host == "github.com" || host == "raw.githubusercontent.com";
        }
    }
    false
}

fn is_valid_commit_sha(value: &str) -> bool {
    let len = value.len();
    if !(7..=64).contains(&len) {
        return false;
    }
    value.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_safe_contract_path(value: &str) -> bool {
    if value.contains("..") || value.contains('\\') {
        return false;
    }
    !value.starts_with('/') && !value.starts_with('.')
}

fn detect_secret_like_json(value: &Value, path: &str) -> Option<String> {
    match value {
        Value::String(s) => {
            let lower = s.to_ascii_lowercase();
            if lower.contains("sk-") || lower.contains("token") || lower.contains("secret") || lower.contains("ghp_") {
                return Some(format!("{path}: secret-looking string"));
            }
            None
        }
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(idx, item)| detect_secret_like_json(item, &format!("{path}[{idx}]"))),
        Value::Object(map) => map.iter().find_map(|(k, v)| {
            let lower_key = k.to_ascii_lowercase();
            if lower_key.contains("token") || lower_key.contains("secret") || lower_key.contains("api_key") {
                return Some(format!("{path}.{k}: forbidden key"));
            }
            detect_secret_like_json(v, &format!("{path}.{k}"))
        }),
        _ => None,
    }
}

async fn get_contracts(State(state): State<AppState>) -> impl IntoResponse {
    let contracts = state
        .admitted_contracts
        .lock()
        .expect("contract registry lock poisoned");
    let list = contracts
        .values()
        .map(|c| ContractSummary {
            contract_id: c.contract_id.clone(),
            execution_type: c.execution_type.clone(),
            has_source_url: c.source_url.is_some(),
        })
        .collect::<Vec<_>>();
    (StatusCode::OK, Json(json!({"contracts": list}))).into_response()
}

async fn get_contract(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> impl IntoResponse {
    let contracts = state
        .admitted_contracts
        .lock()
        .expect("contract registry lock poisoned");
    match contracts.get(&contract_id) {
        Some(contract) => (StatusCode::OK, Json(contract)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: ApiError {
                    code: "contract_not_found".to_string(),
                    message: "contract was not found".to_string(),
                    details: vec![format!("contract_id: {contract_id}")],
                },
            }),
        )
            .into_response(),
    }
}

fn seed_default_contracts(admitted_contracts: &Arc<Mutex<HashMap<String, AdmittedContract>>>) {
    let mut contracts = admitted_contracts
        .lock()
        .expect("contract registry lock poisoned");
    if contracts.contains_key("daily_owner_report_v0") {
        return;
    }

    let flow = parse_inline_flow_json(&json!({
        "version": "1.0",
        "description": "controlled contract-run corridor fixture",
        "entry": "step_001",
        "steps": [
            {
                "id": "step_001",
                "step_type": "connector_action",
                "operation_id": "op_notify_webhook",
                "execution_mode": "DETERMINISTIC",
                "connector": {
                    "name":"connector.http.generic.v1",
                    "endpoint_ref":"webhook_site_test",
                    "method":"POST",
                    "side_effect_intent":"external_network"
                },
                "next": {"default": {"step":"step_002"}}
            },
            {
                "id": "step_002",
                "step_type": "human_gate",
                "operation_id": "op_send_report",
                "execution_mode": "HUMAN_GATE",
                "next": {
                    "approve": {"step":"step_003"},
                    "reject": {"terminal":"blocked"}
                }
            },
            {
                "id": "step_003",
                "step_type": "terminal",
                "execution_mode": "DETERMINISTIC",
                "next": {"default": {"terminal":"completed"}}
            }
        ]
    }))
    .expect("fixture contract must parse");

    contracts.insert(
        "daily_owner_report_v0".to_string(),
        AdmittedContract {
            contract_id: "daily_owner_report_v0".to_string(),
            execution_type: "agent".to_string(),
            source_type: Some("fixture".to_string()),
            source_url: None,
            commit_sha: None,
            contract_path: None,
            registered_at: Some("2026-05-23T08:30:00Z".to_string()),
            admitted: Some(true),
            manifest: None,
            flow,
            policy: None,
            connector_requirements: None,
            evidence_expectations: None,
            open_questions: None,
        },
    );
}

fn serialize_contract_run_status(state: &ContractRunState) -> CorridorRunStatusResponse {
    CorridorRunStatusResponse {
        run_id: state.run_id.clone(),
        contract_id: state.contract_id.clone(),
        status: state.status,
        current_step_id: state.current_step_id.clone(),
        next_allowed_operation_id: state.next_allowed_operation_id.clone(),
        trace_ref: state.trace_ref.clone(),
        observation_count: state.observation_count,
        gate: state.gate.clone(),
    }
}

fn current_contract_step(state: &ContractRunState) -> Option<crate::contract_step::ContractStep> {
    state
        .steps
        .iter()
        .find(|step| step.step_id == state.current_step_id)
        .cloned()
}

fn admitted_contract_for_run(state: &AppState, run_state: &ContractRunState) -> Option<AdmittedContract> {
    let contracts = state
        .admitted_contracts
        .lock()
        .expect("contract registry lock poisoned");
    contracts.get(&run_state.contract_id).cloned()
}

fn parse_side_effect_intent_str(value: &str) -> Option<SideEffectIntent> {
    match value {
        "none" => Some(SideEffectIntent::None),
        "local_only" => Some(SideEffectIntent::LocalOnly),
        "external_network" => Some(SideEffectIntent::ExternalNetwork),
        "destructive" => Some(SideEffectIntent::Destructive),
        _ => None,
    }
}

fn connector_metadata_for_current_step(
    admitted_contract: &AdmittedContract,
    step_id: &str,
) -> Result<Option<ContractStepConnectorMetadata>, ApiError> {
    let step = flow_step_by_id(&admitted_contract.flow, step_id).ok_or(ApiError {
        code: "contract_step_not_found".to_string(),
        message: "contract step was not found".to_string(),
        details: vec![format!("step_id: {step_id}")],
    })?;
    if step.step_type.as_deref() != Some("connector_action") {
        return Ok(None);
    }
    let connector = step.connector.as_ref().ok_or(ApiError {
        code: "contract_connector_metadata_missing".to_string(),
        message: "connector_action step is missing connector metadata".to_string(),
        details: vec![format!("step_id: {step_id}")],
    })?;
    let connector_id = connector
        .get("name")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or(ApiError {
            code: "contract_connector_metadata_missing".to_string(),
            message: "connector_action step is missing connector name".to_string(),
            details: vec![format!("step_id: {step_id}")],
        })?
        .to_string();
    let side_effect_intent = connector
        .get("side_effect_intent")
        .and_then(|value| value.as_str())
        .and_then(parse_side_effect_intent_str);

    Ok(Some(ContractStepConnectorMetadata {
        connector_id,
        endpoint_ref: connector
            .get("endpoint_ref")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string()),
        method: connector
            .get("method")
            .and_then(|value| value.as_str())
            .map(|value| value.to_ascii_uppercase()),
        side_effect_intent,
    }))
}

fn contract_run_policy_summary(admission: Option<&OperationAdmission>) -> crate::policy::PolicySummary {
    crate::policy::PolicySummary {
        decision: admission
            .map(|value| value.decision)
            .unwrap_or(AdmissionDecision::Allow),
        policy_version: admission
            .map(|value| value.policy_version.clone())
            .unwrap_or_else(|| "policy.default.v0".to_string()),
        reason_code: admission
            .map(|value| value.reason_code.clone())
            .unwrap_or_else(|| "CONTRACT_RUN_COMPLETED".to_string()),
    }
}

async fn persist_contract_run_snapshot(
    state: &AppState,
    run_state: &ContractRunState,
) -> Result<(), StorageError> {
    let observations = {
        let records = state
            .contract_run_observations
            .lock()
            .expect("contract run observation lock poisoned");
        records.get(&run_state.run_id).cloned().unwrap_or_default()
    };
    let evidence = build_evidence_response(
        run_state.run_id.clone(),
        match run_state.status {
            ContractRunStatus::Accepted | ContractRunStatus::InProgress | ContractRunStatus::WaitingReview => {
                RunStatus::InProgress
            }
            ContractRunStatus::Completed => RunStatus::Completed,
            ContractRunStatus::Failed => RunStatus::Failed,
            ContractRunStatus::Blocked => RunStatus::Blocked,
        },
        run_state.trace_ref.clone(),
        &observations,
        contract_run_policy_summary(run_state.last_admission.as_ref()),
    );
    state
        .run_store
        .put_snapshot(RunSnapshot {
            run_id: run_state.run_id.clone(),
            evidence,
            observations,
        })
        .await
}

fn contract_run_scope_admission(
    run_id: &str,
    operation_id: &str,
    auth_context: &AuthContext,
    verifier: &dyn AuthVerifier,
) -> PolicyAdmission {
    PolicyAdmission::from_auth_context(
        format!("adm_{run_id}_{operation_id}"),
        operation_id.to_string(),
        "policy.default.v0".to_string(),
        "contracts.run",
        Some(auth_context.clone()),
        verifier,
    )
}

fn build_current_operation_admission(
    run_state: &ContractRunState,
    admitted_contract: Option<&AdmittedContract>,
    verifier: &dyn AuthVerifier,
) -> OperationAdmission {
    if admitted_contract.is_none() {
        return OperationAdmission {
            admission_id: format!("adm_{}", run_state.run_id),
            decision: AdmissionDecision::Deny,
            reason_code: "CONTRACT_NOT_FOUND".to_string(),
            policy_version: "policy.default.v0".to_string(),
            contract_id: run_state.contract_id.clone(),
            run_id: run_state.run_id.clone(),
            step_id: run_state.current_step_id.clone(),
            operation_id: run_state
                .next_allowed_operation_id
                .clone()
                .unwrap_or_else(|| "op_terminal".to_string()),
            allowed_connector_id: None,
            allowed_endpoint_ref: None,
            allowed_method: None,
            constraints: json!({}),
            expires_at: None,
        };
    }
    let admitted_contract = admitted_contract.expect("checked above");
    let scope_admission = contract_run_scope_admission(
        &run_state.run_id,
        run_state
            .next_allowed_operation_id
            .as_deref()
            .unwrap_or("op_terminal"),
        &run_state.auth_context,
        verifier,
    );

    if matches!(
        run_state.status,
        ContractRunStatus::Blocked | ContractRunStatus::Completed | ContractRunStatus::Failed
    ) {
        return OperationAdmission {
            admission_id: format!("adm_{}", run_state.run_id),
            decision: AdmissionDecision::Deny,
            reason_code: "RUN_ALREADY_TERMINAL".to_string(),
            policy_version: "policy.default.v0".to_string(),
            contract_id: run_state.contract_id.clone(),
            run_id: run_state.run_id.clone(),
            step_id: run_state.current_step_id.clone(),
            operation_id: run_state
                .next_allowed_operation_id
                .clone()
                .unwrap_or_else(|| "op_terminal".to_string()),
            allowed_connector_id: None,
            allowed_endpoint_ref: None,
            allowed_method: None,
            constraints: json!({}),
            expires_at: None,
        };
    }

    let step = current_contract_step(run_state).expect("current step must exist");
    if run_state.status == ContractRunStatus::WaitingReview
        || matches!(
            run_state.gate.as_ref().map(|gate| gate.status),
            Some(HumanGateStatus::WaitingReview)
        )
    {
        return OperationAdmission {
            admission_id: format!("adm_review_{}", run_state.run_id),
            decision: AdmissionDecision::RequireReview,
            reason_code: "HUMAN_GATE_REQUIRED".to_string(),
            policy_version: "policy.default.v0".to_string(),
            contract_id: run_state.contract_id.clone(),
            run_id: run_state.run_id.clone(),
            step_id: run_state.current_step_id.clone(),
            operation_id: step
                .operation_id
                .clone()
                .unwrap_or_else(|| "op_gate".to_string()),
            allowed_connector_id: None,
            allowed_endpoint_ref: None,
            allowed_method: None,
            constraints: json!({}),
            expires_at: None,
        };
    }

    if scope_admission.decision != AdmissionDecision::Allow {
        return OperationAdmission {
            admission_id: scope_admission.admission_id,
            decision: scope_admission.decision,
            reason_code: "OPERATION_NOT_ALLOWED".to_string(),
            policy_version: scope_admission.policy_version,
            contract_id: run_state.contract_id.clone(),
            run_id: run_state.run_id.clone(),
            step_id: run_state.current_step_id.clone(),
            operation_id: step
                .operation_id
                .clone()
                .unwrap_or_else(|| "op_denied".to_string()),
            allowed_connector_id: None,
            allowed_endpoint_ref: None,
            allowed_method: None,
            constraints: json!({"auth_context": scope_admission.auth_context}),
            expires_at: None,
        };
    }

    let connector_metadata =
        connector_metadata_for_current_step(admitted_contract, &run_state.current_step_id).ok().flatten();
    let (connector_id, endpoint_ref, method, reason_code, constraints) =
        if let Some(metadata) = connector_metadata {
            (
                Some(metadata.connector_id),
                metadata.endpoint_ref,
                metadata.method,
                "NEXT_OPERATION_ALLOWED".to_string(),
                json!({
                    "side_effect_intent": metadata.side_effect_intent.map(|value| serde_json::to_value(value).unwrap_or(Value::Null)).unwrap_or(Value::Null)
                }),
            )
        } else if step.step_type == ContractStepType::Terminal {
            return OperationAdmission {
                admission_id: format!("adm_{}", run_state.run_id),
                decision: AdmissionDecision::Deny,
                reason_code: "RUN_ALREADY_TERMINAL".to_string(),
                policy_version: "policy.default.v0".to_string(),
                contract_id: run_state.contract_id.clone(),
                run_id: run_state.run_id.clone(),
                step_id: run_state.current_step_id.clone(),
                operation_id: step
                    .operation_id
                    .clone()
                    .unwrap_or_else(|| "op_terminal".to_string()),
                allowed_connector_id: None,
                allowed_endpoint_ref: None,
                allowed_method: None,
                constraints: json!({}),
                expires_at: None,
            };
        } else {
            (None, None, None, "HUMAN_GATE_APPROVED".to_string(), json!({}))
        };

    OperationAdmission {
        admission_id: format!("adm_{}", run_state.run_id),
        decision: AdmissionDecision::Allow,
        reason_code,
        policy_version: "policy.default.v0".to_string(),
        contract_id: run_state.contract_id.clone(),
        run_id: run_state.run_id.clone(),
        step_id: run_state.current_step_id.clone(),
        operation_id: step
            .operation_id
            .clone()
            .unwrap_or_else(|| "op_terminal".to_string()),
        allowed_connector_id: connector_id,
        allowed_endpoint_ref: endpoint_ref,
        allowed_method: method,
        constraints,
        expires_at: Some("2026-05-23T08:35:00Z".to_string()),
    }
}

fn contains_forbidden_connector_override(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    ["connector_id", "endpoint_ref", "method", "target_url", "side_effect_intent"]
        .iter()
        .any(|key| object.contains_key(*key))
}

fn set_step_status(run_state: &mut ContractRunState, step_id: &str, status: ContractStepStatus) {
    if let Some(step) = run_state.steps.iter_mut().find(|step| step.step_id == step_id) {
        step.status = status;
    }
}

fn step_by_id<'a>(run_state: &'a ContractRunState, step_id: &str) -> Option<&'a crate::contract_step::ContractStep> {
    run_state.steps.iter().find(|step| step.step_id == step_id)
}

fn transition_trace_json(
    from_step_id: &str,
    outcome: ContractStepOutcome,
    transition: &ContractTransition,
    reason_code: &str,
) -> Value {
    match transition {
        ContractTransition::NextStep { step_id } => json!({
            "from_step_id": from_step_id,
            "outcome": outcome.as_flow_key(),
            "transition": {
                "kind": "next_step",
                "target_step_id": step_id,
                "terminal_status": Value::Null
            },
            "reason_code": reason_code
        }),
        ContractTransition::Terminal { status, .. } => json!({
            "from_step_id": from_step_id,
            "outcome": outcome.as_flow_key(),
            "transition": {
                "kind": "terminal",
                "target_step_id": Value::Null,
                "terminal_status": status.as_str()
            },
            "reason_code": reason_code
        }),
        ContractTransition::Blocked { .. } => json!({
            "from_step_id": from_step_id,
            "outcome": outcome.as_flow_key(),
            "transition": {
                "kind": "blocked",
                "target_step_id": Value::Null,
                "terminal_status": "blocked"
            },
            "reason_code": reason_code
        }),
    }
}

fn apply_transition_to_run_state(
    run_state: &mut ContractRunState,
    transition: &ContractTransition,
) -> Result<(), ApiError> {
    match transition {
        ContractTransition::NextStep { step_id } => {
            let next_step = step_by_id(run_state, step_id).cloned().ok_or(ApiError {
                code: "contract_step_not_found".to_string(),
                message: "transition target step was not found".to_string(),
                details: vec![format!("step_id: {step_id}")],
            })?;
            run_state.current_step_id = step_id.clone();
            run_state.current_gate_id = None;
            run_state.gate = None;
            match next_step.step_type {
                ContractStepType::HumanGate => {
                    run_state.status = ContractRunStatus::WaitingReview;
                    run_state.next_allowed_operation_id = next_step.operation_id.clone();
                    set_step_status(run_state, step_id, ContractStepStatus::WaitingReview);
                    let gate = gate_for_step(&run_state.run_id, &next_step);
                    run_state.current_gate_id = Some(gate.gate_id.clone());
                    run_state.gate = Some(gate);
                }
                ContractStepType::ConnectorAction | ContractStepType::DeterministicAction => {
                    run_state.status = ContractRunStatus::InProgress;
                    run_state.next_allowed_operation_id = next_step.operation_id.clone();
                    set_step_status(run_state, step_id, ContractStepStatus::Ready);
                }
                ContractStepType::Terminal => {
                    run_state.status = ContractRunStatus::Completed;
                    run_state.next_allowed_operation_id = None;
                    set_step_status(run_state, step_id, ContractStepStatus::Completed);
                }
            }
        }
        ContractTransition::Terminal { status, .. } => {
            run_state.status = *status;
            run_state.next_allowed_operation_id = None;
            run_state.current_gate_id = None;
            run_state.gate = None;
        }
        ContractTransition::Blocked { .. } => {
            run_state.status = ContractRunStatus::Blocked;
            run_state.next_allowed_operation_id = None;
            run_state.current_gate_id = None;
            run_state.gate = None;
        }
    }
    Ok(())
}

fn append_contract_run_observation(
    state: &AppState,
    run_id: &str,
    record: ObservationRecord,
) -> usize {
    let mut observations = state
        .contract_run_observations
        .lock()
        .expect("contract run observation lock poisoned");
    let records = observations.entry(run_id.to_string()).or_default();
    records.push(record);
    records.len()
}

async fn post_contract_run_start(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let _contract = {
        let contracts = state
            .admitted_contracts
            .lock()
            .expect("contract registry lock poisoned");
        contracts.get(&contract_id).cloned()
    };
    if _contract.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: ApiError {
                    code: "contract_not_found".to_string(),
                    message: "contract was not found".to_string(),
                    details: vec![format!("contract_id: {contract_id}")],
                },
            }),
        )
            .into_response();
    }

    let request: ContractRunRequest = match serde_json::from_value(payload) {
        Ok(value) => value,
        Err(err) => {
            return bad_request(
                "validation_failed",
                "contract run request parsing failed",
                vec![format!("payload: {err}")],
            )
            .into_response()
        }
    };
    if request.request_id.trim().is_empty()
        || request.actor.actor_id.trim().is_empty()
        || request.source.client_id.trim().is_empty()
        || request.auth_context.mode.trim().is_empty()
    {
        return bad_request(
            "validation_failed",
            "contract run request validation failed",
            vec!["request_id, actor, source, and auth_context.mode are required".to_string()],
        )
        .into_response();
    }

    let scope_admission = contract_run_scope_admission(
        &format!("contract_run_{}", request.request_id),
        "op_start_contract",
        &request.auth_context,
        state.auth_verifier.as_ref(),
    );
    if scope_admission.decision != AdmissionDecision::Allow {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: ApiError {
                    code: "authorization_failed".to_string(),
                    message: "policy authorization failed".to_string(),
                    details: vec![format!("reason_code: {}", scope_admission.reason_code)],
                },
            }),
        )
            .into_response();
    }

    let run_id = format!("contract_run_{}", request.request_id);
    let trace_ref = request
        .correlation
        .get("trace_id")
        .and_then(|value| value.as_str())
        .unwrap_or("trace_contract_001")
        .to_string();
    let admitted_contract = _contract.expect("checked above");
    let steps = match contract_steps_from_flow(&admitted_contract) {
        Ok(steps) => steps,
        Err(err) => {
            return bad_request(&err.code, "contract flow is invalid", vec![err.message]).into_response();
        }
    };
    let entry_step = match steps.iter().find(|step| step.step_id == admitted_contract.flow.entry) {
        Some(step) => step.clone(),
        None => {
            return bad_request(
                "contract_flow_invalid",
                "contract flow entry step was not found",
                vec![format!("entry: {}", admitted_contract.flow.entry)],
            )
            .into_response();
        }
    };
    let mut run_state = ContractRunState {
        run_id: run_id.clone(),
        contract_id: contract_id.clone(),
        status: ContractRunStatus::Accepted,
        auth_context: request.auth_context.clone(),
        current_step_id: admitted_contract.flow.entry.clone(),
        next_allowed_operation_id: entry_step.operation_id.clone(),
        completed_step_ids: Vec::new(),
        current_gate_id: None,
        trace_ref: trace_ref.clone(),
        observation_count: 0,
        created_at: "2026-05-23T08:30:00Z".to_string(),
        updated_at: "2026-05-23T08:30:00Z".to_string(),
        steps,
        gate: None,
        last_admission: None,
    };
    let admission = build_current_operation_admission(
        &run_state,
        Some(&admitted_contract),
        state.auth_verifier.as_ref(),
    );
    run_state.last_admission = Some(admission);
    run_state.observation_count = append_contract_run_observation(
        &state,
        &run_id,
        ObservationRecord {
            run_id: run_id.clone(),
            step_id: run_state.current_step_id.clone(),
            event_type: "contract_run.started".to_string(),
            timestamp: "2026-05-23T08:30:00Z".to_string(),
            subject: json!({"contract_id": contract_id, "actor_id": request.actor.actor_id}),
            result: json!({"status": "accepted"}),
            metadata: json!({}),
            evidence_ref: format!("ev_{run_id}_001"),
        },
    );

    {
        let mut runs = state.contract_runs.lock().expect("contract run lock poisoned");
        runs.insert(run_id.clone(), run_state.clone());
    }
    if let Err(err) = persist_contract_run_snapshot(&state, &run_state).await {
        return storage_unavailable(&err).into_response();
    }

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "run_id": run_id,
            "contract_id": contract_id,
            "status": run_state.status.as_str(),
            "current_step_id": run_state.current_step_id,
            "next_allowed_operation_id": run_state.next_allowed_operation_id,
            "trace_ref": trace_ref,
            "observation_count": run_state.observation_count,
            "gate": Value::Null
        })),
    )
        .into_response()
}

async fn get_contract_run_status(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    let runs = state.contract_runs.lock().expect("contract run lock poisoned");
    match runs.get(&run_id) {
        Some(run_state) => (StatusCode::OK, Json(serialize_contract_run_status(run_state))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: ApiError {
                    code: "run_not_found".to_string(),
                    message: "contract run was not found".to_string(),
                    details: vec![format!("run_id: {run_id}")],
                },
            }),
        )
            .into_response(),
    }
}

async fn get_contract_run_next(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    let mut runs = state.contract_runs.lock().expect("contract run lock poisoned");
    let run_state = match runs.get_mut(&run_id) {
        Some(run_state) => run_state,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: ApiError {
                        code: "run_not_found".to_string(),
                        message: "contract run was not found".to_string(),
                        details: vec![format!("run_id: {run_id}")],
                    },
                }),
            )
                .into_response()
        }
    };
    let admitted_contract = admitted_contract_for_run(&state, run_state);
    let admission = build_current_operation_admission(
        run_state,
        admitted_contract.as_ref(),
        state.auth_verifier.as_ref(),
    );
    run_state.last_admission = Some(admission.clone());
    let step = current_contract_step(run_state).expect("current step must exist");
    (
        StatusCode::OK,
        Json(json!({
            "run_id": run_state.run_id,
            "contract_id": run_state.contract_id,
            "step": step,
            "operation_admission": admission
        })),
    )
        .into_response()
}

async fn post_contract_run_step_execute(
    State(state): State<AppState>,
    Path((run_id, step_id)): Path<(String, String)>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let operation_id = payload
        .get("operation_id")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    if operation_id.is_empty() {
        return bad_request(
            "validation_failed",
            "operation_id is required",
            vec!["operation_id".to_string()],
        )
        .into_response();
    }

    let mut run_state = {
        let runs = state.contract_runs.lock().expect("contract run lock poisoned");
        match runs.get(&run_id) {
            Some(run_state) => run_state.clone(),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: ApiError {
                            code: "run_not_found".to_string(),
                            message: "contract run was not found".to_string(),
                            details: vec![format!("run_id: {run_id}")],
                        },
                    }),
                )
                    .into_response()
            }
        }
    };

    if run_state.current_step_id != step_id {
        return (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: ApiError {
                    code: "step_state_conflict".to_string(),
                    message: "step_id is not the current step".to_string(),
                    details: vec![format!("current_step_id: {}", run_state.current_step_id)],
                },
            }),
        )
            .into_response();
    }
    if run_state.status == ContractRunStatus::WaitingReview {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: ApiError {
                    code: "human_gate_required".to_string(),
                    message: "run is waiting for human gate resolution".to_string(),
                    details: vec!["reason_code: HUMAN_GATE_REQUIRED".to_string()],
                },
            }),
        )
            .into_response();
    }
    if matches!(
        run_state.status,
        ContractRunStatus::Completed | ContractRunStatus::Failed | ContractRunStatus::Blocked
    ) {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: ApiError {
                    code: "run_terminal".to_string(),
                    message: "run is already terminal".to_string(),
                    details: vec!["reason_code: RUN_ALREADY_TERMINAL".to_string()],
                },
            }),
        )
            .into_response();
    }
    if run_state.next_allowed_operation_id.as_deref() != Some(operation_id.as_str()) {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: ApiError {
                    code: "operation_not_allowed".to_string(),
                    message: "operation_id is not allowed for current step".to_string(),
                    details: vec![format!("operation_id: {operation_id}")],
                },
            }),
        )
            .into_response();
    }
    if payload.get("connector_id").is_some() || payload.get("endpoint_ref").is_some() || payload.get("method").is_some() {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: ApiError {
                    code: "connector_override_forbidden".to_string(),
                    message: "client-provided connector override is forbidden".to_string(),
                    details: vec![
                        "Client-provided connector override is forbidden.".to_string(),
                    ],
                },
            }),
        )
            .into_response();
    }
    if payload.get("target_url").is_some() || payload.get("side_effect_intent").is_some() {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: ApiError {
                    code: "connector_override_forbidden".to_string(),
                    message: "client-provided connector override is forbidden".to_string(),
                    details: vec!["CONNECTOR_OVERRIDE_FORBIDDEN".to_string()],
                },
            }),
        )
            .into_response();
    }
    if contains_forbidden_connector_override(payload.get("input_payload").unwrap_or(&Value::Null)) {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: ApiError {
                    code: "connector_override_forbidden".to_string(),
                    message: "nested connector override is forbidden".to_string(),
                    details: vec!["CONNECTOR_OVERRIDE_FORBIDDEN".to_string()],
                },
            }),
        )
            .into_response();
    }

    let step = current_contract_step(&run_state).expect("current step must exist");
    let admitted_contract = admitted_contract_for_run(&state, &run_state);
    let admitted_contract_ref = match admitted_contract.as_ref() {
        Some(contract) => contract,
        None => {
            return (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: ApiError {
                        code: "contract_not_found".to_string(),
                        message: "admitted contract for run was not found".to_string(),
                        details: vec![format!("contract_id: {}", run_state.contract_id)],
                    },
                }),
            )
                .into_response()
        }
    };
    let admission = build_current_operation_admission(
        &run_state,
        Some(admitted_contract_ref),
        state.auth_verifier.as_ref(),
    );
    if admission.decision != AdmissionDecision::Allow {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: ApiError {
                    code: "operation_not_allowed".to_string(),
                    message: "operation admission denied".to_string(),
                    details: vec![format!("reason_code: {}", admission.reason_code)],
                },
            }),
        )
            .into_response();
    }

    run_state.last_admission = Some(admission.clone());
    run_state.observation_count = append_contract_run_observation(
        &state,
        &run_state.run_id,
        ObservationRecord {
            run_id: run_state.run_id.clone(),
            step_id: step.step_id.clone(),
            event_type: "contract_run.operation_admitted".to_string(),
            timestamp: "2026-05-23T08:32:30Z".to_string(),
            subject: json!({"operation_id": operation_id}),
            result: json!({
                "operation_id": operation_id,
                "decision": "allow",
                "reason_code": admission.reason_code,
                "allowed_connector_id": admission.allowed_connector_id,
                "allowed_endpoint_ref": admission.allowed_endpoint_ref,
                "allowed_method": admission.allowed_method
            }),
            metadata: json!({}),
            evidence_ref: format!("ev_{}_{}", run_state.run_id, run_state.observation_count + 1),
        },
    );

    let connector_summary = if step.step_type == ContractStepType::ConnectorAction {
        let side_effect_intent = admission
            .constraints
            .get("side_effect_intent")
            .and_then(|value| value.as_str())
            .and_then(parse_side_effect_intent_str)
            .unwrap_or(SideEffectIntent::ExternalNetwork);
        let connector_id = match admission.allowed_connector_id.clone() {
            Some(value) => value,
            None => {
                return (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        error: ApiError {
                            code: "connector_not_allowed".to_string(),
                            message: "connector metadata missing in admission".to_string(),
                            details: vec!["CONNECTOR_NOT_ALLOWED".to_string()],
                        },
                    }),
                )
                    .into_response()
            }
        };
        let method = admission
            .allowed_method
            .clone()
            .unwrap_or_else(|| "POST".to_string());
        let endpoint_ref = admission.allowed_endpoint_ref.clone();
        let auth_context = serde_json::to_value(&run_state.auth_context).unwrap_or_else(|_| json!({}));
        let connector_request = ConnectorExecutionRequest {
            connector_id: connector_id.clone(),
            call_id: format!("call_{}_{}", run_state.run_id, operation_id),
            side_effect_intent,
            request: redact_json(&json!({
                "endpoint_ref": endpoint_ref,
                "method": method,
                "body": payload.get("input_payload").cloned().unwrap_or_else(|| json!({})),
                "run_id": run_state.run_id,
                "correlation_id": payload
                    .get("correlation")
                    .and_then(|value| value.get("correlation_id"))
                    .cloned()
                    .unwrap_or(Value::Null),
                "trace_ref": run_state.trace_ref,
                "input": payload.get("input_payload").cloned().unwrap_or_else(|| json!({}))
            })),
            auth_context: redact_json(&auth_context),
            credential_refs: vec![],
            policy_result: contract_run_policy_summary(Some(&admission)),
            started_at: "2026-05-23T08:33:00Z".to_string(),
        };
        let connector_result = match state.connector_executor.execute(connector_request).await {
            Ok(value) => value,
            Err(err) => return connector_unavailable(&err).into_response(),
        };
        json!({
            "connector_id": connector_id,
            "endpoint_ref": endpoint_ref,
            "method": method,
            "side_effect_intent": side_effect_intent,
            "connector_status": connector_result.call.status,
            "provider": connector_result.call.response.get("provider").cloned().unwrap_or_else(|| json!("unknown")),
            "connector_mode": connector_result.call.response.get("connector_mode").cloned().unwrap_or_else(|| json!("unknown")),
            "response_preview": connector_result.call.response.get("response_preview").cloned().unwrap_or_else(|| json!({})),
            "attempts": connector_result.call.response.get("attempts").cloned().unwrap_or_else(|| json!(1)),
            "timeout_ms": connector_result.call.response.get("timeout_ms").cloned().unwrap_or_else(|| json!(0)),
            "resolved_url": connector_result.call.response.get("resolved_url").cloned().unwrap_or(Value::Null)
        })
    } else {
        json!({
            "provider": "human_gate_resolution",
            "decision_basis": "approved_gate"
        })
    };

    for item in &mut run_state.steps {
        if item.step_id == run_state.current_step_id {
            item.status = ContractStepStatus::Completed;
        }
    }
    let executed_step_id = run_state.current_step_id.clone();
    run_state.completed_step_ids.push(executed_step_id.clone());
    let outcome = if step.step_type == ContractStepType::HumanGate {
        ContractStepOutcome::Approve
    } else {
        ContractStepOutcome::Default
    };
    let transition = match resolve_flow_transition(admitted_contract_ref, &executed_step_id, outcome) {
        Ok(transition) => transition,
        Err(err) => {
            run_state.status = ContractRunStatus::Failed;
            run_state.next_allowed_operation_id = None;
            run_state.current_gate_id = None;
            run_state.gate = None;
            run_state.updated_at = "2026-05-23T08:33:00Z".to_string();
            {
                let mut runs = state.contract_runs.lock().expect("contract run lock poisoned");
                runs.insert(run_id.clone(), run_state.clone());
            }
            let _ = persist_contract_run_snapshot(&state, &run_state).await;
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: ApiError {
                        code: "transition_not_found".to_string(),
                        message: "contract transition could not be resolved".to_string(),
                        details: vec![err.message],
                    },
                }),
            )
                .into_response();
        }
    };
    let transition_reason_code = match &transition {
        ContractTransition::NextStep { .. } => "TRANSITION_APPLIED".to_string(),
        ContractTransition::Terminal { reason_code, .. } => reason_code.clone(),
        ContractTransition::Blocked { reason_code } => reason_code.clone(),
    };
    if let Err(err) = apply_transition_to_run_state(&mut run_state, &transition) {
        return (
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                error: ApiError {
                    code: err.code,
                    message: err.message,
                    details: vec![],
                },
            }),
        )
            .into_response();
    }
    run_state.updated_at = "2026-05-23T08:33:00Z".to_string();
    run_state.last_admission = Some(OperationAdmission {
        admission_id: format!("adm_exec_{}", run_state.run_id),
        decision: AdmissionDecision::Allow,
        reason_code: "OPERATION_ALLOWED".to_string(),
        policy_version: "policy.default.v0".to_string(),
        contract_id: run_state.contract_id.clone(),
        run_id: run_state.run_id.clone(),
        step_id: step.step_id.clone(),
        operation_id: operation_id.clone(),
        allowed_connector_id: admission.allowed_connector_id.clone(),
        allowed_endpoint_ref: admission.allowed_endpoint_ref.clone(),
        allowed_method: admission.allowed_method.clone(),
        constraints: admission.constraints.clone(),
        expires_at: None,
    });
    run_state.observation_count = append_contract_run_observation(
        &state,
        &run_state.run_id,
        ObservationRecord {
            run_id: run_state.run_id.clone(),
            step_id: executed_step_id.clone(),
            event_type: "contract_run.transition_applied".to_string(),
            timestamp: "2026-05-23T08:33:00Z".to_string(),
            subject: json!({"operation_id": operation_id}),
            result: transition_trace_json(
                &executed_step_id,
                outcome,
                &transition,
                &transition_reason_code,
            )
            .as_object()
            .cloned()
            .map(Value::Object)
            .unwrap_or_else(|| json!({})),
            metadata: json!({}),
            evidence_ref: format!("ev_{}_{}", run_state.run_id, run_state.observation_count + 1),
        },
    );
    run_state.observation_count = append_contract_run_observation(
        &state,
        &run_state.run_id,
        ObservationRecord {
            run_id: run_state.run_id.clone(),
            step_id: executed_step_id.clone(),
            event_type: "contract_run.step_executed".to_string(),
            timestamp: "2026-05-23T08:33:00Z".to_string(),
            subject: json!({"operation_id": operation_id}),
            result: json!({
                "status": "step_completed",
                "admission": {
                    "decision": "allow",
                    "reason_code": "OPERATION_ALLOWED",
                    "allowed_connector_id": admission.allowed_connector_id,
                    "allowed_endpoint_ref": admission.allowed_endpoint_ref,
                    "allowed_method": admission.allowed_method
                },
                "connector_summary": connector_summary
            }),
            metadata: json!({}),
            evidence_ref: format!("ev_{}_{}", run_state.run_id, run_state.observation_count + 1),
        },
    );

    {
        let mut runs = state.contract_runs.lock().expect("contract run lock poisoned");
        runs.insert(run_id.clone(), run_state.clone());
    }
    if let Err(err) = persist_contract_run_snapshot(&state, &run_state).await {
        return storage_unavailable(&err).into_response();
    }

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "run_id": run_state.run_id,
            "contract_id": run_state.contract_id,
            "step_id": executed_step_id,
            "operation_id": operation_id,
            "status": "step_completed",
            "next_step_id": run_state.current_step_id,
            "current_step_id": run_state.current_step_id,
            "next_allowed_operation_id": run_state.next_allowed_operation_id,
            "trace_ref": run_state.trace_ref,
            "observation_count": run_state.observation_count,
            "policy_summary": {
                "decision": "allow",
                "policy_version": "policy.default.v0",
                "reason_code": "OPERATION_ALLOWED"
            }
        })),
    )
        .into_response()
}

async fn get_contract_run_current_gate(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    let runs = state.contract_runs.lock().expect("contract run lock poisoned");
    let run_state = match runs.get(&run_id) {
        Some(run_state) => run_state,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: ApiError {
                        code: "run_not_found".to_string(),
                        message: "contract run was not found".to_string(),
                        details: vec![format!("run_id: {run_id}")],
                    },
                }),
            )
                .into_response()
        }
    };
    match &run_state.gate {
        Some(gate) if gate.status == HumanGateStatus::WaitingReview => (StatusCode::OK, Json(json!({
            "run_id": run_id,
            "gate_id": gate.gate_id,
            "step_id": gate.step_id,
            "status": "waiting_review",
            "reason_code": gate.reason_code,
            "prompt": gate.prompt,
            "requested_operation_id": gate.requested_operation_id,
            "created_at": gate.created_at
        }))).into_response(),
        _ => (StatusCode::OK, Json(json!({"run_id": run_id, "gate": Value::Null}))).into_response(),
    }
}

async fn post_contract_run_gate_resolve(
    State(state): State<AppState>,
    Path((run_id, gate_id)): Path<(String, String)>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let request: HumanGateResolutionRequest = match serde_json::from_value(payload) {
        Ok(value) => value,
        Err(err) => {
            return bad_request(
                "validation_failed",
                "gate resolution parsing failed",
                vec![format!("payload: {err}")],
            )
            .into_response()
        }
    };

    let mut run_state = {
        let runs = state.contract_runs.lock().expect("contract run lock poisoned");
        match runs.get(&run_id) {
            Some(run_state) => run_state.clone(),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: ApiError {
                            code: "run_not_found".to_string(),
                            message: "contract run was not found".to_string(),
                            details: vec![format!("run_id: {run_id}")],
                        },
                    }),
                )
                    .into_response()
            }
        }
    };

    let gate = match run_state.gate.clone() {
        Some(gate) if gate.status == HumanGateStatus::WaitingReview && gate.gate_id == gate_id => gate,
        _ => {
            return (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: ApiError {
                        code: "gate_state_conflict".to_string(),
                        message: "only current active gate can be resolved".to_string(),
                        details: vec![format!("gate_id: {gate_id}")],
                    },
                }),
            )
                .into_response()
        }
    };
    let gate_step_id = gate.step_id.clone();
    let admitted_contract = admitted_contract_for_run(&state, &run_state);
    let admitted_contract_ref = match admitted_contract.as_ref() {
        Some(contract) => contract,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: ApiError {
                        code: "contract_not_found".to_string(),
                        message: "admitted contract for run was not found".to_string(),
                        details: vec![format!("contract_id: {}", run_state.contract_id)],
                    },
                }),
            )
                .into_response()
        }
    };

    let resolved_gate = HumanGate {
        status: HumanGateStatus::Resolved,
        resolved_at: Some(
            request
                .timestamps
                .get("decided_at")
                .and_then(|value| value.as_str())
                .unwrap_or("2026-05-23T08:32:00Z")
                .to_string(),
        ),
        decision: Some(request.decision),
        decided_by: Some(request.actor.actor_id.clone()),
        ..gate
    };

    if request.decision == HumanGateDecision::Approve {
        run_state.status = ContractRunStatus::InProgress;
        run_state.next_allowed_operation_id = step_by_id(&run_state, &run_state.current_step_id)
            .and_then(|step| step.operation_id.clone());
        if let Some(step) = run_state
            .steps
            .iter_mut()
            .find(|step| step.step_id == run_state.current_step_id)
        {
            step.status = ContractStepStatus::Ready;
        }
        run_state.last_admission = Some(OperationAdmission {
            admission_id: format!("adm_gate_{}", run_state.run_id),
            decision: AdmissionDecision::Allow,
            reason_code: "HUMAN_GATE_APPROVED".to_string(),
            policy_version: "policy.default.v0".to_string(),
            contract_id: run_state.contract_id.clone(),
            run_id: run_state.run_id.clone(),
            step_id: run_state.current_step_id.clone(),
            operation_id: run_state
                .next_allowed_operation_id
                .clone()
                .unwrap_or_else(|| "op_send_report".to_string()),
            allowed_connector_id: None,
            allowed_endpoint_ref: None,
            allowed_method: None,
            constraints: json!({}),
            expires_at: None,
        });
    } else {
        let transition =
            match resolve_flow_transition(admitted_contract_ref, &run_state.current_step_id, ContractStepOutcome::Reject)
            {
                Ok(transition) => transition,
                Err(err) => {
                    run_state.status = ContractRunStatus::Failed;
                    run_state.next_allowed_operation_id = None;
                    run_state.current_gate_id = None;
                    run_state.gate = None;
                    run_state.updated_at = "2026-05-23T08:32:00Z".to_string();
                    {
                        let mut runs = state.contract_runs.lock().expect("contract run lock poisoned");
                        runs.insert(run_id.clone(), run_state.clone());
                    }
                    let _ = persist_contract_run_snapshot(&state, &run_state).await;
                    return (
                        StatusCode::BAD_GATEWAY,
                        Json(ErrorResponse {
                            error: ApiError {
                                code: "transition_not_found".to_string(),
                                message: "contract transition could not be resolved".to_string(),
                                details: vec![err.message],
                            },
                        }),
                    )
                        .into_response();
                }
            };
        let current_step_id = run_state.current_step_id.clone();
        set_step_status(&mut run_state, &current_step_id, ContractStepStatus::Blocked);
        let _ = apply_transition_to_run_state(&mut run_state, &transition);
        run_state.last_admission = Some(OperationAdmission {
            admission_id: format!("adm_gate_{}", run_state.run_id),
            decision: AdmissionDecision::Deny,
            reason_code: "HUMAN_GATE_REQUIRED".to_string(),
            policy_version: "policy.default.v0".to_string(),
            contract_id: run_state.contract_id.clone(),
            run_id: run_state.run_id.clone(),
            step_id: run_state.current_step_id.clone(),
            operation_id: run_state
                .next_allowed_operation_id
                .clone()
                .unwrap_or_else(|| "op_send_report".to_string()),
            allowed_connector_id: None,
            allowed_endpoint_ref: None,
            allowed_method: None,
            constraints: json!({}),
            expires_at: None,
        });
        run_state.observation_count = append_contract_run_observation(
            &state,
            &run_state.run_id,
            ObservationRecord {
                run_id: run_state.run_id.clone(),
                step_id: gate_step_id.clone(),
                event_type: "contract_run.transition_applied".to_string(),
                timestamp: "2026-05-23T08:32:00Z".to_string(),
                subject: json!({"gate_id": gate_id}),
                result: transition_trace_json(
                    &gate_step_id,
                    ContractStepOutcome::Reject,
                    &transition,
                    "TRANSITION_APPLIED",
                )
                .as_object()
                .cloned()
                .map(Value::Object)
                .unwrap_or_else(|| json!({})),
                metadata: json!({}),
                evidence_ref: format!("ev_{}_{}", run_state.run_id, run_state.observation_count + 1),
            },
        );
    }

    run_state.gate = Some(resolved_gate.clone());
    run_state.updated_at = resolved_gate
        .resolved_at
        .clone()
        .unwrap_or_else(|| "2026-05-23T08:32:00Z".to_string());
    run_state.observation_count = append_contract_run_observation(
        &state,
        &run_state.run_id,
        ObservationRecord {
            run_id: run_state.run_id.clone(),
            step_id: "step_002".to_string(),
            event_type: "contract_run.gate_resolved".to_string(),
            timestamp: run_state.updated_at.clone(),
            subject: json!({"gate_id": gate_id, "actor_id": request.actor.actor_id}),
            result: json!({
                "decision": match request.decision {
                    HumanGateDecision::Approve => "approve",
                    HumanGateDecision::Reject => "reject"
                },
                "reason_code": if request.decision == HumanGateDecision::Approve {
                    "HUMAN_GATE_APPROVED"
                } else {
                    "HUMAN_GATE_REQUIRED"
                }
            }),
            metadata: json!({"reason": request.reason}),
            evidence_ref: format!("ev_{}_{}", run_state.run_id, run_state.observation_count + 1),
        },
    );

    {
        let mut runs = state.contract_runs.lock().expect("contract run lock poisoned");
        runs.insert(run_id.clone(), run_state.clone());
    }
    if let Err(err) = persist_contract_run_snapshot(&state, &run_state).await {
        return storage_unavailable(&err).into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "run_id": run_state.run_id,
            "gate_id": gate_id,
            "status": "resolved",
            "decision": match request.decision {
                HumanGateDecision::Approve => "approve",
                HumanGateDecision::Reject => "reject"
            },
            "next_step_id": run_state.current_step_id,
            "current_step_id": run_state.current_step_id,
            "next_allowed_operation_id": run_state.next_allowed_operation_id,
            "trace_ref": run_state.trace_ref,
            "observation_count": run_state.observation_count
        })),
    )
        .into_response()
}

async fn get_contract_run_evidence(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    let run_state = {
        let runs = state.contract_runs.lock().expect("contract run lock poisoned");
        runs.get(&run_id).cloned()
    };
    let run_state = match run_state {
        Some(run_state) => run_state,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: ApiError {
                        code: "run_not_found".to_string(),
                        message: "contract run was not found".to_string(),
                        details: vec![format!("run_id: {run_id}")],
                    },
                }),
            )
                .into_response()
        }
    };
    let observations = {
        let records = state
            .contract_run_observations
            .lock()
            .expect("contract run observation lock poisoned");
        records.get(&run_id).cloned().unwrap_or_default()
    };
    let steps = observations
        .iter()
        .filter(|record| record.event_type == "contract_run.step_executed")
        .map(|record| {
            json!({
                "step_id": record.step_id,
                "operation_id": record.subject.get("operation_id").cloned().unwrap_or(Value::Null),
                "decision": "allow",
                "reason_code": "OPERATION_ALLOWED",
                "admission": record.result.get("admission").cloned().unwrap_or_else(|| json!({})),
                "connector_summary": record.result.get("connector_summary").cloned().unwrap_or_else(|| json!({}))
            })
        })
        .collect::<Vec<_>>();
    let transitions = observations
        .iter()
        .filter(|record| record.event_type == "contract_run.transition_applied")
        .map(|record| {
            json!({
                "from_step_id": record.result.get("from_step_id").cloned().unwrap_or(Value::Null),
                "outcome": record.result.get("outcome").cloned().unwrap_or(Value::Null),
                "kind": record
                    .result
                    .get("transition")
                    .and_then(|value| value.get("kind"))
                    .cloned()
                    .unwrap_or(Value::Null),
                "target_step_id": record
                    .result
                    .get("transition")
                    .and_then(|value| value.get("target_step_id"))
                    .cloned()
                    .unwrap_or(Value::Null),
                "terminal_status": record
                    .result
                    .get("transition")
                    .and_then(|value| value.get("terminal_status"))
                    .cloned()
                    .unwrap_or(Value::Null),
                "reason_code": record.result.get("reason_code").cloned().unwrap_or(Value::Null)
            })
        })
        .collect::<Vec<_>>();
    let gates = observations
        .iter()
        .filter(|record| record.event_type == "contract_run.gate_resolved")
        .map(|record| {
            json!({
                "gate_id": record.subject.get("gate_id").cloned().unwrap_or(Value::Null),
                "decision": record.result.get("decision").cloned().unwrap_or(Value::Null),
                "decided_by": record.subject.get("actor_id").cloned().unwrap_or(Value::Null),
                "reason_code": record.result.get("reason_code").cloned().unwrap_or(Value::Null)
            })
        })
        .collect::<Vec<_>>();
    let response = build_contract_run_evidence_response(
        run_state.run_id.clone(),
        run_state.contract_id.clone(),
        run_state.status,
        run_state.trace_ref.clone(),
        &observations,
        json!(steps),
        json!(gates),
        json!(transitions),
        contract_run_policy_summary(run_state.last_admission.as_ref()),
    );
    (StatusCode::OK, Json(response)).into_response()
}

async fn post_contract_run(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let admitted = {
        let contracts = state
            .admitted_contracts
            .lock()
            .expect("contract registry lock poisoned");
        contracts.get(&contract_id).cloned()
    };
    let admitted = match admitted {
        Some(c) => c,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: ApiError {
                        code: "contract_not_found".to_string(),
                        message: "contract was not found".to_string(),
                        details: vec![format!("contract_id: {contract_id}")],
                    },
                }),
            )
                .into_response()
        }
    };

    let run_id = payload
        .get("run_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("ctrun_{}", contract_id.replace('.', "_")));
    let trace_ref = payload
        .get("trace_ref")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("trace:{run_id}"));
    let outcomes = payload
        .get("input_payload")
        .map(extract_outcomes_map)
        .unwrap_or_default();

    let mut exec = match execute_contract_flow(&admitted, &admitted.flow.entry, &outcomes, None, true) {
        Ok(v) => v,
        Err(err) => {
            return bad_request(
                &err.code,
                "contract runtime execution failed",
                vec![err.message],
            )
            .into_response()
        }
    };

    if admitted.contract_id == "barbershop.owner_report.daily.v0" {
        let (input_path, rules_path, report_path, audit_path) = barbershop_default_file_paths(&run_id);
        let telegram_live = false;
        match run_barbershop_file_e2e(
            &admitted.contract_id,
            &run_id,
            &input_path,
            &rules_path,
            &report_path,
            &audit_path,
            telegram_live,
        ) {
            Ok(file_result) => {
                let mut marker_set = exec.evidence_markers.clone();
                for m in [
                    "external_data_fetch_result",
                    "deterministic_metrics_result",
                    "anomaly_or_deviation_check_result",
                    "ai_report_draft",
                    "telegram_delivery_result",
                    "final_run_status",
                ] {
                    if !marker_set.iter().any(|x| x == m) {
                        marker_set.push(m.to_string());
                    }
                }
                if file_result.require_human_gate && !marker_set.iter().any(|x| x == "human_gate_decision_if_required") {
                    marker_set.push("human_gate_decision_if_required".to_string());
                }
                exec.evidence_markers = marker_set;
                exec.github_file_result = Some(json!({
                    "mode": "local_repo_files_only",
                    "input_path": input_path,
                    "rules_path": rules_path,
                    "report_path": file_result.report_path,
                    "audit_path": file_result.audit_path,
                    "flags": file_result.flags,
                    "require_human_gate": file_result.require_human_gate,
                    "telegram_delivery_result": file_result.telegram_delivery_result,
                    "metrics": {
                        "total_revenue": file_result.metrics.total_revenue,
                        "visit_count": file_result.metrics.visit_count,
                        "average_check": file_result.metrics.average_check,
                        "cancellations": file_result.metrics.cancellations,
                        "no_shows": file_result.metrics.no_shows
                    }
                }));
                if file_result.require_human_gate {
                    exec.waiting_for_human = true;
                    exec.status = "waiting_human".to_string();
                    if flow_step_by_id(&admitted.flow, "human_gate_escalation").is_some() {
                        exec.current_step_id = "human_gate_escalation".to_string();
                    }
                }
            }
            Err(err) => {
                return bad_request(
                    &err.code,
                    "local file execution failed",
                    vec![err.message],
                )
                .into_response()
            }
        }
    }

    let mut journal = ObservationJournal::new();
    journal.append(ObservationRecord {
        run_id: run_id.clone(),
        step_id: "contract_flow_trace".to_string(),
        event_type: "observation.write".to_string(),
        timestamp: "2026-05-23T10:30:01Z".to_string(),
        subject: json!({"contract_id": admitted.contract_id, "current_step_id": exec.current_step_id}),
        result: json!({
            "status": exec.status,
            "waiting_for_human": exec.waiting_for_human,
            "terminal": exec.terminal,
            "step_trace": exec.step_trace,
            "connector_status": if exec.waiting_for_human { "blocked" } else { "completed" },
            "connector_response": {
                "provider": "contract_admission_bridge.v0",
                "evidence_markers": exec.evidence_markers,
                "telegram_delivery_executed": exec.telegram_executed,
                "github_file_result": exec.github_file_result
            }
        }),
        metadata: json!({}),
        evidence_ref: format!("ev_{run_id}"),
    });

    let evidence = build_evidence_response(
        run_id.clone(),
        if exec.waiting_for_human {
            RunStatus::Blocked
        } else {
            RunStatus::Completed
        },
        trace_ref.clone(),
        journal.records(),
        PolicyAdmission::new(
            "adm_contract_run".to_string(),
            "act_contract_run".to_string(),
            AdmissionDecision::Allow,
            "contract_admission_bridge_allow".to_string(),
            "policy.default.v0".to_string(),
        )
        .to_summary(),
    );
    let snapshot = RunSnapshot {
        run_id: run_id.clone(),
        evidence,
        observations: journal.records().to_vec(),
    };
    if let Err(err) = state.run_store.put_snapshot(snapshot).await {
        return storage_unavailable(&err).into_response();
    }

    if exec.waiting_for_human {
        let mut runs = state
            .legacy_contract_runs
            .lock()
            .expect("contract run lock poisoned");
        runs.insert(
            run_id.clone(),
            LegacyContractRunState {
                run_id: run_id.clone(),
                contract_id: contract_id.clone(),
                current_step_id: exec.current_step_id.clone(),
                status: exec.status.clone(),
                waiting_for_human: true,
                trace_ref: trace_ref.clone(),
                outcomes,
            },
        );
    }

    (
        StatusCode::ACCEPTED,
        Json(ContractRunResponse {
            run_id,
            contract_id,
            status: exec.status,
            current_step_id: exec.current_step_id,
            waiting_for_human: exec.waiting_for_human,
        }),
    )
        .into_response()
}

async fn post_contract_run_decision(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(payload): Json<ContractDecisionRequest>,
) -> impl IntoResponse {
    if payload.decision != "approve" && payload.decision != "reject" {
        return bad_request(
            "contract_decision_invalid",
            "decision must be approve or reject",
            vec![format!("decision: {}", payload.decision)],
        )
        .into_response();
    }
    let run_state = {
        let mut runs = state.legacy_contract_runs.lock().expect("contract run lock poisoned");
        runs.remove(&run_id)
    };
    let run_state = match run_state {
        Some(v) => v,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: ApiError {
                        code: "contract_run_not_found".to_string(),
                        message: "contract run was not found or not waiting_human".to_string(),
                        details: vec![format!("run_id: {run_id}")],
                    },
                }),
            )
                .into_response()
        }
    };

    let admitted = {
        let contracts = state
            .admitted_contracts
            .lock()
            .expect("contract registry lock poisoned");
        contracts.get(&run_state.contract_id).cloned()
    };
    let admitted = match admitted {
        Some(c) => c,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: ApiError {
                        code: "contract_not_found".to_string(),
                        message: "admitted contract for run was not found".to_string(),
                        details: vec![format!("contract_id: {}", run_state.contract_id)],
                    },
                }),
            )
                .into_response()
        }
    };

    let exec = match execute_contract_flow(
        &admitted,
        &run_state.current_step_id,
        &run_state.outcomes,
        Some(payload.decision.as_str()),
        false,
    ) {
        Ok(v) => v,
        Err(err) => {
            return bad_request(
                &err.code,
                "contract continuation failed",
                vec![err.message],
            )
            .into_response()
        }
    };

    let status = exec.status.clone();
    let mut journal = ObservationJournal::new();
    journal.append(ObservationRecord {
        run_id: run_state.run_id.clone(),
        step_id: run_state.current_step_id.clone(),
        event_type: "observation.write".to_string(),
        timestamp: "2026-05-23T10:35:00Z".to_string(),
        subject: json!({"contract_id": run_state.contract_id}),
        result: json!({
            "decision": payload.decision,
            "status": status,
            "terminal": exec.terminal,
            "step_trace": exec.step_trace,
            "connector_status": "completed",
            "connector_response": {
                "provider": "contract_admission_bridge.v0",
                "evidence_markers": exec.evidence_markers,
                "telegram_delivery_executed": exec.telegram_executed
            }
        }),
        metadata: json!({}),
        evidence_ref: format!("ev_{}", run_state.run_id),
    });
    let evidence = build_evidence_response(
        run_state.run_id.clone(),
        RunStatus::Completed,
        run_state.trace_ref.clone(),
        journal.records(),
        PolicyAdmission::new(
            "adm_contract_decision".to_string(),
            "act_contract_decision".to_string(),
            AdmissionDecision::Allow,
            "contract_human_gate_decision".to_string(),
            "policy.default.v0".to_string(),
        )
        .to_summary(),
    );
    let snapshot = RunSnapshot {
        run_id: run_state.run_id.clone(),
        evidence,
        observations: journal.records().to_vec(),
    };
    if let Err(err) = state.run_store.put_snapshot(snapshot).await {
        return storage_unavailable(&err).into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "run_id": run_state.run_id,
            "contract_id": run_state.contract_id,
            "status": status,
            "waiting_for_human": false
        })),
    )
        .into_response()
}

fn is_human_gate_step(step: &ContractFlowStep) -> bool {
    if step.execution_mode == "HUMAN_GATE" {
        return true;
    }
    step.step_type
        .as_ref()
        .map(|s| s == "human_gate_escalation")
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
struct ContractExecutionResult {
    current_step_id: String,
    status: String,
    waiting_for_human: bool,
    terminal: Option<String>,
    step_trace: Vec<String>,
    evidence_markers: Vec<String>,
    telegram_executed: bool,
    github_file_result: Option<Value>,
}

fn marker_for_step(step: &ContractFlowStep) -> Option<&'static str> {
    let step_type = step.step_type.as_deref().unwrap_or("");
    if step_type == "external_resource_call" {
        return Some("external_data_fetch_result");
    }
    if step_type == "ai_composition_step" {
        return Some("ai_report_draft");
    }
    if step_type == "human_gate_escalation" {
        return Some("human_gate_decision_if_required");
    }
    if step_type == "telegram_delivery" {
        return Some("telegram_delivery_result");
    }
    if step_type == "deterministic_calculation" {
        if step.id.contains("anomal") || step.id.contains("deviation") {
            return Some("anomaly_or_deviation_check_result");
        }
        if step.id.contains("metric") || step.id.contains("calculate") {
            return Some("deterministic_metrics_result");
        }
    }
    None
}

fn execute_contract_flow(
    contract: &AdmittedContract,
    start_step_id: &str,
    outcomes: &HashMap<String, String>,
    forced_gate_outcome: Option<&str>,
    pause_on_human_gate: bool,
) -> Result<ContractExecutionResult, ContractRegistryError> {
    let mut current = start_step_id.to_string();
    let mut waiting_for_human = false;
    let mut terminal: Option<String> = None;
    let mut step_trace = Vec::new();
    let mut evidence_markers = Vec::new();
    let mut telegram_executed = false;
    let mut forced_used = false;

    for _ in 0..128 {
        let step = flow_step_by_id(&contract.flow, &current).ok_or_else(|| {
            ContractRegistryError::new(
                "contract_runtime_invalid",
                format!("missing step: {current}"),
            )
        })?;
        step_trace.push(step.id.clone());
        if let Some(marker) = marker_for_step(step) {
            evidence_markers.push(marker.to_string());
        }
        if step.step_type.as_deref() == Some("telegram_delivery") {
            telegram_executed = true;
        }
        if is_human_gate_step(step) && pause_on_human_gate && forced_gate_outcome.is_none() {
            waiting_for_human = true;
            break;
        }
        let outcome = if is_human_gate_step(step) && forced_gate_outcome.is_some() && !forced_used {
            forced_used = true;
            forced_gate_outcome.unwrap_or("default")
        } else {
            outcomes.get(&step.id).map(|s| s.as_str()).unwrap_or("default")
        };
        let target = pick_next_target(step, outcome).ok_or_else(|| {
            ContractRegistryError::new(
                "contract_runtime_invalid",
                format!("step {} has no outcome {}", step.id, outcome),
            )
        })?;
        if let Some(next_step) = target.get("step").and_then(|v| v.as_str()) {
            current = next_step.to_string();
            continue;
        }
        if let Some(t) = target.get("terminal").and_then(|v| v.as_str()) {
            terminal = Some(t.to_string());
            break;
        }
        return Err(ContractRegistryError::new(
            "contract_runtime_invalid",
            format!("step {} target must include step or terminal", step.id),
        ));
    }

    let status = if waiting_for_human {
        "waiting_human".to_string()
    } else if terminal.as_deref() == Some("completed") {
        "completed".to_string()
    } else {
        terminal.clone().unwrap_or_else(|| "completed".to_string())
    };
    evidence_markers.push("final_run_status".to_string());
    Ok(ContractExecutionResult {
        current_step_id: current,
        status,
        waiting_for_human,
        terminal,
        step_trace,
        evidence_markers,
        telegram_executed,
        github_file_result: None,
    })
}

fn barbershop_default_file_paths(run_id: &str) -> (String, String, String, String) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("barbershop");
    let input = base.join("input").join("daily_2026-05-25.json");
    let rules = base.join("config").join("metrics_rules.json");
    let report = base
        .join("output")
        .join("reports")
        .join("owner_report_2026-05-25.md");
    let audit = base
        .join("output")
        .join("audit")
        .join(format!("run_{run_id}.json"));
    (
        input.to_string_lossy().to_string(),
        rules.to_string_lossy().to_string(),
        report.to_string_lossy().to_string(),
        audit.to_string_lossy().to_string(),
    )
}
