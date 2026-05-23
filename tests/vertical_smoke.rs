use mova_agent_api::connectors::{
    create_connector_executor, ConnectorExecutionConfig, ConnectorExecutionRequest, SideEffectIntent,
};
use mova_agent_api::evidence::{build_evidence_response, RunStatus};
use mova_agent_api::execution::FlatExecutionPlan;
use mova_agent_api::observation::{ObservationJournal, ObservationRecord};
use mova_agent_api::policy::{AdmissionDecision, PolicyAdmission};
use mova_agent_api::request::parse_request_envelope;
use mova_agent_api::secrets::{SecretRef, SecretRefKind};

#[test]
fn vertical_smoke_single_action_path() {
    let request_json = serde_json::json!({
        "request_id": "req_01",
        "actor": {"actor_type": "ai_agent", "actor_id": "agent_001"},
        "source": {"channel": "api", "client_id": "client_001"},
        "action": {
            "action_id": "act_01",
            "action_type": "validate_document",
            "target_kind": "document",
            "input_payload": {"document_id": "doc_123"},
            "policy_context": {"policy_profile_ref": "policy.default.v0"},
            "connector_context": {"connector_set": ["connector.docs.v1"]},
            "trace_ref": "trace:req_01"
        },
        "inputs": {"document_id": "doc_123"},
        "context": {"tenant_id": "tenant_001"},
        "correlation": {"trace_id": "trace_abc123"},
        "timestamps": {"requested_at": "2026-05-23T08:30:00Z"}
    });

    let envelope = parse_request_envelope(request_json).unwrap();

    let admission = PolicyAdmission::new(
        "adm_01".to_string(),
        envelope.action.action_id.clone(),
        AdmissionDecision::Allow,
        "ok".to_string(),
        "policy.default.v0".to_string(),
    );

    let plan = FlatExecutionPlan::from_action("run_01".to_string(), envelope.action.action_id.clone());
    assert_eq!(plan.steps.len(), 4);

    let connector_executor =
        create_connector_executor(&ConnectorExecutionConfig::deterministic_local_default());
    let connector_call = connector_executor
        .execute(ConnectorExecutionRequest {
            connector_id: "connector.docs.v1".to_string(),
            call_id: "call_01".to_string(),
            side_effect_intent: SideEffectIntent::None,
            request: serde_json::json!({"document_id":"doc_123"}),
            auth_context: serde_json::json!({}),
            credential_refs: vec![SecretRef {
                kind: SecretRefKind::SecretRef,
                reference: "secret://connector/docs".to_string(),
            }],
            policy_result: admission.to_summary(),
            started_at: "2026-05-23T09:30:00Z".to_string(),
        })
        .unwrap()
        .call;
    assert_eq!(connector_call.connector_id, "connector.docs.v1");

    let mut journal = ObservationJournal::new();
    journal.append(ObservationRecord {
        run_id: "run_01".to_string(),
        step_id: "step_observation_write".to_string(),
        event_type: "observation.write".to_string(),
        timestamp: "2026-05-23T09:30:01Z".to_string(),
        subject: serde_json::json!({"call_id": connector_call.call_id}),
        result: serde_json::json!({"status": "ok"}),
        metadata: serde_json::json!({}),
        evidence_ref: "ev_01".to_string(),
    });

    let evidence = build_evidence_response(
        "run_01".to_string(),
        RunStatus::Completed,
        envelope.action.trace_ref.clone(),
        journal.records(),
        admission.to_summary(),
    );

    assert_eq!(evidence.run_id, "run_01");
    assert_eq!(evidence.trace_ref, "trace:req_01");
    assert_eq!(evidence.observation_refs, vec!["ev_01"]);
}
