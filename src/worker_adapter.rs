//! Cloudflare Worker deployment adapter for MOVA Agent API V0.
//!
//! This adapter keeps provider-specific routing in Worker runtime while
//! preserving MOVA core module ownership.

use crate::auth::{create_auth_verifier, AuthVerifier};
use crate::connectors::{
    ConnectorExecutionConfig, ConnectorExecutionError, ConnectorExecutionRequest, ConnectorExecutionResult, ConnectorExecutor,
    ConnectorSecretResolver, DisabledWebhookHttpClient, EndpointEvidencePolicy, EndpointRegistryEntry,
    GenericHttpClient, GenericHttpConnectorExecutor, GenericHttpRequest, ProviderConnectorRegistryEntry,
    SideEffectIntent, WebhookHttpClient, WebhookHttpResult, WebhookSiteConnectorExecutor,
};
use crate::contracts::{
    extract_outcomes_map, flow_step_by_id, parse_contract_connector_requirements, parse_inline_flow_json,
    pick_next_target, validate_admitted_contract, AdmittedContract, ContractFlowStep, ContractRegistryError,
};
use crate::contract_run::{
    contract_steps_from_flow, gate_for_step, resolve_flow_transition, validate_contract_run_flow,
    ContractRunRequest as CorridorContractRunRequest, ContractRunState as CorridorContractRunState,
    ContractRunStatus as CorridorContractRunStatus, ContractStepOutcome, ContractTransition,
};
use crate::contract_step::{ContractStepStatus, ContractStepType};
use crate::evidence::{build_contract_run_evidence_response, build_evidence_response, RunStatus};
use crate::execution::FlatExecutionPlan;
use crate::gate::{HumanGate, HumanGateDecision, HumanGateResolutionRequest, HumanGateStatus};
use crate::observation::{ObservationJournal, ObservationRecord};
use crate::operation_admission::OperationAdmission;
use crate::policy::{AdmissionDecision, PolicyAdmission};
use crate::public_api::{authenticate_api_key, PublicApiAuthError, PublicApiConfig};
use crate::request::{parse_request_envelope, validate_request_envelope};
use crate::secrets::redact_json;
use crate::storage::{CloudflareKvRunStore, RunSnapshot, RunStore};
use crate::worker_surface::{match_worker_route, WorkerRoute};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use worker::wasm_bindgen::JsCast;
use worker::wasm_bindgen::JsValue;
use worker::wasm_bindgen_futures::JsFuture;
use worker::{event, js_sys, web_sys, Context, Env, Request, Response, Result, Router};
use url::Url;

const MAX_REQUEST_BODY_BYTES: usize = 64 * 1024;

struct WorkerState {
    run_store: Arc<dyn RunStore>,
    auth_verifier: Arc<dyn AuthVerifier>,
    connector_executor: Arc<dyn ConnectorExecutor>,
    public_api: PublicApiConfig,
    provider_connector_registry: Vec<ProviderConnectorRegistryEntry>,
    denied_client_ids: Vec<String>,
    kv_store: Option<worker::kv::KvStore>,
    admitted_contracts: Arc<Mutex<HashMap<String, AdmittedContract>>>,
    legacy_contract_runs: Arc<Mutex<HashMap<String, LegacyContractRunState>>>,
    contract_runs: Arc<Mutex<HashMap<String, CorridorContractRunState>>>,
    contract_run_observations: Arc<Mutex<HashMap<String, Vec<ObservationRecord>>>>,
}

