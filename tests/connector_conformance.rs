use mova_agent_api::connectors::{
    create_connector_executor, ConnectorCallStatus, ConnectorExecutionConfig, ConnectorExecutionError,
    ConnectorExecutionRequest, ConnectorExecutor, EndpointEvidencePolicy, EndpointRegistryEntry, GenericHttpClient,
    GenericHttpConnectorExecutor, GenericHttpRequest, OfflineStubRule, SideEffectIntent, WebhookHttpClient,
    WebhookHttpResult, WebhookSiteConnectorExecutor,
};
use mova_agent_api::policy::{AdmissionDecision, PolicySummary};
use mova_agent_api::secrets::{SecretRef, SecretRefKind};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use std::sync::Mutex;

fn request(connector_id: &str, intent: SideEffectIntent) -> ConnectorExecutionRequest {
    ConnectorExecutionRequest {
        connector_id: connector_id.to_string(),
        call_id: "call_conformance_01".to_string(),
        side_effect_intent: intent,
        request: json!({"document_id":"doc_123"}),
        auth_context: json!({}),
        credential_refs: vec![SecretRef {
            kind: SecretRefKind::SecretRef,
            reference: "secret://connector/docs".to_string(),
        }],
        policy_result: PolicySummary {
            decision: AdmissionDecision::Allow,
            policy_version: "policy.default.v0".to_string(),
            reason_code: "authorized".to_string(),
        },
        started_at: "2026-05-23T10:00:00Z".to_string(),
    }
}

#[tokio::test]
async fn deterministic_local_allows_none_intent() {
    let cfg = ConnectorExecutionConfig::deterministic_local_default();
    let exec = create_connector_executor(&cfg);
    let result = exec
        .execute(request("connector.docs.v1", SideEffectIntent::None))
        .await
        .unwrap();
    assert_eq!(result.call.status, ConnectorCallStatus::Completed);
}

#[tokio::test]
async fn deterministic_local_denies_external_network_intent() {
    let cfg = ConnectorExecutionConfig::deterministic_local_default();
    let exec = create_connector_executor(&cfg);
    let err = exec
        .execute(request(
            "connector.docs.v1",
            SideEffectIntent::ExternalNetwork,
        ))
        .await
        .unwrap_err();
    assert_eq!(err.code, "connector_side_effect_denied");
}

#[tokio::test]
async fn deterministic_local_denies_unknown_connector() {
    let cfg = ConnectorExecutionConfig::deterministic_local_default();
    let exec = create_connector_executor(&cfg);
    let err = exec
        .execute(request("connector.unknown", SideEffectIntent::None))
        .await
        .unwrap_err();
    assert_eq!(err.code, "connector_denied");
}

#[tokio::test]
async fn offline_stub_returns_rule_driven_result() {
    let cfg = ConnectorExecutionConfig {
        adapter_kind: "offline_stub".to_string(),
        allowed_connectors: vec![],
        allowed_side_effect_intents: vec![],
        allowed_webhook_urls: vec![],
        endpoint_registry: vec![],
        timeout_ms: 10_000,
        max_retries: 0,
        offline_stub_rules: vec![OfflineStubRule {
            connector_id: "connector.docs.v1".to_string(),
            call_status: ConnectorCallStatus::Completed,
            guard_reason: "stubbed_allow".to_string(),
        }],
    };
    let exec = create_connector_executor(&cfg);
    let result = exec
        .execute(request("connector.docs.v1", SideEffectIntent::LocalOnly))
        .await
        .unwrap();
    assert_eq!(result.guard_reason, "stubbed_allow");
}

#[tokio::test]
async fn invalid_config_maps_to_deterministic_failure() {
    let cfg = ConnectorExecutionConfig {
        adapter_kind: "unsupported".to_string(),
        allowed_connectors: vec![],
        allowed_side_effect_intents: vec![],
        allowed_webhook_urls: vec![],
        endpoint_registry: vec![],
        timeout_ms: 10_000,
        max_retries: 0,
        offline_stub_rules: vec![],
    };
    let exec = create_connector_executor(&cfg);
    let err = exec
        .execute(request("connector.docs.v1", SideEffectIntent::None))
        .await
        .unwrap_err();
    assert_eq!(err.code, "connector_config_invalid");
}

