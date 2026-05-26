//! Cloudflare Worker deployment adapter for MOVA Agent API V0.
//!
//! This adapter keeps provider-specific routing in Worker runtime while
//! preserving MOVA core module ownership.

use crate::auth::{create_auth_verifier, AuthVerifier};
use crate::connectors::{
    ConnectorExecutionConfig, ConnectorExecutionError, ConnectorExecutionRequest, ConnectorExecutionResult, ConnectorExecutor,
    DisabledWebhookHttpClient, EndpointEvidencePolicy, EndpointRegistryEntry, GenericHttpClient,
    GenericHttpConnectorExecutor, GenericHttpRequest, SideEffectIntent, WebhookHttpClient, WebhookHttpResult,
    WebhookSiteConnectorExecutor,
};
use crate::contracts::{
    extract_outcomes_map, flow_step_by_id, parse_contract_connector_requirements, parse_inline_flow_json,
    pick_next_target, validate_admitted_contract, AdmittedContract, ContractFlowStep, ContractRegistryError,
};
use crate::evidence::{build_evidence_response, RunStatus};
use crate::execution::FlatExecutionPlan;
use crate::observation::{ObservationJournal, ObservationRecord};
use crate::policy::{AdmissionDecision, PolicyAdmission};
use crate::request::{parse_request_envelope, validate_request_envelope};
use crate::storage::{CloudflareKvRunStore, RunSnapshot, RunStore};
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
    denied_client_ids: Vec<String>,
    kv_store: Option<worker::kv::KvStore>,
    admitted_contracts: Arc<Mutex<HashMap<String, AdmittedContract>>>,
    contract_runs: Arc<Mutex<HashMap<String, ContractRunState>>>,
}

fn shared_admitted_contracts() -> Arc<Mutex<HashMap<String, AdmittedContract>>> {
    static CONTRACTS: OnceLock<Arc<Mutex<HashMap<String, AdmittedContract>>>> = OnceLock::new();
    CONTRACTS
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

fn shared_contract_runs() -> Arc<Mutex<HashMap<String, ContractRunState>>> {
    static RUNS: OnceLock<Arc<Mutex<HashMap<String, ContractRunState>>>> = OnceLock::new();
    RUNS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

const CONTRACT_INDEX_KEY: &str = "contract_admit:index";
const CONTRACT_KEY_PREFIX: &str = "contract_admit:";
const CONTRACT_RUN_KEY_PREFIX: &str = "contract_run:";

fn contract_kv_key(contract_id: &str) -> String {
    format!("{CONTRACT_KEY_PREFIX}{contract_id}")
}

fn contract_run_kv_key(run_id: &str) -> String {
    format!("{CONTRACT_RUN_KEY_PREFIX}{run_id}")
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
struct ContractRunState {
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
        timeout_ms: 10_000,
        max_retries: 0,
    })
}

fn http_generic_config_from_env(env: &Env) -> Option<ConnectorExecutionConfig> {
    if let Ok(raw) = env.var("MOVA_HTTP_ENDPOINT_REGISTRY_JSON") {
        let parsed: Vec<EndpointRegistryEntry> = serde_json::from_str(&raw.to_string()).ok()?;
        if !parsed.is_empty() {
            return Some(ConnectorExecutionConfig {
                adapter_kind: "http_generic".to_string(),
                allowed_connectors: vec!["connector.http.generic.v1".to_string()],
                allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
                offline_stub_rules: Vec::new(),
                allowed_webhook_urls: Vec::new(),
                endpoint_registry: parsed,
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
        allowed_connectors: vec!["connector.http.generic.v1".to_string()],
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
        if cfg.validate().is_ok() {
            Arc::new(GenericHttpConnectorExecutor::new(
                cfg,
                Arc::new(WorkerWebhookHttpClient),
            ))
        } else {
            Arc::new(GenericHttpConnectorExecutor::new(
                ConnectorExecutionConfig {
                    adapter_kind: "http_generic".to_string(),
                    allowed_connectors: vec!["connector.http.generic.v1".to_string()],
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
                    timeout_ms: 10_000,
                    max_retries: 0,
                },
                Arc::new(DisabledWebhookHttpClient),
            ))
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

    WorkerState {
        run_store,
        auth_verifier: Arc::from(create_auth_verifier(&crate::auth::AuthTrustConfig::default_v0())),
        connector_executor,
        denied_client_ids,
        kv_store,
        admitted_contracts: shared_admitted_contracts(),
        contract_runs: shared_contract_runs(),
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

async fn kv_put_contract_run_state(
    kv: &worker::kv::KvStore,
    run_state: &ContractRunState,
) -> std::result::Result<(), String> {
    let payload = serde_json::to_string(run_state).map_err(|e| format!("serialize run state failed: {e}"))?;
    kv.put(&contract_run_kv_key(&run_state.run_id), payload)
        .map_err(|e| format!("kv put run state build failed: {e}"))?
        .execute()
        .await
        .map_err(|e| format!("kv put run state execute failed: {e}"))?;
    Ok(())
}

async fn kv_get_contract_run_state(
    kv: &worker::kv::KvStore,
    run_id: &str,
) -> std::result::Result<Option<ContractRunState>, String> {
    kv.get(&contract_run_kv_key(run_id))
        .json::<ContractRunState>()
        .await
        .map_err(|e| format!("kv get run state failed: {e}"))
}

async fn kv_delete_contract_run_state(
    kv: &worker::kv::KvStore,
    run_id: &str,
) -> std::result::Result<(), String> {
    kv.delete(&contract_run_kv_key(run_id))
        .await
        .map_err(|e| format!("kv delete run state failed: {e}"))
}


#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let state = Arc::new(state_from_env(&env));
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
                    "supports_secret_resolution": false,
                    "supports_live_deploy_binding": true
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
                    let pending = ContractRunState {
                        run_id: run_id.clone(),
                        contract_id: contract_id.clone(),
                        current_step_id: exec.current_step_id.clone(),
                        status: exec.status.clone(),
                        waiting_for_human: true,
                        trace_ref,
                        outcomes,
                    };
                    let mut runs = state
                        .contract_runs
                        .lock()
                        .expect("contract run lock poisoned");
                    runs.insert(run_id.clone(), pending.clone());
                    drop(runs);
                    if let Some(kv) = &state.kv_store {
                        if let Err(err) = kv_put_contract_run_state(kv, &pending).await {
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
                    let runs = state.contract_runs.lock().expect("contract run lock poisoned");
                    runs.get(&run_id).cloned()
                };
                let run_state = match run_state {
                    Some(v) => v,
                    None => {
                        if let Some(kv) = &state.kv_store {
                            match kv_get_contract_run_state(kv, &run_id).await {
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
                    let mut runs = state.contract_runs.lock().expect("contract run lock poisoned");
                    runs.remove(&run_id);
                }
                if let Some(kv) = &state.kv_store {
                    if let Err(err) = kv_delete_contract_run_state(kv, &run_id).await {
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
