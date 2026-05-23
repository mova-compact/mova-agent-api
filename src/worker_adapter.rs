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
use crate::evidence::{build_evidence_response, RunStatus};
use crate::execution::FlatExecutionPlan;
use crate::observation::{ObservationJournal, ObservationRecord};
use crate::policy::{AdmissionDecision, PolicyAdmission};
use crate::request::{parse_request_envelope, validate_request_envelope};
use crate::storage::{CloudflareKvRunStore, RunSnapshot, RunStore};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use worker::wasm_bindgen::JsCast;
use worker::wasm_bindgen::JsValue;
use worker::wasm_bindgen_futures::JsFuture;
use worker::{event, js_sys, web_sys, Context, Env, Request, Response, Result, Router};

const MAX_REQUEST_BODY_BYTES: usize = 64 * 1024;

struct WorkerState {
    run_store: Arc<dyn RunStore>,
    auth_verifier: Arc<dyn AuthVerifier>,
    connector_executor: Arc<dyn ConnectorExecutor>,
    denied_client_ids: Vec<String>,
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
    let run_store: Arc<dyn RunStore> = match env.kv("MOVA_RUN_STORE") {
        Ok(kv) => Arc::new(CloudflareKvRunStore::new_with_ttl(kv, kv_ttl_seconds)),
        Err(_) => Arc::new(crate::storage::InMemoryRunStore::new()),
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


#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let state = Arc::new(state_from_env(&env));
    let state_run = Arc::clone(&state);
    let state_get_run = Arc::clone(&state);
    let state_get_evidence = Arc::clone(&state);
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