fn shared_admitted_contracts() -> Arc<Mutex<HashMap<String, AdmittedContract>>> {
    static CONTRACTS: OnceLock<Arc<Mutex<HashMap<String, AdmittedContract>>>> = OnceLock::new();
    CONTRACTS
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

fn shared_legacy_contract_runs() -> Arc<Mutex<HashMap<String, LegacyContractRunState>>> {
    static RUNS: OnceLock<Arc<Mutex<HashMap<String, LegacyContractRunState>>>> = OnceLock::new();
    RUNS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

fn shared_contract_runs() -> Arc<Mutex<HashMap<String, CorridorContractRunState>>> {
    static RUNS: OnceLock<Arc<Mutex<HashMap<String, CorridorContractRunState>>>> = OnceLock::new();
    RUNS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

fn shared_contract_run_observations() -> Arc<Mutex<HashMap<String, Vec<ObservationRecord>>>> {
    static OBS: OnceLock<Arc<Mutex<HashMap<String, Vec<ObservationRecord>>>>> = OnceLock::new();
    OBS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

const CONTRACT_INDEX_KEY: &str = "contract_admit:index";
const CONTRACT_KEY_PREFIX: &str = "contract_admit:";
const LEGACY_CONTRACT_RUN_KEY_PREFIX: &str = "legacy_contract_run:";
const CONTRACT_RUN_STATE_KEY_PREFIX: &str = "contract_run:";
const CONTRACT_RUN_OBSERVATION_KEY_PREFIX: &str = "contract_run_observations:";

fn contract_kv_key(contract_id: &str) -> String {
    format!("{CONTRACT_KEY_PREFIX}{contract_id}")
}

fn legacy_contract_run_kv_key(run_id: &str) -> String {
    format!("{LEGACY_CONTRACT_RUN_KEY_PREFIX}{run_id}")
}

fn contract_run_state_kv_key(run_id: &str) -> String {
    format!("{CONTRACT_RUN_STATE_KEY_PREFIX}{run_id}:state")
}

fn contract_run_observations_kv_key(run_id: &str) -> String {
    format!("{CONTRACT_RUN_OBSERVATION_KEY_PREFIX}{run_id}:observations")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, serde::Serialize)]
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

#[derive(Debug, Clone, serde::Serialize)]
struct ContractSummary {
    contract_id: String,
    execution_type: String,
    has_source_url: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ContractRunResponse {
    run_id: String,
    contract_id: String,
    status: String,
    current_step_id: String,
    waiting_for_human: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ContractDecisionRequest {
    decision: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct LegacyContractRunState {
    run_id: String,
    contract_id: String,
    current_step_id: String,
    status: String,
    waiting_for_human: bool,
    trace_ref: String,
    outcomes: HashMap<String, String>,
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
}

#[derive(Debug, Clone)]
struct WorkerWebhookHttpClient;

#[derive(Debug, Clone)]
struct WorkerEnvSecretResolver {
    env: Env,
}

impl ConnectorSecretResolver for WorkerEnvSecretResolver {
    fn resolve(&self, secret_ref: &str) -> std::result::Result<Option<String>, ConnectorExecutionError> {
        Ok(self.env.var(secret_ref).ok().map(|value| value.to_string()))
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl WebhookHttpClient for WorkerWebhookHttpClient {
    async fn post_json(
        &self,
        url: &str,
        body: &Value,
    ) -> std::result::Result<WebhookHttpResult, ConnectorExecutionError> {
        let payload = serde_json::to_string(body).map_err(|_| {
            ConnectorExecutionError::new(
                "connector_request_invalid",
                "failed to serialize webhook payload",
            )
        })?;
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
        init.set_body(&JsValue::from_str(&payload));
        let headers = web_sys::Headers::new().map_err(|_| {
            ConnectorExecutionError::new("connector_request_invalid", "failed to create headers")
        })?;
        headers
            .set("content-type", "application/json")
            .map_err(|_| {
                ConnectorExecutionError::new(
                    "connector_request_invalid",
                    "failed to set webhook content-type",
                )
            })?;
        init.set_headers(&headers);

        let global: web_sys::WorkerGlobalScope = js_sys::global().unchecked_into();
        let promise = global
            .fetch_with_str_and_init(url, &init);
        let response_js = JsFuture::from(promise).await.map_err(|_| {
            ConnectorExecutionError::new("connector_provider_network_failed", "webhook fetch failed")
        })?;
        let response: web_sys::Response = response_js.dyn_into().map_err(|_| {
            ConnectorExecutionError::new("connector_provider_network_failed", "webhook response cast failed")
        })?;
        let status = response.status();
        let text_js = JsFuture::from(response.text().map_err(|_| {
            ConnectorExecutionError::new("connector_provider_network_failed", "webhook response text failed")
        })?)
        .await
        .map_err(|_| {
            ConnectorExecutionError::new("connector_provider_network_failed", "webhook response text await failed")
        })?;
        let preview: String = text_js.as_string().unwrap_or_default().chars().take(256).collect();
        Ok(WebhookHttpResult {
            status,
            body_preview: preview,
        })
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl GenericHttpClient for WorkerWebhookHttpClient {
    async fn execute(
        &self,
        request: GenericHttpRequest,
    ) -> std::result::Result<WebhookHttpResult, ConnectorExecutionError> {
        let init = web_sys::RequestInit::new();
        init.set_method(&request.method);
        let headers = web_sys::Headers::new().map_err(|_| {
            ConnectorExecutionError::new("connector_request_invalid", "failed to create headers")
        })?;
        for (k, v) in request.headers {
            headers.set(&k, &v).map_err(|_| {
                ConnectorExecutionError::new("connector_request_invalid", "invalid request header")
            })?;
        }
        if request.method == "POST" {
            let payload = serde_json::to_string(&request.body.unwrap_or_else(|| json!({}))).map_err(|_| {
                ConnectorExecutionError::new("connector_request_invalid", "failed to serialize HTTP payload")
            })?;
            init.set_body(&JsValue::from_str(&payload));
            headers.set("content-type", "application/json").map_err(|_| {
                ConnectorExecutionError::new("connector_request_invalid", "failed to set content-type")
            })?;
        }
        init.set_headers(&headers);

        let global: web_sys::WorkerGlobalScope = js_sys::global().unchecked_into();
        let promise = global.fetch_with_str_and_init(&request.url, &init);
        let response_js = JsFuture::from(promise).await.map_err(|_| {
            ConnectorExecutionError::new("connector_provider_network_failed", "http_generic fetch failed")
        })?;
        let response: web_sys::Response = response_js.dyn_into().map_err(|_| {
            ConnectorExecutionError::new("connector_provider_network_failed", "http_generic response cast failed")
        })?;
        let status = response.status();
        let text_js = JsFuture::from(response.text().map_err(|_| {
            ConnectorExecutionError::new("connector_provider_network_failed", "http_generic response text failed")
        })?)
        .await
        .map_err(|_| {
            ConnectorExecutionError::new("connector_provider_network_failed", "http_generic response text await failed")
        })?;
        let preview: String = text_js.as_string().unwrap_or_default().chars().take(256).collect();
        Ok(WebhookHttpResult {
            status,
            body_preview: preview,
        })
    }
}

fn webhook_config_from_env(env: &Env) -> Option<ConnectorExecutionConfig> {
    let allowed_url = env.var("MOVA_WEBHOOK_SITE_ALLOWED_URL").ok()?.to_string();
    Some(ConnectorExecutionConfig {
        adapter_kind: "webhook_site".to_string(),
        allowed_connectors: vec!["connector.webhook_site.v1".to_string()],
        allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
        offline_stub_rules: Vec::new(),
        allowed_webhook_urls: vec![allowed_url],
        endpoint_registry: Vec::new(),
        provider_connector_registry: Vec::new(),
        timeout_ms: 10_000,
        max_retries: 0,
    })
}

fn provider_connector_registry_from_env(env: &Env) -> Vec<ProviderConnectorRegistryEntry> {
    env.var("MOVA_PROVIDER_CONNECTOR_REGISTRY_JSON")
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<ProviderConnectorRegistryEntry>>(&raw.to_string()).ok())
        .unwrap_or_default()
}

fn public_api_config_from_env(env: &Env) -> PublicApiConfig {
    let mut cfg = PublicApiConfig::deterministic_local_default();
    if let Ok(value) = env.var("MOVA_API_KEY") {
        cfg.api_key = value.to_string();
    }
    if let Ok(value) = env.var("MOVA_SERVER_TENANT_ID") {
        cfg.tenant_id = value.to_string();
    }
    if let Ok(value) = env.var("MOVA_PUBLIC_ACTOR_ID") {
        cfg.actor_id = value.to_string();
    }
    if let Ok(value) = env.var("MOVA_PUBLIC_CLIENT_ID") {
        cfg.client_id = value.to_string();
    }
    cfg
}

fn http_generic_config_from_env(env: &Env) -> Option<ConnectorExecutionConfig> {
    let provider_connector_registry = provider_connector_registry_from_env(env);
    if let Ok(raw) = env.var("MOVA_HTTP_ENDPOINT_REGISTRY_JSON") {
        let parsed: Vec<EndpointRegistryEntry> = serde_json::from_str(&raw.to_string()).ok()?;
        if !parsed.is_empty() {
            return Some(ConnectorExecutionConfig {
                adapter_kind: "http_generic".to_string(),
                allowed_connectors: vec![
                    "connector.http.generic.v1".to_string(),
                    "provider.connector.v1".to_string(),
                ],
                allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
                offline_stub_rules: Vec::new(),
                allowed_webhook_urls: Vec::new(),
                endpoint_registry: parsed,
                provider_connector_registry,
                timeout_ms: 10_000,
                max_retries: 0,
            });
        }
    }
    let endpoint_ref = env.var("MOVA_HTTP_ENDPOINT_REF").ok()?.to_string();
    let endpoint_url = env.var("MOVA_HTTP_ENDPOINT_URL").ok()?.to_string();
    let methods = env
        .var("MOVA_HTTP_ENDPOINT_ALLOWED_METHODS")
        .ok()
        .map(|v| {
            v.to_string()
                .split(',')
                .map(|s| s.trim().to_ascii_uppercase())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec!["POST".to_string()]);
    Some(ConnectorExecutionConfig {
        adapter_kind: "http_generic".to_string(),
        allowed_connectors: vec![
            "connector.http.generic.v1".to_string(),
            "provider.connector.v1".to_string(),
        ],
        allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
        offline_stub_rules: Vec::new(),
        allowed_webhook_urls: Vec::new(),
        endpoint_registry: vec![EndpointRegistryEntry {
            endpoint_ref,
            url: endpoint_url,
            allowed_methods: methods,
            allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
            required_scopes: vec!["actions.run".to_string()],
            timeout_ms: 10_000,
            max_retries: 0,
            evidence_policy: EndpointEvidencePolicy::SummaryOnly,
            enabled: true,
        }],
        provider_connector_registry,
        timeout_ms: 10_000,
        max_retries: 0,
    })
}

fn state_from_env(env: &Env) -> WorkerState {
    let kv_ttl_seconds = env
        .var("MOVA_RUN_TTL_SECONDS")
        .ok()
        .and_then(|v| v.to_string().parse::<u64>().ok())
        .filter(|v| *v > 0);
    let kv_store = env.kv("MOVA_RUN_STORE").ok();
    let run_store: Arc<dyn RunStore> = match kv_store.clone() {
        Some(kv) => Arc::new(CloudflareKvRunStore::new_with_ttl(kv, kv_ttl_seconds)),
        None => Arc::new(crate::storage::InMemoryRunStore::new()),
    };
    let denied_client_ids = env
        .var("MOVA_RATE_LIMIT_DENY_CLIENTS")
        .ok()
        .map(|v| {
            v.to_string()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let connector_executor: Arc<dyn ConnectorExecutor> = if let Some(cfg) = http_generic_config_from_env(env) {
        let provider_connector_registry = cfg.provider_connector_registry.clone();
        if cfg.validate().is_ok() {
            let executor = Arc::new(GenericHttpConnectorExecutor::with_secret_resolver(
                cfg,
                Arc::new(WorkerWebhookHttpClient),
                Arc::new(WorkerEnvSecretResolver { env: env.clone() }),
            ));
            let admitted_contracts = shared_admitted_contracts();
            seed_default_contracts(&admitted_contracts);
            return WorkerState {
                run_store,
                auth_verifier: Arc::from(create_auth_verifier(&crate::auth::AuthTrustConfig::default_v0())),
                connector_executor: executor,
                public_api: public_api_config_from_env(env),
                provider_connector_registry,
                denied_client_ids,
                kv_store,
                admitted_contracts,
                legacy_contract_runs: shared_legacy_contract_runs(),
                contract_runs: shared_contract_runs(),
                contract_run_observations: shared_contract_run_observations(),
            };
        } else {
            let executor = Arc::new(GenericHttpConnectorExecutor::with_secret_resolver(
                ConnectorExecutionConfig {
                    adapter_kind: "http_generic".to_string(),
                    allowed_connectors: vec![
                        "connector.http.generic.v1".to_string(),
                        "provider.connector.v1".to_string(),
                    ],
                    allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
                    offline_stub_rules: Vec::new(),
                    allowed_webhook_urls: Vec::new(),
                    endpoint_registry: vec![EndpointRegistryEntry {
                        endpoint_ref: "invalid".to_string(),
                        url: "https://webhook.site/invalid".to_string(),
                        allowed_methods: vec!["POST".to_string()],
                        allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
                        required_scopes: vec!["actions.run".to_string()],
                        timeout_ms: 10_000,
                        max_retries: 0,
                        evidence_policy: EndpointEvidencePolicy::SummaryOnly,
                        enabled: true,
                    }],
                    provider_connector_registry: provider_connector_registry_from_env(env),
                    timeout_ms: 10_000,
                    max_retries: 0,
                },
                Arc::new(DisabledWebhookHttpClient),
                Arc::new(WorkerEnvSecretResolver { env: env.clone() }),
            ));
            let admitted_contracts = shared_admitted_contracts();
            seed_default_contracts(&admitted_contracts);
            return WorkerState {
                run_store,
                auth_verifier: Arc::from(create_auth_verifier(&crate::auth::AuthTrustConfig::default_v0())),
                connector_executor: executor,
                public_api: public_api_config_from_env(env),
                provider_connector_registry,
                denied_client_ids,
                kv_store,
                admitted_contracts,
                legacy_contract_runs: shared_legacy_contract_runs(),
                contract_runs: shared_contract_runs(),
                contract_run_observations: shared_contract_run_observations(),
            };
        }
    } else if let Some(cfg) = webhook_config_from_env(env) {
        if cfg.validate().is_ok() {
            Arc::new(WebhookSiteConnectorExecutor::new(
                cfg,
                Arc::new(WorkerWebhookHttpClient),
            ))
        } else {
            Arc::new(WebhookSiteConnectorExecutor::new(
                ConnectorExecutionConfig {
                    adapter_kind: "webhook_site".to_string(),
                    allowed_connectors: vec!["connector.webhook_site.v1".to_string()],
                    allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
                    offline_stub_rules: Vec::new(),
                    allowed_webhook_urls: vec!["https://webhook.site/invalid".to_string()],
                    endpoint_registry: Vec::new(),
                    provider_connector_registry: Vec::new(),
                    timeout_ms: 10_000,
                    max_retries: 0,
                },
                Arc::new(DisabledWebhookHttpClient),
            ))
        }
    } else {
        Arc::from(crate::connectors::create_connector_executor(
            &ConnectorExecutionConfig::deterministic_local_default(),
        ))
    };

    let admitted_contracts = shared_admitted_contracts();
    seed_default_contracts(&admitted_contracts);

    WorkerState {
        run_store,
        auth_verifier: Arc::from(create_auth_verifier(&crate::auth::AuthTrustConfig::default_v0())),
        connector_executor,
        public_api: public_api_config_from_env(env),
        provider_connector_registry: provider_connector_registry_from_env(env),
        denied_client_ids,
        kv_store,
        admitted_contracts,
        legacy_contract_runs: shared_legacy_contract_runs(),
        contract_runs: shared_contract_runs(),
        contract_run_observations: shared_contract_run_observations(),
    }
}

fn seed_default_contracts(admitted_contracts: &Arc<Mutex<HashMap<String, AdmittedContract>>>) {
    let mut contracts = admitted_contracts
        .lock()
        .expect("contract registry lock poisoned");
    if !contracts.contains_key("daily_owner_report_v0") {
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
                    "endpoint_ref":"webhook_site_contract_run_test",
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

    if !contracts.contains_key("provider_connector_owner_report_v0") {
        let provider_flow = parse_inline_flow_json(&json!({
            "version": "1.0",
            "description": "provider connector proxy demo contract",
            "entry": "send_owner_report",
            "steps": [
                {
                    "id": "send_owner_report",
                    "step_type": "connector_action",
                    "operation_id": "op_send_owner_report",
                    "execution_mode": "DETERMINISTIC",
                    "connector": {
                        "name":"provider.connector.v1",
                        "connector_ref":"telegram.owner_report_channel",
                        "side_effect_intent":"external_network",
                        "operation":"send_message"
                    },
                    "next": {"default": {"terminal":"completed"}}
                }
            ]
        }))
        .expect("provider connector fixture contract must parse");

        contracts.insert(
            "provider_connector_owner_report_v0".to_string(),
            AdmittedContract {
                contract_id: "provider_connector_owner_report_v0".to_string(),
                execution_type: "agent".to_string(),
                source_type: Some("fixture".to_string()),
                source_url: None,
                commit_sha: None,
                contract_path: None,
                registered_at: Some("2026-05-23T08:30:00Z".to_string()),
                admitted: Some(true),
                manifest: None,
                flow: provider_flow,
                policy: None,
                connector_requirements: None,
                evidence_expectations: None,
                open_questions: None,
            },
        );
    }
}

fn parse_side_effect_intent(connector_context: &Value) -> SideEffectIntent {
    match connector_context
        .get("side_effect_intent")
        .and_then(|v| v.as_str())
        .unwrap_or("none")
    {
        "external_network" => SideEffectIntent::ExternalNetwork,
        "local_only" => SideEffectIntent::LocalOnly,
        "destructive" => SideEffectIntent::Destructive,
        _ => SideEffectIntent::None,
    }
}

fn sanitize_idempotency_key(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(96)
        .collect()
}

fn json_error(status: u16, code: &str, message: &str, details: Vec<String>) -> Result<Response> {
    Response::from_json(&json!({
        "error": {
            "code": code,
            "message": message,
            "details": details
        }
    }))
    .map(|r| r.with_status(status))
}

fn worker_public_auth_error(error: PublicApiAuthError) -> Result<Response> {
    match error {
        PublicApiAuthError::MissingApiKey => json_error(
            401,
            "authentication_required",
            "missing x-mova-api-key",
            vec!["header: x-mova-api-key".to_string()],
        ),
        PublicApiAuthError::InvalidApiKey => json_error(
            403,
            "authentication_failed",
            "invalid x-mova-api-key",
            vec!["header: x-mova-api-key".to_string()],
        ),
    }
}

fn authorize_worker_public_contract_route(
    req: &Request,
    state: &WorkerState,
) -> std::result::Result<crate::request::AuthContext, Result<Response>> {
    let api_key = req
        .headers()
        .get("x-mova-api-key")
        .ok()
        .flatten()
        .map(|value| value.to_string());
    authenticate_api_key(api_key.as_deref(), &state.public_api).map_err(worker_public_auth_error)?;
    Ok(state.public_api.auth_context())
}

fn parse_json_with_limit(bytes: &[u8]) -> std::result::Result<Value, String> {
    if bytes.len() > MAX_REQUEST_BODY_BYTES {
        return Err(format!("body too large: {} bytes", bytes.len()));
    }
    serde_json::from_slice::<Value>(bytes).map_err(|err| format!("invalid json: {err}"))
}

fn contract_error(status: u16, code: &str, message: &str, details: Vec<String>) -> Result<Response> {
    Response::from_json(&json!({
        "error": {
            "code": code,
            "message": message,
            "details": details
        }
    }))
    .map(|r| r.with_status(status))
}

async fn resolve_contract_register_source(
    payload: &RegisterContractRequest,
) -> std::result::Result<ResolvedRegisterSource, (String, String, Vec<String>)> {
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
            vec![source_url],
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
) -> std::result::Result<FetchedGithubPackage, (String, String, Vec<String>)> {
    let (owner, repo) = parse_github_owner_repo(source_url)?;
    let base = format!(
        "https://raw.githubusercontent.com/{owner}/{repo}/{commit_sha}/{}",
        contract_path.trim_matches('/')
    );
    let manifest = fetch_json_from_url_worker(&format!("{base}/manifest.json")).await?;
    let flow = fetch_json_from_url_worker(&format!("{base}/flow.json")).await?;
    let policy = fetch_json_from_url_worker(&format!("{base}/policy.json")).await?;
    let bindings_example = fetch_json_from_url_worker(&format!("{base}/bindings.example.json")).await?;
    let evidence_requirements =
        fetch_json_from_url_worker(&format!("{base}/evidence_requirements.json")).await?;
    Ok(FetchedGithubPackage {
        manifest,
        flow,
        policy,
        _bindings_example: bindings_example,
        evidence_requirements,
    })
}

async fn fetch_json_from_url_worker(url: &str) -> std::result::Result<Value, (String, String, Vec<String>)> {
    let init = web_sys::RequestInit::new();
    init.set_method("GET");
    let global: web_sys::WorkerGlobalScope = js_sys::global().unchecked_into();
    let promise = global.fetch_with_str_and_init(url, &init);
    let response_js = JsFuture::from(promise).await.map_err(|_| {
        (
            "contract_source_fetch_failed".to_string(),
            "failed to fetch contract source".to_string(),
            vec![url.to_string()],
        )
    })?;
    let response: web_sys::Response = response_js.dyn_into().map_err(|_| {
        (
            "contract_source_fetch_failed".to_string(),
            "failed to read response".to_string(),
            vec![url.to_string()],
        )
    })?;
    if !response.ok() {
        return Err((
            "contract_source_fetch_failed".to_string(),
            "failed to fetch contract source".to_string(),
            vec![format!("{url}: status {}", response.status())],
        ));
    }
    let text_js = JsFuture::from(response.text().map_err(|_| {
        (
            "contract_source_fetch_failed".to_string(),
            "failed to read response text".to_string(),
            vec![url.to_string()],
        )
    })?)
    .await
    .map_err(|_| {
        (
            "contract_source_fetch_failed".to_string(),
            "failed to read response text".to_string(),
            vec![url.to_string()],
        )
    })?;
    let text = text_js.as_string().unwrap_or_default();
    serde_json::from_str::<Value>(&text).map_err(|err| {
        (
            "contract_source_parse_failed".to_string(),
            "fetched contract file is not valid json".to_string(),
            vec![format!("{url}: {err}")],
        )
    })
}

fn parse_github_owner_repo(source_url: &str) -> std::result::Result<(String, String), (String, String, Vec<String>)> {
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
    if host == "github.com" || host == "raw.githubusercontent.com" {
        if segments.len() < 2 {
            return Err((
                "contract_registration_invalid".to_string(),
                "source_url must include owner/repo".to_string(),
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

fn is_human_gate_step(step: &ContractFlowStep) -> bool {
    if step.execution_mode == "HUMAN_GATE" {
        return true;
    }
    step.step_type
        .as_ref()
        .map(|s| s == "human_gate_escalation")
        .unwrap_or(false)
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
) -> std::result::Result<ContractExecutionResult, ContractRegistryError> {
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
    })
}

async fn kv_upsert_contract(kv: &worker::kv::KvStore, contract: &AdmittedContract) -> std::result::Result<(), String> {
    let key = contract_kv_key(&contract.contract_id);
    let payload = serde_json::to_string(contract).map_err(|e| format!("serialize contract failed: {e}"))?;
    kv.put(&key, payload)
        .map_err(|e| format!("kv put build failed: {e}"))?
        .execute()
        .await
        .map_err(|e| format!("kv put execute failed: {e}"))?;

    let mut ids = kv
        .get(CONTRACT_INDEX_KEY)
        .json::<Vec<String>>()
        .await
        .map_err(|e| format!("kv get index failed: {e}"))?
        .unwrap_or_default();
    if !ids.iter().any(|v| v == &contract.contract_id) {
        ids.push(contract.contract_id.clone());
    }
    let index_payload = serde_json::to_string(&ids).map_err(|e| format!("serialize contract index failed: {e}"))?;
    kv.put(CONTRACT_INDEX_KEY, index_payload)
        .map_err(|e| format!("kv put index build failed: {e}"))?
        .execute()
        .await
        .map_err(|e| format!("kv put index execute failed: {e}"))?;
    Ok(())
}

async fn kv_get_contract(kv: &worker::kv::KvStore, contract_id: &str) -> std::result::Result<Option<AdmittedContract>, String> {
    kv.get(&contract_kv_key(contract_id))
        .json::<AdmittedContract>()
        .await
        .map_err(|e| format!("kv get contract failed: {e}"))
}

async fn kv_list_contract_ids(kv: &worker::kv::KvStore) -> std::result::Result<Vec<String>, String> {
    Ok(kv
        .get(CONTRACT_INDEX_KEY)
        .json::<Vec<String>>()
        .await
        .map_err(|e| format!("kv get index failed: {e}"))?
        .unwrap_or_default())
}

async fn kv_put_legacy_contract_run_state(
    kv: &worker::kv::KvStore,
    run_state: &LegacyContractRunState,
) -> std::result::Result<(), String> {
    let payload = serde_json::to_string(run_state).map_err(|e| format!("serialize run state failed: {e}"))?;
    kv.put(&legacy_contract_run_kv_key(&run_state.run_id), payload)
        .map_err(|e| format!("kv put run state build failed: {e}"))?
        .execute()
        .await
        .map_err(|e| format!("kv put run state execute failed: {e}"))?;
    Ok(())
}

async fn kv_get_legacy_contract_run_state(
    kv: &worker::kv::KvStore,
    run_id: &str,
) -> std::result::Result<Option<LegacyContractRunState>, String> {
    kv.get(&legacy_contract_run_kv_key(run_id))
        .json::<LegacyContractRunState>()
        .await
        .map_err(|e| format!("kv get run state failed: {e}"))
}

async fn kv_delete_legacy_contract_run_state(
    kv: &worker::kv::KvStore,
    run_id: &str,
) -> std::result::Result<(), String> {
    kv.delete(&legacy_contract_run_kv_key(run_id))
        .await
        .map_err(|e| format!("kv delete run state failed: {e}"))
}

async fn kv_put_corridor_contract_run_state(
    kv: &worker::kv::KvStore,
    run_state: &CorridorContractRunState,
) -> std::result::Result<(), String> {
    let payload = serde_json::to_string(run_state).map_err(|e| format!("serialize run state failed: {e}"))?;
    kv.put(&contract_run_state_kv_key(&run_state.run_id), payload)
        .map_err(|e| format!("kv put run state build failed: {e}"))?
        .execute()
        .await
        .map_err(|e| format!("kv put run state execute failed: {e}"))?;
    Ok(())
}

async fn kv_get_corridor_contract_run_state(
    kv: &worker::kv::KvStore,
    run_id: &str,
) -> std::result::Result<Option<CorridorContractRunState>, String> {
    kv.get(&contract_run_state_kv_key(run_id))
        .json::<CorridorContractRunState>()
        .await
        .map_err(|e| format!("kv get run state failed: {e}"))
}

async fn kv_put_corridor_contract_run_observations(
    kv: &worker::kv::KvStore,
    run_id: &str,
    observations: &[ObservationRecord],
) -> std::result::Result<(), String> {
    let payload =
        serde_json::to_string(observations).map_err(|e| format!("serialize run observations failed: {e}"))?;
    kv.put(&contract_run_observations_kv_key(run_id), payload)
        .map_err(|e| format!("kv put run observations build failed: {e}"))?
        .execute()
        .await
        .map_err(|e| format!("kv put run observations execute failed: {e}"))?;
    Ok(())
}

async fn kv_get_corridor_contract_run_observations(
    kv: &worker::kv::KvStore,
    run_id: &str,
) -> std::result::Result<Vec<ObservationRecord>, String> {
    Ok(kv
        .get(&contract_run_observations_kv_key(run_id))
        .json::<Vec<ObservationRecord>>()
        .await
        .map_err(|e| format!("kv get run observations failed: {e}"))?
        .unwrap_or_default())
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

#[derive(Debug, Clone)]
struct ContractStepConnectorMetadata {
    connector_id: String,
    endpoint_ref: Option<String>,
    connector_ref: Option<String>,
    provider: Option<String>,
    operation: Option<String>,
    method: Option<String>,
    side_effect_intent: Option<SideEffectIntent>,
}

fn current_contract_step(state: &CorridorContractRunState) -> Option<crate::contract_step::ContractStep> {
    state
        .steps
        .iter()
        .find(|step| step.step_id == state.current_step_id)
        .cloned()
}

async fn worker_load_contract(
    state: &WorkerState,
    contract_id: &str,
) -> std::result::Result<Option<AdmittedContract>, String> {
    if let Some(contract) = state
        .admitted_contracts
        .lock()
        .expect("contract registry lock poisoned")
        .get(contract_id)
        .cloned()
    {
        return Ok(Some(contract));
    }
    if let Some(kv) = &state.kv_store {
        if let Some(contract) = kv_get_contract(kv, contract_id).await? {
            state
                .admitted_contracts
                .lock()
                .expect("contract registry lock poisoned")
                .insert(contract_id.to_string(), contract.clone());
            return Ok(Some(contract));
        }
    }
    Ok(None)
}

async fn load_corridor_run_state(
    state: &WorkerState,
    run_id: &str,
) -> std::result::Result<Option<CorridorContractRunState>, String> {
    if let Some(run_state) = state
        .contract_runs
        .lock()
        .expect("contract run lock poisoned")
        .get(run_id)
        .cloned()
    {
        return Ok(Some(run_state));
    }
    if let Some(kv) = &state.kv_store {
        if let Some(run_state) = kv_get_corridor_contract_run_state(kv, run_id).await? {
            state
                .contract_runs
                .lock()
                .expect("contract run lock poisoned")
                .insert(run_id.to_string(), run_state.clone());
            return Ok(Some(run_state));
        }
    }
    Ok(None)
}

async fn load_corridor_observations(
    state: &WorkerState,
    run_id: &str,
) -> std::result::Result<Vec<ObservationRecord>, String> {
    if let Some(records) = state
        .contract_run_observations
        .lock()
        .expect("contract run observation lock poisoned")
        .get(run_id)
        .cloned()
    {
        return Ok(records);
    }
    if let Some(kv) = &state.kv_store {
        let records = kv_get_corridor_contract_run_observations(kv, run_id).await?;
        state
            .contract_run_observations
            .lock()
            .expect("contract run observation lock poisoned")
            .insert(run_id.to_string(), records.clone());
        return Ok(records);
    }
    Ok(Vec::new())
}

fn append_contract_run_observation(
    state: &WorkerState,
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
    state: &WorkerState,
    run_state: &CorridorContractRunState,
) -> std::result::Result<(), String> {
    let observations = load_corridor_observations(state, &run_state.run_id).await?;
    let evidence = build_evidence_response(
        run_state.run_id.clone(),
        match run_state.status {
            CorridorContractRunStatus::Accepted
            | CorridorContractRunStatus::InProgress
            | CorridorContractRunStatus::WaitingReview => RunStatus::InProgress,
            CorridorContractRunStatus::Completed => RunStatus::Completed,
            CorridorContractRunStatus::Failed => RunStatus::Failed,
            CorridorContractRunStatus::Blocked => RunStatus::Blocked,
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
        .map_err(|err| err.message)
}

async fn persist_corridor_run_state(
    state: &WorkerState,
    run_state: &CorridorContractRunState,
) -> std::result::Result<(), String> {
    state
        .contract_runs
        .lock()
        .expect("contract run lock poisoned")
        .insert(run_state.run_id.clone(), run_state.clone());
    if let Some(kv) = &state.kv_store {
        kv_put_corridor_contract_run_state(kv, run_state).await?;
        let observations = load_corridor_observations(state, &run_state.run_id).await?;
        kv_put_corridor_contract_run_observations(kv, &run_state.run_id, &observations).await?;
    }
    persist_contract_run_snapshot(state, run_state).await
}

fn contract_run_scope_admission(
    run_id: &str,
    operation_id: &str,
    auth_context: &crate::request::AuthContext,
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

fn provider_connector_target_for_ref<'a>(
    registry: &'a [ProviderConnectorRegistryEntry],
    connector_ref: Option<&str>,
) -> Option<&'a ProviderConnectorRegistryEntry> {
    let connector_ref = connector_ref?;
    registry.iter().find(|entry| entry.connector_ref == connector_ref)
}

fn connector_metadata_for_current_step(
    admitted_contract: &AdmittedContract,
    step_id: &str,
) -> std::result::Result<Option<ContractStepConnectorMetadata>, String> {
    let step = flow_step_by_id(&admitted_contract.flow, step_id)
        .ok_or_else(|| format!("contract step was not found: {step_id}"))?;
    if step.step_type.as_deref() != Some("connector_action") {
        return Ok(None);
    }
    let connector = step
        .connector
        .as_ref()
        .ok_or_else(|| format!("connector_action step {step_id} is missing connector metadata"))?;
    let connector_id = connector
        .get("name")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("connector_action step {step_id} is missing connector name"))?
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
        connector_ref: connector
            .get("connector_ref")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string()),
        provider: connector
            .get("provider")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string()),
        operation: connector
            .get("operation")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string()),
        method: connector
            .get("method")
            .and_then(|value| value.as_str())
            .map(|value| value.to_ascii_uppercase()),
        side_effect_intent,
    }))
}

fn build_current_operation_admission(
    run_state: &CorridorContractRunState,
    admitted_contract: Option<&AdmittedContract>,
    verifier: &dyn AuthVerifier,
    provider_connector_registry: &[ProviderConnectorRegistryEntry],
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
        CorridorContractRunStatus::Blocked | CorridorContractRunStatus::Completed | CorridorContractRunStatus::Failed
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
    if step.step_type == ContractStepType::HumanGate {
        match run_state.gate.as_ref() {
            Some(gate) if gate.status == HumanGateStatus::WaitingReview => {
                return OperationAdmission {
                    admission_id: format!("adm_review_{}", run_state.run_id),
                    decision: AdmissionDecision::RequireReview,
                    reason_code: "HUMAN_GATE_REQUIRED".to_string(),
                    policy_version: "policy.default.v0".to_string(),
                    contract_id: run_state.contract_id.clone(),
                    run_id: run_state.run_id.clone(),
                    step_id: run_state.current_step_id.clone(),
                    operation_id: step.operation_id.clone().unwrap_or_default(),
                    allowed_connector_id: None,
                    allowed_endpoint_ref: None,
                    allowed_method: None,
                    constraints: json!({}),
                    expires_at: None,
                };
            }
            Some(gate)
                if gate.status == HumanGateStatus::Resolved
                    && gate.decision == Some(HumanGateDecision::Approve) => {}
            Some(gate)
                if gate.status == HumanGateStatus::Resolved
                    && gate.decision == Some(HumanGateDecision::Reject) =>
            {
                return OperationAdmission {
                    admission_id: format!("adm_review_{}", run_state.run_id),
                    decision: AdmissionDecision::Deny,
                    reason_code: "HUMAN_GATE_REJECTED".to_string(),
                    policy_version: "policy.default.v0".to_string(),
                    contract_id: run_state.contract_id.clone(),
                    run_id: run_state.run_id.clone(),
                    step_id: run_state.current_step_id.clone(),
                    operation_id: step.operation_id.clone().unwrap_or_default(),
                    allowed_connector_id: None,
                    allowed_endpoint_ref: None,
                    allowed_method: None,
                    constraints: json!({}),
                    expires_at: None,
                };
            }
            _ => {
                return OperationAdmission {
                    admission_id: format!("adm_review_{}", run_state.run_id),
                    decision: AdmissionDecision::Deny,
                    reason_code: "HUMAN_GATE_STATE_INVALID".to_string(),
                    policy_version: "policy.default.v0".to_string(),
                    contract_id: run_state.contract_id.clone(),
                    run_id: run_state.run_id.clone(),
                    step_id: run_state.current_step_id.clone(),
                    operation_id: step.operation_id.clone().unwrap_or_default(),
                    allowed_connector_id: None,
                    allowed_endpoint_ref: None,
                    allowed_method: None,
                    constraints: json!({}),
                    expires_at: None,
                };
            }
        }
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
            operation_id: step.operation_id.clone().unwrap_or_default(),
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
            let provider_target = provider_connector_target_for_ref(
                provider_connector_registry,
                metadata.connector_ref.as_deref(),
            );
            (
                Some(metadata.connector_id),
                metadata.endpoint_ref,
                metadata.method.or_else(|| Some("POST".to_string())),
                "NEXT_OPERATION_ALLOWED".to_string(),
                json!({
                    "connector_ref": metadata.connector_ref,
                    "provider": provider_target.map(|value| value.provider.clone()).or(metadata.provider),
                    "operation": provider_target.map(|value| value.operation.clone()).or(metadata.operation),
                    "side_effect_intent": metadata
                        .side_effect_intent
                        .map(|value| serde_json::to_value(value).unwrap_or(Value::Null))
                        .unwrap_or(Value::Null)
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
                operation_id: step.operation_id.clone().unwrap_or_default(),
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
        operation_id: step.operation_id.clone().unwrap_or_default(),
        allowed_connector_id: connector_id,
        allowed_endpoint_ref: endpoint_ref,
        allowed_method: method,
        constraints,
        expires_at: Some("2026-05-23T08:35:00Z".to_string()),
    }
}

fn contains_forbidden_connector_override(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            const FORBIDDEN_KEYS: &[&str] = &[
                "connector_id",
                "connector_ref",
                "endpoint_ref",
                "method",
                "provider",
                "operation",
                "provider_url",
                "telegram_url",
                "target_url",
                "side_effect_intent",
                "bot_token",
                "token",
                "chat_id",
                "secret_ref",
                "secret_refs",
                "token_secret_ref",
                "chat_id_secret_ref",
            ];
            object.keys().any(|key| FORBIDDEN_KEYS.contains(&key.as_str()))
                || object.values().any(contains_forbidden_connector_override)
        }
        Value::Array(items) => items.iter().any(contains_forbidden_connector_override),
        _ => false,
    }
}

fn set_step_status(run_state: &mut CorridorContractRunState, step_id: &str, status: ContractStepStatus) {
    if let Some(step) = run_state.steps.iter_mut().find(|step| step.step_id == step_id) {
        step.status = status;
    }
}

fn step_by_id<'a>(
    run_state: &'a CorridorContractRunState,
    step_id: &str,
) -> Option<&'a crate::contract_step::ContractStep> {
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
    run_state: &mut CorridorContractRunState,
    transition: &ContractTransition,
) -> std::result::Result<(), String> {
    match transition {
        ContractTransition::NextStep { step_id } => {
            let next_step = step_by_id(run_state, step_id)
                .cloned()
                .ok_or_else(|| format!("transition target step was not found: {step_id}"))?;
            run_state.current_step_id = step_id.clone();
            run_state.current_gate_id = None;
            run_state.gate = None;
            match next_step.step_type {
                ContractStepType::HumanGate => {
                    run_state.status = CorridorContractRunStatus::WaitingReview;
                    run_state.next_allowed_operation_id = next_step.operation_id.clone();
                    set_step_status(run_state, step_id, ContractStepStatus::WaitingReview);
                    let gate = gate_for_step(&run_state.run_id, &next_step);
                    run_state.current_gate_id = Some(gate.gate_id.clone());
                    run_state.gate = Some(gate);
                }
                ContractStepType::ConnectorAction | ContractStepType::DeterministicAction => {
                    run_state.status = CorridorContractRunStatus::InProgress;
                    run_state.next_allowed_operation_id = next_step.operation_id.clone();
                    set_step_status(run_state, step_id, ContractStepStatus::Ready);
                }
                ContractStepType::Terminal => {
                    run_state.status = CorridorContractRunStatus::Completed;
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
            run_state.status = CorridorContractRunStatus::Blocked;
            run_state.next_allowed_operation_id = None;
            run_state.current_gate_id = None;
            run_state.gate = None;
        }
    }
    Ok(())
}

fn serialize_contract_run_status(state: &CorridorContractRunState) -> Value {
    serde_json::to_value(crate::contract_run::ContractRunStatusResponse {
        run_id: state.run_id.clone(),
        contract_id: state.contract_id.clone(),
        tenant_id: state.tenant_id.clone(),
        status: state.status,
        current_step_id: state.current_step_id.clone(),
        next_allowed_operation_id: state.next_allowed_operation_id.clone(),
        trace_ref: state.trace_ref.clone(),
        observation_count: state.observation_count,
        gate: state.gate.clone(),
    })
    .unwrap_or_else(|_| json!({}))
}

async fn handle_contract_run_start(
    mut req: Request,
    state: Arc<WorkerState>,
    contract_id: String,
) -> Result<Response> {
    let auth_context = match authorize_worker_public_contract_route(&req, state.as_ref()) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let body = req.bytes().await?;
    let mut payload = match parse_json_with_limit(&body) {
        Ok(v) => v,
        Err(message) if message.starts_with("body too large") => {
            return contract_error(413, "request_too_large", "request body exceeds limit", vec![message]);
        }
        Err(message) => {
            return contract_error(400, "validation_failed", "request parsing failed", vec![message]);
        }
    };
    if let Some(root) = payload.as_object_mut() {
        let context = root
            .entry("context".to_string())
            .or_insert_with(|| json!({}));
        if let Some(context_obj) = context.as_object_mut() {
            if context_obj.get("tenant_id").is_some() {
                return contract_error(
                    400,
                    "tenant_override_forbidden",
                    "client cannot set tenant_id",
                    vec!["context.tenant_id".to_string()],
                );
            }
            context_obj.insert("tenant_id".to_string(), json!(state.public_api.tenant_id.clone()));
        }
        root.insert(
            "auth_context".to_string(),
            serde_json::to_value(&auth_context).unwrap_or_else(|_| json!({})),
        );
    }

    let request: CorridorContractRunRequest = match serde_json::from_value(payload) {
        Ok(value) => value,
        Err(err) => {
            return contract_error(
                400,
                "validation_failed",
                "contract run request parsing failed",
                vec![format!("payload: {err}")],
            );
        }
    };
    if request.request_id.trim().is_empty()
        || request.actor.actor_id.trim().is_empty()
        || request.source.client_id.trim().is_empty()
    {
        return contract_error(
            400,
            "validation_failed",
            "contract run request validation failed",
            vec!["request_id, actor, and source are required".to_string()],
        );
    }

    let admitted_contract = match worker_load_contract(&state, &contract_id).await {
        Ok(Some(contract)) => contract,
        Ok(None) => {
            return contract_error(
                404,
                "contract_not_found",
                "contract was not found",
                vec![format!("contract_id: {contract_id}")],
            )
        }
        Err(err) => {
            return contract_error(503, "storage_unavailable", "contract registry read failed", vec![err]);
        }
    };

    let scope_admission = contract_run_scope_admission(
        &format!("contract_run_{}", request.request_id),
        "op_start_contract",
        &auth_context,
        state.auth_verifier.as_ref(),
    );
    if scope_admission.decision != AdmissionDecision::Allow {
        return contract_error(
            403,
            "authorization_failed",
            "policy authorization failed",
            vec![format!("reason_code: {}", scope_admission.reason_code)],
        );
    }

    if let Err(err) = validate_contract_run_flow(&admitted_contract) {
        return contract_error(400, &err.code, "contract flow is invalid", vec![err.message]);
    }
    let steps = match contract_steps_from_flow(&admitted_contract) {
        Ok(steps) => steps,
        Err(err) => return contract_error(400, &err.code, "contract flow is invalid", vec![err.message]),
    };
    let entry_step = match steps.iter().find(|step| step.step_id == admitted_contract.flow.entry) {
        Some(step) => step.clone(),
        None => {
            return contract_error(
                400,
                "contract_flow_invalid",
                "contract flow entry step was not found",
                vec![format!("entry: {}", admitted_contract.flow.entry)],
            );
        }
    };

    let run_id = format!("contract_run_{}", request.request_id);
    let trace_ref = request
        .correlation
        .get("trace_id")
        .and_then(|value| value.as_str())
        .unwrap_or("trace_contract_001")
        .to_string();

    let mut run_state = CorridorContractRunState {
        run_id: run_id.clone(),
        contract_id: contract_id.clone(),
        tenant_id: state.public_api.tenant_id.clone(),
        status: CorridorContractRunStatus::Accepted,
        auth_context: auth_context.clone(),
        current_step_id: admitted_contract.flow.entry.clone(),
        next_allowed_operation_id: entry_step.operation_id.clone(),
        completed_step_ids: Vec::new(),
        current_gate_id: None,
        trace_ref: trace_ref.clone(),
        observation_count: 0,
        created_at: "2026-05-23T08:30:00Z".to_string(),
        updated_at: "2026-05-23T08:30:00Z".to_string(),
        start_idempotency_key: None,
        last_step_idempotency_key: None,
        steps,
        gate: None,
        step_execution_records: HashMap::new(),
        last_admission: None,
    };
    let admission = build_current_operation_admission(
        &run_state,
        Some(&admitted_contract),
        state.auth_verifier.as_ref(),
        &state.provider_connector_registry,
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

    if let Err(err) = persist_corridor_run_state(&state, &run_state).await {
        return contract_error(503, "storage_unavailable", "contract run persistence failed", vec![err]);
    }

    Response::from_json(&json!({
        "run_id": run_id,
        "contract_id": contract_id,
        "status": run_state.status.as_str(),
        "current_step_id": run_state.current_step_id,
        "next_allowed_operation_id": run_state.next_allowed_operation_id,
        "trace_ref": trace_ref,
        "observation_count": run_state.observation_count,
        "gate": Value::Null
    }))
    .map(|r| r.with_status(202))
}

async fn handle_contract_run_status(state: Arc<WorkerState>, run_id: String) -> Result<Response> {
    match load_corridor_run_state(&state, &run_id).await {
        Ok(Some(run_state)) => Response::from_json(&serialize_contract_run_status(&run_state)),
        Ok(None) => contract_error(
            404,
            "run_not_found",
            "contract run was not found",
            vec![format!("run_id: {run_id}")],
        ),
        Err(err) => contract_error(503, "storage_unavailable", "contract run read failed", vec![err]),
    }
}

async fn handle_contract_run_next(state: Arc<WorkerState>, run_id: String) -> Result<Response> {
    let mut run_state = match load_corridor_run_state(&state, &run_id).await {
        Ok(Some(run_state)) => run_state,
        Ok(None) => {
            return contract_error(
                404,
                "run_not_found",
                "contract run was not found",
                vec![format!("run_id: {run_id}")],
            )
        }
        Err(err) => return contract_error(503, "storage_unavailable", "contract run read failed", vec![err]),
    };
    let admitted_contract = match worker_load_contract(&state, &run_state.contract_id).await {
        Ok(contract) => contract,
        Err(err) => return contract_error(503, "storage_unavailable", "contract registry read failed", vec![err]),
    };
    let admission = build_current_operation_admission(
        &run_state,
        admitted_contract.as_ref(),
        state.auth_verifier.as_ref(),
        &state.provider_connector_registry,
    );
    run_state.last_admission = Some(admission.clone());
    if let Err(err) = persist_corridor_run_state(&state, &run_state).await {
        return contract_error(503, "storage_unavailable", "contract run persistence failed", vec![err]);
    }
    let step = current_contract_step(&run_state).expect("current step must exist");
    Response::from_json(&json!({
        "run_id": run_state.run_id,
        "contract_id": run_state.contract_id,
        "step": step,
        "operation_admission": admission
    }))
}

async fn handle_contract_run_step_execute(
    mut req: Request,
    state: Arc<WorkerState>,
    run_id: String,
    step_id: String,
) -> Result<Response> {
    let body = req.bytes().await?;
    let payload = match parse_json_with_limit(&body) {
        Ok(v) => v,
        Err(message) if message.starts_with("body too large") => {
            return contract_error(413, "request_too_large", "request body exceeds limit", vec![message]);
        }
        Err(message) => return contract_error(400, "validation_failed", "request parsing failed", vec![message]),
    };
    let operation_id = payload
        .get("operation_id")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    if operation_id.is_empty() {
        return contract_error(
            400,
            "validation_failed",
            "operation_id is required",
            vec!["operation_id".to_string()],
        );
    }

    let mut run_state = match load_corridor_run_state(&state, &run_id).await {
        Ok(Some(run_state)) => run_state,
        Ok(None) => {
            return contract_error(
                404,
                "run_not_found",
                "contract run was not found",
                vec![format!("run_id: {run_id}")],
            )
        }
        Err(err) => return contract_error(503, "storage_unavailable", "contract run read failed", vec![err]),
    };

    if run_state.current_step_id != step_id {
        return contract_error(
            409,
            "step_state_conflict",
            "step_id is not the current step",
            vec![format!("current_step_id: {}", run_state.current_step_id)],
        );
    }
    if run_state.status == CorridorContractRunStatus::WaitingReview {
        return contract_error(
            403,
            "human_gate_required",
            "run is waiting for human gate resolution",
            vec!["reason_code: HUMAN_GATE_REQUIRED".to_string()],
        );
    }
    if matches!(
        run_state.status,
        CorridorContractRunStatus::Completed | CorridorContractRunStatus::Failed | CorridorContractRunStatus::Blocked
    ) {
        return contract_error(
            403,
            "run_terminal",
            "run is already terminal",
            vec!["reason_code: RUN_ALREADY_TERMINAL".to_string()],
        );
    }
    if run_state.next_allowed_operation_id.as_deref() != Some(operation_id.as_str()) {
        return contract_error(
            403,
            "operation_not_allowed",
            "operation_id is not allowed for current step",
            vec![format!("operation_id: {operation_id}")],
        );
    }
    if payload.get("connector_id").is_some()
        || payload.get("connector_ref").is_some()
        || payload.get("endpoint_ref").is_some()
        || payload.get("method").is_some()
        || payload.get("provider").is_some()
        || payload.get("operation").is_some()
        || payload.get("provider_url").is_some()
        || payload.get("telegram_url").is_some()
        || payload.get("target_url").is_some()
        || payload.get("side_effect_intent").is_some()
        || payload.get("bot_token").is_some()
        || payload.get("token").is_some()
        || payload.get("chat_id").is_some()
        || payload.get("secret_ref").is_some()
        || payload.get("secret_refs").is_some()
        || payload.get("token_secret_ref").is_some()
        || payload.get("chat_id_secret_ref").is_some()
    {
        return contract_error(
            403,
            "connector_override_forbidden",
            "client-provided connector override is forbidden",
            vec!["CONNECTOR_OVERRIDE_FORBIDDEN".to_string()],
        );
    }
    if contains_forbidden_connector_override(payload.get("input_payload").unwrap_or(&Value::Null)) {
        return contract_error(
            403,
            "connector_override_forbidden",
            "nested connector override is forbidden",
            vec!["CONNECTOR_OVERRIDE_FORBIDDEN".to_string()],
        );
    }

    let step = current_contract_step(&run_state).expect("current step must exist");
    let admitted_contract = match worker_load_contract(&state, &run_state.contract_id).await {
        Ok(Some(contract)) => contract,
        Ok(None) => {
            return contract_error(
                403,
                "contract_not_found",
                "admitted contract for run was not found",
                vec![format!("contract_id: {}", run_state.contract_id)],
            )
        }
        Err(err) => return contract_error(503, "storage_unavailable", "contract registry read failed", vec![err]),
    };
    let admission = build_current_operation_admission(
        &run_state,
        Some(&admitted_contract),
        state.auth_verifier.as_ref(),
        &state.provider_connector_registry,
    );
    if admission.decision != AdmissionDecision::Allow {
        return contract_error(
            403,
            "operation_not_allowed",
            "operation admission denied",
            vec![format!("reason_code: {}", admission.reason_code)],
        );
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
                return contract_error(
                    403,
                    "connector_not_allowed",
                    "connector metadata missing in admission",
                    vec!["CONNECTOR_NOT_ALLOWED".to_string()],
                )
            }
        };
        let method = admission
            .allowed_method
            .clone()
            .unwrap_or_else(|| "POST".to_string());
        let endpoint_ref = admission.allowed_endpoint_ref.clone();
        let connector_ref = admission
            .constraints
            .get("connector_ref")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());
        let provider = admission
            .constraints
            .get("provider")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let provider_operation = admission
            .constraints
            .get("operation")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let auth_context = serde_json::to_value(&run_state.auth_context).unwrap_or_else(|_| json!({}));
        let connector_request = ConnectorExecutionRequest {
            connector_id: connector_id.clone(),
            call_id: format!("call_{}_{}", run_state.run_id, operation_id),
            side_effect_intent,
            request: redact_json(&json!({
                "endpoint_ref": endpoint_ref,
                "connector_ref": connector_ref.clone(),
                "provider": provider.clone(),
                "operation": provider_operation.clone(),
                "method": method,
                "body": payload.get("input_payload").cloned().unwrap_or_else(|| json!({})),
                "text": payload
                    .get("input_payload")
                    .and_then(|value| value.get("text"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("MOVA contract-run notification"),
                "operation_id": operation_id,
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
            Err(ConnectorExecutionError { code, message }) => {
                let status = if code == "connector_secret_missing" || code == "connector_secret_unavailable" {
                    503
                } else if code.contains("denied")
                    || code == "connector_ref_not_allowed"
                    || code == "connector_ref_disabled"
                    || code == "connector_operation_not_allowed"
                    || code == "provider_not_supported"
                    || code == "provider_operation_not_supported"
                    || code == "connector_scope_denied"
                {
                    403
                } else {
                    502
                };
                return contract_error(
                    status,
                    "connector_execution_failed",
                    &message,
                    vec![format!("connector_code: {code}")],
                );
            }
        };
        let response = connector_result.call.response.clone();
        let mut summary = json!({
            "connector_id": connector_id,
            "endpoint_ref": endpoint_ref,
            "method": method,
            "side_effect_intent": side_effect_intent,
            "connector_status": connector_result.call.status,
            "provider": response.get("provider").cloned().unwrap_or_else(|| json!("unknown")),
            "connector_mode": response.get("connector_mode").cloned().unwrap_or_else(|| json!("unknown")),
            "response_preview": response.get("response_preview").cloned().unwrap_or_else(|| json!({})),
            "attempts": response.get("attempts").cloned().unwrap_or_else(|| json!(1)),
            "timeout_ms": response.get("timeout_ms").cloned().unwrap_or_else(|| json!(0))
        });
        if let Some(ref_value) = response.get("connector_ref").cloned().or_else(|| connector_ref.map(|value| json!(value))) {
            summary["connector_ref"] = ref_value;
        }
        if let Some(operation_value) = response.get("operation").cloned().or_else(|| {
            if provider_operation.is_empty() {
                None
            } else {
                Some(json!(provider_operation))
            }
        }) {
            summary["operation"] = operation_value;
        }
        summary
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
    let transition = match resolve_flow_transition(&admitted_contract, &executed_step_id, outcome) {
        Ok(transition) => transition,
        Err(err) => {
            run_state.status = CorridorContractRunStatus::Failed;
            run_state.next_allowed_operation_id = None;
            run_state.current_gate_id = None;
            run_state.gate = None;
            run_state.updated_at = "2026-05-23T08:33:00Z".to_string();
            run_state.observation_count = append_contract_run_observation(
                &state,
                &run_state.run_id,
                ObservationRecord {
                    run_id: run_state.run_id.clone(),
                    step_id: executed_step_id.clone(),
                    event_type: "contract_run.transition_failed".to_string(),
                    timestamp: "2026-05-23T08:33:00Z".to_string(),
                    subject: json!({}),
                    result: json!({
                        "from_step_id": executed_step_id,
                        "outcome": outcome.as_flow_key(),
                        "reason_code": "TRANSITION_NOT_FOUND",
                        "error_code": err.code,
                        "message": err.message
                    }),
                    metadata: json!({}),
                    evidence_ref: format!("ev_{}_{}", run_state.run_id, run_state.observation_count + 1),
                },
            );
            let _ = persist_corridor_run_state(&state, &run_state).await;
            return contract_error(
                409,
                "transition_not_found",
                "contract transition could not be resolved",
                vec![err.message],
            );
        }
    };
    let transition_reason_code = match &transition {
        ContractTransition::NextStep { .. } => "TRANSITION_APPLIED".to_string(),
        ContractTransition::Terminal { reason_code, .. } => reason_code.clone(),
        ContractTransition::Blocked { reason_code } => reason_code.clone(),
    };
    if let Err(err) = apply_transition_to_run_state(&mut run_state, &transition) {
        return contract_error(502, "contract_step_not_found", "transition target step was not found", vec![err]);
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
    run_state.observation_count = append_contract_run_observation(
        &state,
        &run_state.run_id,
        ObservationRecord {
            run_id: run_state.run_id.clone(),
            step_id: executed_step_id.clone(),
            event_type: "contract_run.transition_applied".to_string(),
            timestamp: "2026-05-23T08:33:00Z".to_string(),
            subject: json!({"operation_id": operation_id}),
            result: transition_trace_json(&executed_step_id, outcome, &transition, &transition_reason_code)
                .as_object()
                .cloned()
                .map(Value::Object)
                .unwrap_or_else(|| json!({})),
            metadata: json!({}),
            evidence_ref: format!("ev_{}_{}", run_state.run_id, run_state.observation_count + 1),
        },
    );

    if let Err(err) = persist_corridor_run_state(&state, &run_state).await {
        return contract_error(503, "storage_unavailable", "contract run persistence failed", vec![err]);
    }

    Response::from_json(&json!({
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
    }))
    .map(|r| r.with_status(202))
}

async fn handle_contract_run_current_gate(state: Arc<WorkerState>, run_id: String) -> Result<Response> {
    match load_corridor_run_state(&state, &run_id).await {
        Ok(Some(run_state)) => match &run_state.gate {
            Some(gate) if gate.status == HumanGateStatus::WaitingReview => Response::from_json(&json!({
                "run_id": run_id,
                "gate_id": gate.gate_id,
                "step_id": gate.step_id,
                "status": "waiting_review",
                "reason_code": gate.reason_code,
                "prompt": gate.prompt,
                "requested_operation_id": gate.requested_operation_id,
                "created_at": gate.created_at
            })),
            _ => Response::from_json(&json!({"run_id": run_id, "gate": Value::Null})),
        },
        Ok(None) => contract_error(
            404,
            "run_not_found",
            "contract run was not found",
            vec![format!("run_id: {run_id}")],
        ),
        Err(err) => contract_error(503, "storage_unavailable", "contract run read failed", vec![err]),
    }
}

async fn handle_contract_run_gate_resolve(
    mut req: Request,
    state: Arc<WorkerState>,
    run_id: String,
    gate_id: String,
) -> Result<Response> {
    let body = req.bytes().await?;
    let payload = match parse_json_with_limit(&body) {
        Ok(v) => v,
        Err(message) => return contract_error(400, "validation_failed", "request parsing failed", vec![message]),
    };
    let request: HumanGateResolutionRequest = match serde_json::from_value(payload) {
        Ok(value) => value,
        Err(err) => {
            return contract_error(
                400,
                "validation_failed",
                "gate resolution parsing failed",
                vec![format!("payload: {err}")],
            );
        }
    };

    let mut run_state = match load_corridor_run_state(&state, &run_id).await {
        Ok(Some(run_state)) => run_state,
        Ok(None) => {
            return contract_error(
                404,
                "run_not_found",
                "contract run was not found",
                vec![format!("run_id: {run_id}")],
            )
        }
        Err(err) => return contract_error(503, "storage_unavailable", "contract run read failed", vec![err]),
    };
    let gate = match run_state.gate.clone() {
        Some(gate) if gate.status == HumanGateStatus::WaitingReview && gate.gate_id == gate_id => gate,
        _ => {
            return contract_error(
                409,
                "gate_state_conflict",
                "only current active gate can be resolved",
                vec![format!("gate_id: {gate_id}")],
            );
        }
    };
    let admitted_contract = match worker_load_contract(&state, &run_state.contract_id).await {
        Ok(Some(contract)) => contract,
        Ok(None) => {
            return contract_error(
                404,
                "contract_not_found",
                "admitted contract for run was not found",
                vec![format!("contract_id: {}", run_state.contract_id)],
            )
        }
        Err(err) => return contract_error(503, "storage_unavailable", "contract registry read failed", vec![err]),
    };
    let gate_step_id = gate.step_id.clone();
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
        run_state.status = CorridorContractRunStatus::InProgress;
        run_state.next_allowed_operation_id =
            step_by_id(&run_state, &run_state.current_step_id).and_then(|step| step.operation_id.clone());
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
            operation_id: run_state.next_allowed_operation_id.clone().unwrap_or_default(),
            allowed_connector_id: None,
            allowed_endpoint_ref: None,
            allowed_method: None,
            constraints: json!({}),
            expires_at: None,
        });
    } else {
        let transition = match resolve_flow_transition(
            &admitted_contract,
            &run_state.current_step_id,
            ContractStepOutcome::Reject,
        ) {
            Ok(transition) => transition,
            Err(err) => {
                run_state.status = CorridorContractRunStatus::Failed;
                run_state.next_allowed_operation_id = None;
                run_state.current_gate_id = None;
                run_state.gate = None;
                run_state.updated_at = "2026-05-23T08:32:00Z".to_string();
                run_state.observation_count = append_contract_run_observation(
                    &state,
                    &run_state.run_id,
                    ObservationRecord {
                        run_id: run_state.run_id.clone(),
                        step_id: gate_step_id.clone(),
                        event_type: "contract_run.transition_failed".to_string(),
                        timestamp: "2026-05-23T08:32:00Z".to_string(),
                        subject: json!({}),
                        result: json!({
                            "from_step_id": gate_step_id,
                            "outcome": ContractStepOutcome::Reject.as_flow_key(),
                            "reason_code": "TRANSITION_NOT_FOUND",
                            "error_code": err.code,
                            "message": err.message
                        }),
                        metadata: json!({}),
                        evidence_ref: format!("ev_{}_{}", run_state.run_id, run_state.observation_count + 1),
                    },
                );
                let _ = persist_corridor_run_state(&state, &run_state).await;
                return contract_error(
                    409,
                    "transition_not_found",
                    "contract transition could not be resolved",
                    vec![err.message],
                );
            }
        };
        let current_step_id = run_state.current_step_id.clone();
        set_step_status(&mut run_state, &current_step_id, ContractStepStatus::Blocked);
        let _ = apply_transition_to_run_state(&mut run_state, &transition);
        run_state.last_admission = Some(OperationAdmission {
            admission_id: format!("adm_gate_{}", run_state.run_id),
            decision: AdmissionDecision::Deny,
            reason_code: "HUMAN_GATE_REJECTED".to_string(),
            policy_version: "policy.default.v0".to_string(),
            contract_id: run_state.contract_id.clone(),
            run_id: run_state.run_id.clone(),
            step_id: gate_step_id.clone(),
            operation_id: resolved_gate.requested_operation_id.clone(),
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
            step_id: gate_step_id.clone(),
            event_type: "contract_run.gate_resolved".to_string(),
            timestamp: run_state.updated_at.clone(),
            subject: json!({"gate_id": gate_id, "actor_id": request.actor.actor_id}),
            result: json!({
                "step_id": gate_step_id,
                "requested_operation_id": resolved_gate.requested_operation_id,
                "decision": if request.decision == HumanGateDecision::Approve { "approve" } else { "reject" },
                "reason_code": if request.decision == HumanGateDecision::Approve {
                    "HUMAN_GATE_APPROVED"
                } else {
                    "HUMAN_GATE_REJECTED"
                }
            }),
            metadata: json!({"reason": request.reason}),
            evidence_ref: format!("ev_{}_{}", run_state.run_id, run_state.observation_count + 1),
        },
    );

    if let Err(err) = persist_corridor_run_state(&state, &run_state).await {
        return contract_error(503, "storage_unavailable", "contract run persistence failed", vec![err]);
    }

    Response::from_json(&json!({
        "run_id": run_state.run_id,
        "gate_id": gate_id,
        "status": "resolved",
        "decision": if request.decision == HumanGateDecision::Approve { "approve" } else { "reject" },
        "next_step_id": run_state.current_step_id,
        "current_step_id": run_state.current_step_id,
        "next_allowed_operation_id": run_state.next_allowed_operation_id,
        "trace_ref": run_state.trace_ref,
        "observation_count": run_state.observation_count
    }))
}

async fn handle_contract_run_evidence(state: Arc<WorkerState>, run_id: String) -> Result<Response> {
    let run_state = match load_corridor_run_state(&state, &run_id).await {
        Ok(Some(run_state)) => run_state,
        Ok(None) => {
            return contract_error(
                404,
                "run_not_found",
                "contract run was not found",
                vec![format!("run_id: {run_id}")],
            )
        }
        Err(err) => return contract_error(503, "storage_unavailable", "contract run read failed", vec![err]),
    };
    let observations = match load_corridor_observations(&state, &run_id).await {
        Ok(records) => records,
        Err(err) => return contract_error(503, "storage_unavailable", "contract run read failed", vec![err]),
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
                "kind": record.result.get("transition").and_then(|value| value.get("kind")).cloned().unwrap_or(Value::Null),
                "target_step_id": record.result.get("transition").and_then(|value| value.get("target_step_id")).cloned().unwrap_or(Value::Null),
                "terminal_status": record.result.get("transition").and_then(|value| value.get("terminal_status")).cloned().unwrap_or(Value::Null),
                "reason_code": record.result.get("reason_code").cloned().unwrap_or(Value::Null)
            })
        })
        .collect::<Vec<_>>();
    let transition_failures = observations
        .iter()
        .filter(|record| record.event_type == "contract_run.transition_failed")
        .map(|record| {
            json!({
                "from_step_id": record.result.get("from_step_id").cloned().unwrap_or(Value::Null),
                "outcome": record.result.get("outcome").cloned().unwrap_or(Value::Null),
                "reason_code": record.result.get("reason_code").cloned().unwrap_or(Value::Null),
                "error_code": record.result.get("error_code").cloned().unwrap_or(Value::Null),
                "message": record.result.get("message").cloned().unwrap_or(Value::Null)
            })
        })
        .collect::<Vec<_>>();
    let gates = observations
        .iter()
        .filter(|record| record.event_type == "contract_run.gate_resolved")
        .map(|record| {
            json!({
                "gate_id": record.subject.get("gate_id").cloned().unwrap_or(Value::Null),
                "step_id": record.result.get("step_id").cloned().unwrap_or(Value::Null),
                "requested_operation_id": record.result.get("requested_operation_id").cloned().unwrap_or(Value::Null),
                "decision": record.result.get("decision").cloned().unwrap_or(Value::Null),
                "decided_by": record.subject.get("actor_id").cloned().unwrap_or(Value::Null),
                "reason_code": record.result.get("reason_code").cloned().unwrap_or(Value::Null)
            })
        })
        .collect::<Vec<_>>();
    Response::from_json(&build_contract_run_evidence_response(
        run_state.run_id.clone(),
        run_state.contract_id.clone(),
        run_state.tenant_id.clone(),
        run_state.status,
        run_state.trace_ref.clone(),
        &observations,
        json!(steps),
        json!(gates),
        json!(transitions),
        json!(transition_failures),
        run_state.last_step_idempotency_key.clone().or(run_state.start_idempotency_key.clone()),
        contract_run_policy_summary(run_state.last_admission.as_ref()),
    ))
}

async fn dispatch_worker_route(req: Request, state: Arc<WorkerState>, route: WorkerRoute) -> Result<Response> {
    if let Err(err) = authorize_worker_public_contract_route(&req, state.as_ref()) {
        return err;
    }
    match route {
        WorkerRoute::StartContractRun { contract_id } => handle_contract_run_start(req, state, contract_id).await,
        WorkerRoute::GetContractRunStatus { run_id } => handle_contract_run_status(state, run_id).await,
        WorkerRoute::GetContractRunNext { run_id } => handle_contract_run_next(state, run_id).await,
        WorkerRoute::ExecuteContractRunStep { run_id, step_id } => {
            handle_contract_run_step_execute(req, state, run_id, step_id).await
        }
        WorkerRoute::GetCurrentGate { run_id } => handle_contract_run_current_gate(state, run_id).await,
        WorkerRoute::ResolveGate { run_id, gate_id } => handle_contract_run_gate_resolve(req, state, run_id, gate_id).await,
        WorkerRoute::GetContractRunEvidence { run_id } => handle_contract_run_evidence(state, run_id).await,
    }
}


#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let method = req.method().to_string();
    let path = req.path();
    let state = Arc::new(state_from_env(&env));
    if let Some(route) = match_worker_route(method.as_str(), path.as_str()) {
        return dispatch_worker_route(req, state, route).await;
    }
    if !matches!(path.as_str(), "/health" | "/ready" | "/capabilities") {
        return Response::from_json(&json!({
            "error": {
                "code": "route_not_found",
                "message": "route is not public in v0.1 runtime surface",
                "details": [format!("method: {method}"), format!("path: {path}")]
            }
        }))
        .map(|response| response.with_status(404));
    }
    let state_run = Arc::clone(&state);
    let state_get_run = Arc::clone(&state);
    let state_get_evidence = Arc::clone(&state);
    let state_contract_register = Arc::clone(&state);
    let state_contracts = Arc::clone(&state);
    let state_contract = Arc::clone(&state);
    let state_contract_run = Arc::clone(&state);
    let state_contract_decision = Arc::clone(&state);
    Router::new()
        .get_async("/health", |_req, _ctx| async move {
            Response::from_json(&json!({
                "status": "ok",
                "service": "mova-agent-api-v0",
                "version": "v0"
            }))
        })
        .get_async("/ready", |_req, _ctx| async move {
            Response::from_json(&json!({
                "ready": true,
                "checks": {
                    "storage": "bound",
                    "connector": "configured"
                }
            }))
        })
        .get_async("/capabilities", |_req, _ctx| async move {
            Response::from_json(&json!({
                "action_types": ["validate_document", "webhook_notify"],
                "policy_decisions": ["allow", "deny", "require_review", "redact"],
                "execution_path": [
                    "agent_request",
                    "action_model",
                    "policy_admission",
                    "flat_execution",
                    "connector_call",
                    "observation_write",
                    "evidence_response"
                ],
                "runtime_provider": {
                    "provider_kind": "cloudflare_worker",
                    "supports_env_loading": true,
                    "supports_secret_resolution": true,
                    "supports_live_deploy_binding": true
                },
                "contract_run": {
                    "supported": true,
                    "supports_next_step": true,
                    "supports_human_gate": true,
                    "supports_terminal_evidence": true,
                    "worker_surface": true
                },
                "provider_connectors": {
                    "supported": true,
                    "registry_env": "MOVA_PROVIDER_CONNECTOR_REGISTRY_JSON",
                    "providers": [
                        {
                            "provider": "telegram",
                            "operations": ["send_message"],
                            "status": "first_adapter"
                        }
                    ]
                }
            }))
        })
        .post_async("/contracts/register", move |mut req, _ctx| {
            let state = Arc::clone(&state_contract_register);
            async move {
                let body = req.bytes().await?;
                let payload = match parse_json_with_limit(&body) {
                    Ok(v) => v,
                    Err(message) if message.starts_with("body too large") => {
                        return contract_error(413, "request_too_large", "request body exceeds limit", vec![message]);
                    }
                    Err(message) => {
                        return contract_error(400, "validation_failed", "request parsing failed", vec![message]);
                    }
                };
                let payload: RegisterContractRequest = match serde_json::from_value(payload) {
                    Ok(v) => v,
                    Err(err) => {
                        return contract_error(400, "contract_register_invalid", "invalid register payload", vec![err.to_string()]);
                    }
                };
                let resolved = match resolve_contract_register_source(&payload).await {
                    Ok(v) => v,
                    Err((code, message, details)) => return contract_error(400, &code, &message, details),
                };
                if let Some(secret_field) = detect_secret_like_json(&resolved.flow, "flow") {
                    return contract_error(
                        400,
                        "contract_registration_secret_like_payload",
                        "contract package contains secret-looking value",
                        vec![secret_field],
                    );
                }
                if let Some(policy) = resolved.policy.as_ref() {
                    if let Some(secret_field) = detect_secret_like_json(policy, "policy") {
                        return contract_error(
                            400,
                            "contract_registration_secret_like_payload",
                            "contract package contains secret-looking value",
                            vec![secret_field],
                        );
                    }
                }
                let flow = match parse_inline_flow_json(&resolved.flow) {
                    Ok(flow) => flow,
                    Err(err) => {
                        return contract_error(400, &err.code, "inline_flow_json parsing failed", vec![err.message]);
                    }
                };
                let connector_requirements = match parse_contract_connector_requirements(payload.connector_requirements) {
                    Ok(c) => c,
                    Err(err) => {
                        return contract_error(400, &err.code, "connector requirements are invalid", vec![err.message]);
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
                    return contract_error(400, &err.code, "contract admission validation failed", vec![err.message]);
                }
                {
                    let mut contracts = state
                        .admitted_contracts
                        .lock()
                        .expect("contract registry lock poisoned");
                    contracts.insert(admitted.contract_id.clone(), admitted);
                }
                if let Some(kv) = &state.kv_store {
                    if let Some(saved) = state
                        .admitted_contracts
                        .lock()
                        .expect("contract registry lock poisoned")
                        .get(&payload.contract_id)
                        .cloned()
                    {
                        if let Err(err) = kv_upsert_contract(kv, &saved).await {
                            return contract_error(503, "storage_unavailable", "contract registry persistence failed", vec![err]);
                        }
                    }
                }
                Response::from_json(&ContractRegisterResponse {
                    contract_id: payload.contract_id,
                    admitted: true,
                    mode: resolved.mode,
                })
                .map(|r| r.with_status(201))
            }
        })
        .get_async("/contracts", move |_req, _ctx| {
            let state = Arc::clone(&state_contracts);
            async move {
                let contracts = state
                    .admitted_contracts
                    .lock()
                    .expect("contract registry lock poisoned");
                let mut list = contracts
                    .values()
                    .map(|c| ContractSummary {
                        contract_id: c.contract_id.clone(),
                        execution_type: c.execution_type.clone(),
                        has_source_url: c.source_url.is_some(),
                    })
                    .collect::<Vec<_>>();
                drop(contracts);
                if list.is_empty() {
                    if let Some(kv) = &state.kv_store {
                        if let Ok(ids) = kv_list_contract_ids(kv).await {
                            for id in ids {
                                if let Ok(Some(c)) = kv_get_contract(kv, &id).await {
                                    list.push(ContractSummary {
                                        contract_id: c.contract_id,
                                        execution_type: c.execution_type,
                                        has_source_url: c.source_url.is_some(),
                                    });
                                }
                            }
                        }
                    }
                }
                Response::from_json(&json!({ "contracts": list }))
            }
        })
        .get_async("/contracts/:contract_id", move |_req, ctx| {
            let state = Arc::clone(&state_contract);
            async move {
                let contract_id = ctx.param("contract_id").cloned().unwrap_or_default();
                let contracts = state
                    .admitted_contracts
                    .lock()
                    .expect("contract registry lock poisoned");
                match contracts.get(&contract_id) {
                    Some(contract) => Response::from_json(contract),
                    None => {
                        drop(contracts);
                        if let Some(kv) = &state.kv_store {
                            match kv_get_contract(kv, &contract_id).await {
                                Ok(Some(contract)) => {
                                    return Response::from_json(&contract);
                                }
                                Ok(None) => {}
                                Err(err) => {
                                    return contract_error(503, "storage_unavailable", "contract registry read failed", vec![err]);
                                }
                            }
                        }
                        contract_error(
                            404,
                            "contract_not_found",
                            "contract was not found",
                            vec![format!("contract_id: {contract_id}")],
                        )
                    }
                }
            }
        })
        .post_async("/contracts/:contract_id/run", move |mut req, ctx| {
            let state = Arc::clone(&state_contract_run);
            async move {
                let contract_id = ctx.param("contract_id").cloned().unwrap_or_default();
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
                        if let Some(kv) = &state.kv_store {
                            match kv_get_contract(kv, &contract_id).await {
                                Ok(Some(c)) => c,
                                Ok(None) => {
                                    return contract_error(
                                        404,
                                        "contract_not_found",
                                        "contract was not found",
                                        vec![format!("contract_id: {contract_id}")],
                                    );
                                }
                                Err(err) => {
                                    return contract_error(503, "storage_unavailable", "contract registry read failed", vec![err]);
                                }
                            }
                        } else {
                            return contract_error(
                                404,
                                "contract_not_found",
                                "contract was not found",
                                vec![format!("contract_id: {contract_id}")],
                            );
                        }
                    }
                };
                let body = req.bytes().await?;
                let payload = if body.is_empty() {
                    json!({})
                } else {
                    match parse_json_with_limit(&body) {
                        Ok(v) => v,
                        Err(message) if message.starts_with("body too large") => {
                            return contract_error(413, "request_too_large", "request body exceeds limit", vec![message]);
                        }
                        Err(message) => {
                            return contract_error(400, "validation_failed", "request parsing failed", vec![message]);
                        }
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
                let exec = match execute_contract_flow(&admitted, &admitted.flow.entry, &outcomes, None, true) {
                    Ok(v) => v,
                    Err(err) => {
                        return contract_error(400, &err.code, "contract runtime execution failed", vec![err.message]);
                    }
                };
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
                            "telegram_delivery_executed": exec.telegram_executed
                        }
                    }),
                    metadata: json!({}),
                    evidence_ref: format!("ev_{run_id}"),
                });
                let evidence = build_evidence_response(
                    run_id.clone(),
                    if exec.waiting_for_human { RunStatus::Blocked } else { RunStatus::Completed },
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
                    return contract_error(503, "storage_unavailable", "storage adapter failed", vec![err.message]);
                }
                if exec.waiting_for_human {
                    let pending = LegacyContractRunState {
                        run_id: run_id.clone(),
                        contract_id: contract_id.clone(),
                        current_step_id: exec.current_step_id.clone(),
                        status: exec.status.clone(),
                        waiting_for_human: true,
                        trace_ref,
                        outcomes,
                    };
                    let mut runs = state
                        .legacy_contract_runs
                        .lock()
                        .expect("contract run lock poisoned");
                    runs.insert(run_id.clone(), pending.clone());
                    drop(runs);
                    if let Some(kv) = &state.kv_store {
                        if let Err(err) = kv_put_legacy_contract_run_state(kv, &pending).await {
                            return contract_error(
                                503,
                                "storage_unavailable",
                                "contract run state persistence failed",
                                vec![err],
                            );
                        }
                    }
                }
                Response::from_json(&ContractRunResponse {
                    run_id,
                    contract_id,
                    status: exec.status,
                    current_step_id: exec.current_step_id,
                    waiting_for_human: exec.waiting_for_human,
                })
                .map(|r| r.with_status(202))
            }
        })
        .post_async("/contracts/runs/:run_id/decision", move |mut req, ctx| {
            let state = Arc::clone(&state_contract_decision);
            async move {
                let run_id = ctx.param("run_id").cloned().unwrap_or_default();
                let body = req.bytes().await?;
                let payload = match parse_json_with_limit(&body) {
                    Ok(v) => v,
                    Err(message) => return contract_error(400, "validation_failed", "request parsing failed", vec![message]),
                };
                let payload: ContractDecisionRequest = match serde_json::from_value(payload) {
                    Ok(v) => v,
                    Err(err) => {
                        return contract_error(400, "contract_decision_invalid", "invalid decision payload", vec![err.to_string()]);
                    }
                };
                if payload.decision != "approve" && payload.decision != "reject" {
                    return contract_error(
                        400,
                        "contract_decision_invalid",
                        "decision must be approve or reject",
                        vec![format!("decision: {}", payload.decision)],
                    );
                }
                let run_state = {
                    let runs = state.legacy_contract_runs.lock().expect("contract run lock poisoned");
                    runs.get(&run_id).cloned()
                };
                let run_state = match run_state {
                    Some(v) => v,
                    None => {
                        if let Some(kv) = &state.kv_store {
                            match kv_get_legacy_contract_run_state(kv, &run_id).await {
                                Ok(Some(v)) => v,
                                Ok(None) => {
                                    return contract_error(
                                        404,
                                        "contract_run_not_found",
                                        "contract run was not found or not waiting_human",
                                        vec![format!("run_id: {run_id}")],
                                    );
                                }
                                Err(err) => {
                                    return contract_error(
                                        503,
                                        "storage_unavailable",
                                        "contract run state read failed",
                                        vec![err],
                                    );
                                }
                            }
                        } else {
                        return contract_error(
                            404,
                            "contract_run_not_found",
                            "contract run was not found or not waiting_human",
                            vec![format!("run_id: {run_id}")],
                        );
                        }
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
                        if let Some(kv) = &state.kv_store {
                            match kv_get_contract(kv, &run_state.contract_id).await {
                                Ok(Some(c)) => c,
                                Ok(None) => {
                                    return contract_error(
                                        404,
                                        "contract_not_found",
                                        "admitted contract for run was not found",
                                        vec![format!("contract_id: {}", run_state.contract_id)],
                                    );
                                }
                                Err(err) => {
                                    return contract_error(
                                        503,
                                        "storage_unavailable",
                                        "contract registry read failed",
                                        vec![err],
                                    );
                                }
                            }
                        } else {
                        return contract_error(
                            404,
                            "contract_not_found",
                            "admitted contract for run was not found",
                            vec![format!("contract_id: {}", run_state.contract_id)],
                        );
                        }
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
                        return contract_error(400, &err.code, "contract continuation failed", vec![err.message]);
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
                    return contract_error(503, "storage_unavailable", "storage adapter failed", vec![err.message]);
                }
                {
                    let mut runs = state.legacy_contract_runs.lock().expect("contract run lock poisoned");
                    runs.remove(&run_id);
                }
                if let Some(kv) = &state.kv_store {
                    if let Err(err) = kv_delete_legacy_contract_run_state(kv, &run_id).await {
                        return contract_error(
                            503,
                            "storage_unavailable",
                            "contract run state cleanup failed",
                            vec![err],
                        );
                    }
                }
                Response::from_json(&json!({
                    "run_id": run_state.run_id,
                    "contract_id": run_state.contract_id,
                    "status": status,
                    "waiting_for_human": false
                }))
            }
        })
        .post_async("/actions/validate", |mut req, _ctx| async move {
            let body = req.bytes().await?;
            let payload = match parse_json_with_limit(&body) {
                Ok(v) => v,
                Err(message) if message.starts_with("body too large") => {
                    return json_error(413, "request_too_large", "request body exceeds limit", vec![message]);
                }
                Err(message) => {
                    return json_error(400, "validation_failed", "request parsing failed", vec![message]);
                }
            };
            match parse_request_envelope(payload) {
                Ok(envelope) => {
                    let semantic_errors = validate_request_envelope(&envelope);
                    if semantic_errors.is_empty() {
                        Response::from_json(&json!({"valid": true}))
                    } else {
                        json_error(
                            400,
                            "validation_failed",
                            "request validation failed",
                            semantic_errors
                                .iter()
                                .map(|e| format!("{}: {}", e.field, e.message))
                                .collect::<Vec<_>>(),
                        )
                    }
                }
                Err(err) => json_error(
                    400,
                    "validation_failed",
                    "request parsing failed",
                    vec![format!("payload: {err}")],
                ),
            }
        })
        .post_async("/actions/run", move |mut req, _ctx| {
            let state = Arc::clone(&state_run);
            async move {
                let idempotency_key = req
                    .headers()
                    .get("idempotency-key")
                    .ok()
                    .flatten()
                    .map(|v| sanitize_idempotency_key(v.as_str()));
                let body = req.bytes().await?;
                let payload = match parse_json_with_limit(&body) {
                    Ok(v) => v,
                    Err(message) if message.starts_with("body too large") => {
                        return json_error(413, "request_too_large", "request body exceeds limit", vec![message]);
                    }
                    Err(message) => {
                        return json_error(400, "validation_failed", "request parsing failed", vec![message]);
                    }
                };
                let envelope = match parse_request_envelope(payload) {
                    Ok(v) => v,
                    Err(err) => {
                        return json_error(
                            400,
                            "validation_failed",
                            "request parsing failed",
                            vec![format!("payload: {err}")],
                        );
                    }
                };
                let semantic_errors = validate_request_envelope(&envelope);
                if !semantic_errors.is_empty() {
                    return json_error(
                        400,
                        "validation_failed",
                        "request validation failed",
                        semantic_errors
                            .iter()
                            .map(|e| format!("{}: {}", e.field, e.message))
                            .collect::<Vec<_>>(),
                    );
                }

                let client_id = envelope.source.client_id.clone();
                if state.denied_client_ids.iter().any(|v| v == &client_id) {
                    return json_error(
                        429,
                        "rate_limited",
                        "request denied by abuse guard policy",
                        vec![format!("client_id: {client_id}")],
                    );
                }

                let run_id = if let Some(key) = idempotency_key.clone() {
                    format!("run_idem_{key}")
                } else {
                    format!("run_{}", envelope.request_id)
                };
                if idempotency_key.is_some() {
                    match state.run_store.get_snapshot(&run_id).await {
                        Ok(Some(snapshot)) => {
                            return Response::from_json(&json!({
                                "run_id": snapshot.run_id,
                                "status": snapshot.evidence.status.as_str(),
                                "trace_ref": snapshot.evidence.trace_ref,
                                "observation_count": snapshot.evidence.observation_refs.len(),
                                "idempotent_replay": true
                            }));
                        }
                        Ok(None) => {}
                        Err(err) => {
                            return json_error(
                                503,
                                "storage_unavailable",
                                "storage adapter failed",
                                vec![err.message],
                            );
                        }
                    }
                }
                let admission = PolicyAdmission::from_auth_context(
                    format!("adm_{}", envelope.request_id),
                    envelope.action.action_id.clone(),
                    "policy.default.v0".to_string(),
                    "actions.run",
                    envelope.auth_context.clone(),
                    state.auth_verifier.as_ref(),
                );
                if admission.decision != AdmissionDecision::Allow {
                    return json_error(
                        403,
                        "authorization_failed",
                        "policy authorization failed",
                        vec![format!("reason_code: {}", admission.reason_code)],
                    );
                }

                let _plan =
                    FlatExecutionPlan::from_action(run_id.clone(), envelope.action.action_id.clone());

                let side_effect_intent = parse_side_effect_intent(&envelope.action.connector_context);
                let connector_id = envelope
                    .action
                    .connector_context
                    .get("connector_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("connector.docs.v1")
                    .to_string();
                let target_url = envelope.action.connector_context.get("target_url").cloned().unwrap_or(Value::Null);
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

                let connector_request = json!({
                    "target_url": target_url,
                    "endpoint_ref": endpoint_ref,
                    "method": method,
                    "headers": headers,
                    "body": envelope.action.input_payload.clone().unwrap_or_else(|| json!({})),
                    "run_id": run_id.clone(),
                    "correlation_id": envelope
                        .correlation
                        .get("correlation_id")
                        .cloned()
                        .unwrap_or(Value::Null),
                    "trace_ref": envelope.action.trace_ref,
                    "input": envelope.action.input_payload.clone().unwrap_or_else(|| json!({}))
                });

                let connector_call = match state
                    .connector_executor
                    .execute(ConnectorExecutionRequest {
                        connector_id,
                        call_id: format!("call_{}", envelope.request_id),
                        side_effect_intent,
                        request: connector_request,
                        auth_context: serde_json::to_value(&envelope.auth_context).unwrap_or_else(|_| json!({})),
                        credential_refs: Vec::new(),
                        policy_result: admission.to_summary(),
                        started_at: "2026-05-23T10:30:00Z".to_string(),
                    })
                    .await
                {
                    Ok(ConnectorExecutionResult { call, .. }) => call,
                    Err(ConnectorExecutionError { code, message }) => {
                        let status = if code.contains("denied") { 403 } else if code.contains("timeout") { 504 } else { 502 };
                        return json_error(
                            status,
                            "connector_execution_failed",
                            &message,
                            vec![format!("connector_code: {code}")],
                        );
                    }
                };

                let mut journal = ObservationJournal::new();
                journal.append(ObservationRecord {
                    run_id: run_id.clone(),
                    step_id: "step_observation_write".to_string(),
                    event_type: "observation.write".to_string(),
                    timestamp: "2026-05-23T10:30:01Z".to_string(),
                    subject: json!({"call_id": connector_call.call_id, "connector_id": connector_call.connector_id}),
                    result: json!({"status":"ok","connector_status": connector_call.status, "connector_response": connector_call.response}),
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
                if let Err(err) = state.run_store.put_snapshot(snapshot.clone()).await {
                    return json_error(
                        503,
                        "storage_unavailable",
                        "storage adapter failed",
                        vec![err.message],
                    );
                }

                Response::from_json(&json!({
                    "run_id": run_id,
                    "status": "completed",
                    "trace_ref": snapshot.evidence.trace_ref,
                    "observation_count": snapshot.evidence.observation_refs.len()
                }))
            }
        })
        .get_async("/runs/:run_id", move |_req, ctx| {
            let state = Arc::clone(&state_get_run);
            async move {
                let run_id = ctx.param("run_id").cloned().unwrap_or_default();
                match state.run_store.get_snapshot(&run_id).await {
                    Ok(Some(snapshot)) => Response::from_json(&json!({
                        "run_id": snapshot.run_id,
                        "status": snapshot.evidence.status.as_str(),
                        "trace_ref": snapshot.evidence.trace_ref,
                        "observation_count": snapshot.evidence.observation_refs.len()
                    })),
                    Ok(None) => Response::error(
                        format!(
                            "{{\"error\":{{\"code\":\"run_not_found\",\"message\":\"run was not found\",\"details\":[\"run_id: {}\"]}}}}",
                            run_id
                        ),
                        404,
                    ),
                    Err(err) => Response::error(
                        format!(
                            "{{\"error\":{{\"code\":\"storage_unavailable\",\"message\":\"{}\"}}}}",
                            err.message
                        ),
                        503,
                    ),
                }
            }
        })
        .get_async("/runs/:run_id/evidence", move |_req, ctx| {
            let state = Arc::clone(&state_get_evidence);
            async move {
                let run_id = ctx.param("run_id").cloned().unwrap_or_default();
                match state.run_store.get_snapshot(&run_id).await {
                    Ok(Some(snapshot)) => Response::from_json(&snapshot.evidence),
                    Ok(None) => Response::error(
                        format!(
                            "{{\"error\":{{\"code\":\"run_not_found\",\"message\":\"run was not found\",\"details\":[\"run_id: {}\"]}}}}",
                            run_id
                        ),
                        404,
                    ),
                    Err(err) => Response::error(
                        format!(
                            "{{\"error\":{{\"code\":\"storage_unavailable\",\"message\":\"{}\"}}}}",
                            err.message
                        ),
                        503,
                    ),
                }
            }
        })
        .run(req, env)
        .await
}
