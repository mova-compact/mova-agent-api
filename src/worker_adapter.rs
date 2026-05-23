//! Cloudflare Worker deployment adapter for MOVA Agent API V0.
//!
//! This adapter keeps provider-specific routing in Worker runtime while
//! preserving MOVA core module ownership.

use crate::auth::{create_auth_verifier, AuthVerifier};
use crate::connectors::{
    ConnectorExecutionConfig, ConnectorExecutionError, ConnectorExecutionRequest, ConnectorExecutionResult, ConnectorExecutor,
    DisabledWebhookHttpClient, SideEffectIntent, WebhookHttpClient, WebhookHttpResult, WebhookSiteConnectorExecutor,
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
use worker::{event, Context, Env, Method, Request, RequestInit, Response, Result, Router};

struct WorkerState {
    run_store: Arc<dyn RunStore>,
    auth_verifier: Arc<dyn AuthVerifier>,
    connector_executor: Arc<dyn ConnectorExecutor>,
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
        let mut init = RequestInit::new();
        init.with_method(Method::Post);
        init.with_body(Some(payload.into()));
        let mut req = Request::new_with_init(url, &init).map_err(|err| {
            ConnectorExecutionError::new(
                "connector_request_invalid",
                &format!("failed to build webhook request: {err}"),
            )
        })?;
        req.headers_mut()
            .map_err(|err| {
                ConnectorExecutionError::new(
                    "connector_request_invalid",
                    &format!("failed to set webhook headers: {err}"),
                )
            })?
            .set("content-type", "application/json")
            .map_err(|err| {
                ConnectorExecutionError::new(
                    "connector_request_invalid",
                    &format!("failed to set webhook content-type: {err}"),
                )
            })?;

        let mut response = worker::Fetch::Request(req)
            .send()
            .await
            .map_err(|err| {
                ConnectorExecutionError::new("connector_provider_network_failed", &format!("{err}"))
            })?;
        let status = response.status_code();
        let text = response.text().await.unwrap_or_default();
        let preview: String = text.chars().take(256).collect();
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
    })
}

fn state_from_env(env: &Env) -> WorkerState {
    let run_store: Arc<dyn RunStore> = match env.kv("MOVA_RUN_STORE") {
        Ok(kv) => Arc::new(CloudflareKvRunStore::new(kv)),
        Err(_) => Arc::new(crate::storage::InMemoryRunStore::new()),
    };

    let connector_executor: Arc<dyn ConnectorExecutor> = if let Some(cfg) = webhook_config_from_env(env) {
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

#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let state = Arc::new(state_from_env(&env));
    let state_run = Arc::clone(&state);
    let state_get_run = Arc::clone(&state);
    let state_get_evidence = Arc::clone(&state);
    Router::new()
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
            let payload: Value = req.json().await?;
            match parse_request_envelope(payload) {
                Ok(envelope) => {
                    let semantic_errors = validate_request_envelope(&envelope);
                    if semantic_errors.is_empty() {
                        Response::from_json(&json!({"valid": true}))
                    } else {
                        Response::error(
                            format!(
                                "{{\"error\":{{\"code\":\"validation_failed\",\"message\":\"request validation failed\",\"details\":{:?}}}}}",
                                semantic_errors
                                    .iter()
                                    .map(|e| format!("{}: {}", e.field, e.message))
                                    .collect::<Vec<_>>()
                            ),
                            400,
                        )
                    }
                }
                Err(err) => Response::error(
                    format!(
                        "{{\"error\":{{\"code\":\"validation_failed\",\"message\":\"request parsing failed\",\"details\":[\"payload: {}\"]}}}}",
                        err
                    ),
                    400,
                ),
            }
        })
        .post_async("/actions/run", move |mut req, _ctx| {
            let state = Arc::clone(&state_run);
            async move {
                let payload: Value = req.json().await?;
                let envelope = match parse_request_envelope(payload) {
                    Ok(v) => v,
                    Err(err) => {
                        return Response::error(
                            format!(
                                "{{\"error\":{{\"code\":\"validation_failed\",\"message\":\"request parsing failed\",\"details\":[\"payload: {}\"]}}}}",
                                err
                            ),
                            400,
                        );
                    }
                };
                let semantic_errors = validate_request_envelope(&envelope);
                if !semantic_errors.is_empty() {
                    return Response::error(
                        format!(
                            "{{\"error\":{{\"code\":\"validation_failed\",\"message\":\"request validation failed\",\"details\":{:?}}}}}",
                            semantic_errors
                                .iter()
                                .map(|e| format!("{}: {}", e.field, e.message))
                                .collect::<Vec<_>>()
                        ),
                        400,
                    );
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
                    return Response::error(
                        format!(
                            "{{\"error\":{{\"code\":\"authorization_failed\",\"message\":\"policy authorization failed\",\"details\":[\"reason_code: {}\"]}}}}",
                            admission.reason_code
                        ),
                        403,
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
                let target_url = envelope
                    .action
                    .connector_context
                    .get("target_url")
                    .cloned()
                    .unwrap_or(Value::Null);

                let connector_request = json!({
                    "target_url": target_url,
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
                        auth_context: json!({}),
                        credential_refs: Vec::new(),
                        policy_result: admission.to_summary(),
                        started_at: "2026-05-23T10:30:00Z".to_string(),
                    })
                    .await
                {
                    Ok(ConnectorExecutionResult { call, .. }) => call,
                    Err(ConnectorExecutionError { message, .. }) => {
                        return Response::error(
                            format!(
                                "{{\"error\":{{\"code\":\"connector_execution_failed\",\"message\":\"{}\"}}}}",
                                message
                            ),
                            502,
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
                    return Response::error(
                        format!(
                            "{{\"error\":{{\"code\":\"storage_unavailable\",\"message\":\"{}\"}}}}",
                            err.message
                        ),
                        503,
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
