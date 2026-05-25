//! HTTP transport adapter skeleton for MOVA Agent API V0.
//!
//! This module is transport-only and delegates to existing module boundaries.

use crate::connectors::{
    create_connector_executor, ConnectorExecutionConfig, ConnectorExecutionError, ConnectorExecutionRequest,
    ConnectorExecutor, SideEffectIntent,
};
use crate::contracts::{
    extract_outcomes_map, flow_step_by_id, parse_contract_connector_requirements, parse_inline_flow_json,
    pick_next_target, validate_admitted_contract, AdmittedContract, ContractFlowStep,
};
use crate::evidence::{build_evidence_response, RunStatus};
use crate::execution::FlatExecutionPlan;
use crate::observation::{ObservationJournal, ObservationRecord};
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
use std::sync::Arc;
use std::sync::Mutex;

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
    contract_runs: Arc<Mutex<HashMap<String, ContractRunState>>>,
}

#[derive(Debug, Clone, Serialize)]
struct ContractRunState {
    run_id: String,
    contract_id: String,
    current_step_id: String,
    status: String,
    waiting_for_human: bool,
    trace_ref: String,
}

#[derive(Debug, Clone, Serialize)]
struct CapabilitiesResponse {
    action_types: Vec<String>,
    policy_decisions: Vec<String>,
    execution_path: Vec<String>,
    runtime_provider: RuntimeProviderCapabilities,
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
    source_url: Option<String>,
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
        .route("/contracts/runs/:run_id/decision", post(post_contract_run_decision))
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
        Self {
            run_store,
            auth_verifier: verifier,
            connector_executor,
            secret_resolver,
            runtime_provider_capabilities: provider.capabilities(),
            admitted_contracts: Arc::new(Mutex::new(HashMap::new())),
            contract_runs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_auth_verifier(auth_verifier: Arc<dyn AuthVerifier>) -> Self {
        let storage = StorageConfig::in_memory_default();
        let run_store: Arc<dyn RunStore> = Arc::from(create_run_store(&storage));
        let connector_config = ConnectorExecutionConfig::deterministic_local_default();
        let connector_executor: Arc<dyn ConnectorExecutor> =
            Arc::from(create_connector_executor(&connector_config));
        let secret_resolver: Arc<dyn SecretResolver> = Arc::new(LocalEnvSecretResolver::default());
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
            admitted_contracts: Arc::new(Mutex::new(HashMap::new())),
            contract_runs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_auth_and_store(auth_verifier: Arc<dyn AuthVerifier>, run_store: Arc<dyn RunStore>) -> Self {
        let connector_config = ConnectorExecutionConfig::deterministic_local_default();
        let connector_executor: Arc<dyn ConnectorExecutor> =
            Arc::from(create_connector_executor(&connector_config));
        let secret_resolver: Arc<dyn SecretResolver> = Arc::new(LocalEnvSecretResolver::default());
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
            admitted_contracts: Arc::new(Mutex::new(HashMap::new())),
            contract_runs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_boundaries(
        auth_verifier: Arc<dyn AuthVerifier>,
        run_store: Arc<dyn RunStore>,
        connector_executor: Arc<dyn ConnectorExecutor>,
    ) -> Self {
        let secret_resolver: Arc<dyn SecretResolver> = Arc::new(LocalEnvSecretResolver::default());
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
            admitted_contracts: Arc::new(Mutex::new(HashMap::new())),
            contract_runs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_runtime_boundaries(
        auth_verifier: Arc<dyn AuthVerifier>,
        run_store: Arc<dyn RunStore>,
        connector_executor: Arc<dyn ConnectorExecutor>,
        secret_resolver: Arc<dyn SecretResolver>,
    ) -> Self {
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
            admitted_contracts: Arc::new(Mutex::new(HashMap::new())),
            contract_runs: Arc::new(Mutex::new(HashMap::new())),
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
    let has_source_url = payload
        .source_url
        .as_ref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let has_inline = payload.inline_flow_json.is_some();
    if has_source_url == has_inline {
        return bad_request(
            "contract_registration_invalid",
            "contract registration payload is invalid",
            vec!["exactly one of source_url or inline_flow_json is required".to_string()],
        )
        .into_response();
    }

    let flow = if let Some(inline) = payload.inline_flow_json.as_ref() {
        match parse_inline_flow_json(inline) {
            Ok(v) => v,
            Err(err) => {
                return bad_request(
                    "contract_flow_parse_failed",
                    "inline flow parse failed",
                    vec![err.message],
                )
                .into_response()
            }
        }
    } else {
        return bad_request(
            "contract_registration_invalid",
            "source_url admission currently requires inline_flow_json bridge mode",
            vec!["source_url is stored as identity only in V0 bridge".to_string()],
        )
        .into_response();
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
        source_url: payload.source_url.clone(),
        manifest: payload.manifest.clone(),
        flow,
        policy: payload.policy.clone(),
        connector_requirements,
        evidence_expectations: payload.evidence_expectations.clone(),
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
            mode: "inline_flow_json".to_string(),
        }),
    )
        .into_response()
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

    let mut current = admitted.flow.entry.clone();
    let mut waiting_for_human = false;
    let mut terminal: Option<String> = None;
    let mut step_trace = Vec::new();

    for _ in 0..128 {
        let step = match flow_step_by_id(&admitted.flow, &current) {
            Some(s) => s,
            None => {
                return bad_request(
                    "contract_runtime_invalid",
                    "flow step resolution failed",
                    vec![format!("missing step: {current}")],
                )
                .into_response()
            }
        };
        step_trace.push(step.id.clone());
        if is_human_gate_step(step) {
            waiting_for_human = true;
            break;
        }
        let outcome = outcomes
            .get(&step.id)
            .map(|s| s.as_str())
            .unwrap_or("default");
        let target = match pick_next_target(step, outcome) {
            Some(v) => v,
            None => {
                return bad_request(
                    "contract_runtime_invalid",
                    "step transition missing",
                    vec![format!("step {} has no outcome {}", step.id, outcome)],
                )
                .into_response()
            }
        };
        if let Some(next_step) = target.get("step").and_then(|v| v.as_str()) {
            current = next_step.to_string();
            continue;
        }
        if let Some(t) = target.get("terminal").and_then(|v| v.as_str()) {
            terminal = Some(t.to_string());
            break;
        }
        return bad_request(
            "contract_runtime_invalid",
            "step transition target invalid",
            vec![format!("step {} target must include step or terminal", step.id)],
        )
        .into_response();
    }

    let status = if waiting_for_human {
        "waiting_human".to_string()
    } else if terminal.as_deref() == Some("completed") {
        "completed".to_string()
    } else {
        terminal.clone().unwrap_or_else(|| "completed".to_string())
    };

    let mut journal = ObservationJournal::new();
    journal.append(ObservationRecord {
        run_id: run_id.clone(),
        step_id: "contract_flow_trace".to_string(),
        event_type: "contract.run".to_string(),
        timestamp: "2026-05-23T10:30:01Z".to_string(),
        subject: json!({"contract_id": admitted.contract_id, "current_step_id": current}),
        result: json!({
            "status": status,
            "waiting_for_human": waiting_for_human,
            "terminal": terminal,
            "step_trace": step_trace
        }),
        metadata: json!({}),
        evidence_ref: format!("ev_{run_id}"),
    });

    let evidence = build_evidence_response(
        run_id.clone(),
        if waiting_for_human {
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

    if waiting_for_human {
        let mut runs = state
            .contract_runs
            .lock()
            .expect("contract run lock poisoned");
        runs.insert(
            run_id.clone(),
            ContractRunState {
                run_id: run_id.clone(),
                contract_id: contract_id.clone(),
                current_step_id: current.clone(),
                status: status.clone(),
                waiting_for_human: true,
                trace_ref: trace_ref.clone(),
            },
        );
    }

    (
        StatusCode::ACCEPTED,
        Json(ContractRunResponse {
            run_id,
            contract_id,
            status,
            current_step_id: current,
            waiting_for_human,
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
        let mut runs = state.contract_runs.lock().expect("contract run lock poisoned");
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

    let status = if payload.decision == "approve" {
        "completed"
    } else {
        "stopped_by_human_gate"
    };
    let mut journal = ObservationJournal::new();
    journal.append(ObservationRecord {
        run_id: run_state.run_id.clone(),
        step_id: run_state.current_step_id.clone(),
        event_type: "human.gate.decision".to_string(),
        timestamp: "2026-05-23T10:35:00Z".to_string(),
        subject: json!({"contract_id": run_state.contract_id}),
        result: json!({"decision": payload.decision, "status": status}),
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
