use mova_agent_api::connectors::{
    create_connector_executor, ConnectorCallStatus, ConnectorExecutionConfig, ConnectorExecutionError,
    ConnectorExecutionRequest, ConnectorExecutor, OfflineStubRule, SideEffectIntent, WebhookHttpClient, WebhookHttpResult,
    WebhookSiteConnectorExecutor,
};
use mova_agent_api::policy::{AdmissionDecision, PolicySummary};
use mova_agent_api::secrets::{SecretRef, SecretRefKind};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

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
            auth_context: json!({}),
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
            auth_context: json!({}),
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
            auth_context: json!({}),
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