#[derive(Clone)]
struct MockWebhookClient {
    result: Result<WebhookHttpResult, ConnectorExecutionError>,
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl WebhookHttpClient for MockWebhookClient {
    async fn post_json(
        &self,
        _url: &str,
        _body: &serde_json::Value,
    ) -> Result<WebhookHttpResult, ConnectorExecutionError> {
        self.result.clone()
    }
}

#[tokio::test]
async fn webhook_site_allows_only_allowlisted_target() {
    let cfg = ConnectorExecutionConfig {
        adapter_kind: "webhook_site".to_string(),
        allowed_connectors: vec!["connector.webhook_site.v1".to_string()],
        allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
        offline_stub_rules: vec![],
        allowed_webhook_urls: vec!["https://webhook.site/allowed-token".to_string()],
        endpoint_registry: vec![],
        timeout_ms: 10_000,
        max_retries: 0,
    };
    let exec = WebhookSiteConnectorExecutor::new(
        cfg,
        Arc::new(MockWebhookClient {
            result: Ok(WebhookHttpResult {
                status: 200,
                body_preview: "ok".to_string(),
            }),
        }),
    );
    let result = exec
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.webhook_site.v1".to_string(),
            call_id: "call_webhook_01".to_string(),
            side_effect_intent: SideEffectIntent::ExternalNetwork,
            request: json!({
                "target_url": "https://webhook.site/allowed-token",
                "run_id": "run_01",
                "correlation_id": "corr_01",
                "trace_ref": "trace:run_01"
            }),
            auth_context: json!({"scopes":["actions.run"]}),
            credential_refs: vec![],
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T10:00:00Z".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(result.call.status, ConnectorCallStatus::Completed);
    assert_eq!(
        result.call.response["provider"].as_str(),
        Some("webhook.site")
    );
}

#[tokio::test]
async fn webhook_site_denies_non_allowlisted_target() {
    let cfg = ConnectorExecutionConfig {
        adapter_kind: "webhook_site".to_string(),
        allowed_connectors: vec!["connector.webhook_site.v1".to_string()],
        allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
        offline_stub_rules: vec![],
        allowed_webhook_urls: vec!["https://webhook.site/allowed-token".to_string()],
        endpoint_registry: vec![],
        timeout_ms: 10_000,
        max_retries: 0,
    };
    let exec = WebhookSiteConnectorExecutor::new(
        cfg,
        Arc::new(MockWebhookClient {
            result: Ok(WebhookHttpResult {
                status: 200,
                body_preview: "ok".to_string(),
            }),
        }),
    );
    let err = exec
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.webhook_site.v1".to_string(),
            call_id: "call_webhook_02".to_string(),
            side_effect_intent: SideEffectIntent::ExternalNetwork,
            request: json!({
                "target_url": "https://webhook.site/other-token",
                "run_id": "run_02",
                "correlation_id": "corr_02",
                "trace_ref": "trace:run_02"
            }),
            auth_context: json!({"scopes":["actions.run"]}),
            credential_refs: vec![],
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T10:00:00Z".to_string(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "connector_target_denied");
}

#[tokio::test]
async fn webhook_site_maps_provider_http_failure() {
    let cfg = ConnectorExecutionConfig {
        adapter_kind: "webhook_site".to_string(),
        allowed_connectors: vec!["connector.webhook_site.v1".to_string()],
        allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
        offline_stub_rules: vec![],
        allowed_webhook_urls: vec!["https://webhook.site/allowed-token".to_string()],
        endpoint_registry: vec![],
        timeout_ms: 10_000,
        max_retries: 0,
    };
    let exec = WebhookSiteConnectorExecutor::new(
        cfg,
        Arc::new(MockWebhookClient {
            result: Ok(WebhookHttpResult {
                status: 500,
                body_preview: "failed".to_string(),
            }),
        }),
    );
    let err = exec
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.webhook_site.v1".to_string(),
            call_id: "call_webhook_03".to_string(),
            side_effect_intent: SideEffectIntent::ExternalNetwork,
            request: json!({
                "target_url": "https://webhook.site/allowed-token",
                "run_id": "run_03",
                "correlation_id": "corr_03",
                "trace_ref": "trace:run_03"
            }),
            auth_context: json!({"scopes":["actions.run"]}),
            credential_refs: vec![],
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T10:00:00Z".to_string(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "connector_provider_http_failed");
}

#[derive(Clone)]
struct MockGenericHttpClient {
    result: Result<WebhookHttpResult, ConnectorExecutionError>,
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl GenericHttpClient for MockGenericHttpClient {
    async fn execute(
        &self,
        _request: GenericHttpRequest,
    ) -> Result<WebhookHttpResult, ConnectorExecutionError> {
        self.result.clone()
    }
}

#[derive(Clone)]
struct RetryThenSuccessHttpClient {
    calls: Arc<Mutex<u8>>,
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl GenericHttpClient for RetryThenSuccessHttpClient {
    async fn execute(
        &self,
        _request: GenericHttpRequest,
    ) -> Result<WebhookHttpResult, ConnectorExecutionError> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        if *calls == 1 {
            Err(ConnectorExecutionError::new(
                "connector_provider_timeout",
                "timeout",
            ))
        } else {
            Ok(WebhookHttpResult {
                status: 200,
                body_preview: "ok".to_string(),
            })
        }
    }
}

fn generic_cfg() -> ConnectorExecutionConfig {
    ConnectorExecutionConfig {
        adapter_kind: "http_generic".to_string(),
        allowed_connectors: vec!["connector.http.generic.v1".to_string()],
        allowed_side_effect_intents: vec![SideEffectIntent::ExternalNetwork],
        offline_stub_rules: vec![],
        allowed_webhook_urls: vec![],
        endpoint_registry: vec![EndpointRegistryEntry {
            endpoint_ref: "webhook_site_test".to_string(),
            url: "https://webhook.site/allowed-token".to_string(),
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
    }
}

#[tokio::test]
async fn generic_http_allows_allowlisted_endpoint_ref() {
    let exec = GenericHttpConnectorExecutor::new(
        generic_cfg(),
        Arc::new(MockGenericHttpClient {
            result: Ok(WebhookHttpResult {
                status: 200,
                body_preview: "ok".to_string(),
            }),
        }),
    );

    let result = exec
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.http.generic.v1".to_string(),
            call_id: "call_http_01".to_string(),
            side_effect_intent: SideEffectIntent::ExternalNetwork,
            request: json!({
                "endpoint_ref": "webhook_site_test",
                "method": "POST",
                "headers": {"x-correlation-id":"corr_01"},
                "body": {"hello":"world"}
            }),
            auth_context: json!({"scopes":["actions.run"]}),
            credential_refs: vec![],
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T10:00:00Z".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(result.call.status, ConnectorCallStatus::Completed);
    assert_eq!(result.call.response["provider"], "http.generic.v1");
}

#[tokio::test]
async fn generic_http_denies_unknown_endpoint_ref() {
    let exec = GenericHttpConnectorExecutor::new(
        generic_cfg(),
        Arc::new(MockGenericHttpClient {
            result: Ok(WebhookHttpResult {
                status: 200,
                body_preview: "ok".to_string(),
            }),
        }),
    );
    let err = exec
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.http.generic.v1".to_string(),
            call_id: "call_http_02".to_string(),
            side_effect_intent: SideEffectIntent::ExternalNetwork,
            request: json!({
                "endpoint_ref": "unknown",
                "method": "POST"
            }),
            auth_context: json!({"scopes":["actions.run"]}),
            credential_refs: vec![],
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T10:00:00Z".to_string(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "endpoint_unknown");
}

#[tokio::test]
async fn generic_http_denies_disallowed_method() {
    let exec = GenericHttpConnectorExecutor::new(
        generic_cfg(),
        Arc::new(MockGenericHttpClient {
            result: Ok(WebhookHttpResult {
                status: 200,
                body_preview: "ok".to_string(),
            }),
        }),
    );
    let err = exec
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.http.generic.v1".to_string(),
            call_id: "call_http_03".to_string(),
            side_effect_intent: SideEffectIntent::ExternalNetwork,
            request: json!({
                "endpoint_ref": "webhook_site_test",
                "method": "GET"
            }),
            auth_context: json!({"scopes":["actions.run"]}),
            credential_refs: vec![],
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T10:00:00Z".to_string(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "endpoint_method_denied");
}

#[tokio::test]
async fn generic_http_maps_timeout_without_retry() {
    let exec = GenericHttpConnectorExecutor::new(
        generic_cfg(),
        Arc::new(MockGenericHttpClient {
            result: Err(ConnectorExecutionError::new(
                "connector_provider_timeout",
                "timeout",
            )),
        }),
    );
    let err = exec
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.http.generic.v1".to_string(),
            call_id: "call_http_04".to_string(),
            side_effect_intent: SideEffectIntent::ExternalNetwork,
            request: json!({
                "endpoint_ref": "webhook_site_test",
                "method": "POST"
            }),
            auth_context: json!({"scopes":["actions.run"]}),
            credential_refs: vec![],
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T10:00:00Z".to_string(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "connector_provider_timeout");
}

#[tokio::test]
async fn generic_http_retries_timeout_then_succeeds() {
    let mut cfg = generic_cfg();
    cfg.endpoint_registry[0].max_retries = 1;
    let calls = Arc::new(Mutex::new(0));
    let exec = GenericHttpConnectorExecutor::new(
        cfg,
        Arc::new(RetryThenSuccessHttpClient {
            calls: Arc::clone(&calls),
        }),
    );
    let result = exec
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.http.generic.v1".to_string(),
            call_id: "call_http_05".to_string(),
            side_effect_intent: SideEffectIntent::ExternalNetwork,
            request: json!({
                "endpoint_ref": "webhook_site_test",
                "method": "POST"
            }),
            auth_context: json!({"scopes":["actions.run"]}),
            credential_refs: vec![],
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T10:00:00Z".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(result.call.response["attempts"], 2);
    assert_eq!(*calls.lock().unwrap(), 2);
}

#[tokio::test]
async fn generic_http_denies_disabled_endpoint() {
    let mut cfg = generic_cfg();
    cfg.endpoint_registry[0].enabled = false;
    let exec = GenericHttpConnectorExecutor::new(
        cfg,
        Arc::new(MockGenericHttpClient {
            result: Ok(WebhookHttpResult {
                status: 200,
                body_preview: "ok".to_string(),
            }),
        }),
    );
    let err = exec
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.http.generic.v1".to_string(),
            call_id: "call_http_07".to_string(),
            side_effect_intent: SideEffectIntent::ExternalNetwork,
            request: json!({
                "endpoint_ref": "webhook_site_test",
                "method": "POST"
            }),
            auth_context: json!({"scopes":["actions.run"]}),
            credential_refs: vec![],
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T10:00:00Z".to_string(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "endpoint_disabled");
}

#[tokio::test]
async fn generic_http_respects_status_only_evidence_policy() {
    let mut cfg = generic_cfg();
    cfg.endpoint_registry[0].evidence_policy = EndpointEvidencePolicy::StatusOnly;
    let exec = GenericHttpConnectorExecutor::new(
        cfg,
        Arc::new(MockGenericHttpClient {
            result: Ok(WebhookHttpResult {
                status: 200,
                body_preview: "should-not-be-exposed".to_string(),
            }),
        }),
    );
    let result = exec
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.http.generic.v1".to_string(),
            call_id: "call_http_08".to_string(),
            side_effect_intent: SideEffectIntent::ExternalNetwork,
            request: json!({
                "endpoint_ref": "webhook_site_test",
                "method": "POST"
            }),
            auth_context: json!({"scopes":["actions.run"]}),
            credential_refs: vec![],
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T10:00:00Z".to_string(),
        })
        .await
        .unwrap();
    assert!(result.call.response.get("response_preview").is_none());
}

#[tokio::test]
async fn generic_http_denies_missing_required_scope() {
    let exec = GenericHttpConnectorExecutor::new(
        generic_cfg(),
        Arc::new(MockGenericHttpClient {
            result: Ok(WebhookHttpResult {
                status: 200,
                body_preview: "ok".to_string(),
            }),
        }),
    );
    let err = exec
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.http.generic.v1".to_string(),
            call_id: "call_http_06".to_string(),
            side_effect_intent: SideEffectIntent::ExternalNetwork,
            request: json!({
                "endpoint_ref": "webhook_site_test",
                "method": "POST"
            }),
            auth_context: json!({"scopes":["actions.validate"]}),
            credential_refs: vec![],
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T10:00:00Z".to_string(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "endpoint_scope_denied");
}

#[tokio::test]
async fn generic_http_denies_disallowed_intent_for_endpoint() {
    let mut cfg = generic_cfg();
    cfg.endpoint_registry[0].allowed_side_effect_intents = vec![SideEffectIntent::LocalOnly];
    let exec = GenericHttpConnectorExecutor::new(
        cfg,
        Arc::new(MockGenericHttpClient {
            result: Ok(WebhookHttpResult {
                status: 200,
                body_preview: "ok".to_string(),
            }),
        }),
    );
    let err = exec
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.http.generic.v1".to_string(),
            call_id: "call_http_09".to_string(),
            side_effect_intent: SideEffectIntent::ExternalNetwork,
            request: json!({
                "endpoint_ref": "webhook_site_test",
                "method": "POST"
            }),
            auth_context: json!({"scopes":["actions.run"]}),
            credential_refs: vec![],
            policy_result: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
            started_at: "2026-05-23T10:00:00Z".to_string(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "endpoint_intent_denied");
}
