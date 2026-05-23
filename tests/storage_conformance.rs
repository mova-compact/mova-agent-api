use mova_agent_api::evidence::{EvidenceResponse, RunStatus};
use mova_agent_api::observation::ObservationRecord;
use mova_agent_api::policy::{AdmissionDecision, PolicySummary};
use mova_agent_api::storage::{
    create_run_store, RunSnapshot, RunStore, StorageConfig,
};

fn sample_snapshot(run_id: &str) -> RunSnapshot {
    RunSnapshot {
        run_id: run_id.to_string(),
        evidence: EvidenceResponse {
            run_id: run_id.to_string(),
            status: RunStatus::Completed,
            result: serde_json::json!({"outcome": "skeleton"}),
            evidence: serde_json::json!({"record_count": 1}),
            trace_ref: format!("trace:{run_id}"),
            observation_refs: vec!["ev_01".to_string()],
            policy_summary: PolicySummary {
                decision: AdmissionDecision::Allow,
                policy_version: "policy.default.v0".to_string(),
                reason_code: "authorized".to_string(),
            },
        },
        observations: vec![ObservationRecord {
            run_id: run_id.to_string(),
            step_id: "step_observation_write".to_string(),
            event_type: "observation.write".to_string(),
            timestamp: "2026-05-23T10:30:01Z".to_string(),
            subject: serde_json::json!({"call_id": "call_01"}),
            result: serde_json::json!({"status": "ok"}),
            metadata: serde_json::json!({}),
            evidence_ref: "ev_01".to_string(),
        }],
    }
}

fn assert_store_roundtrip(store: &dyn RunStore, run_id: &str) {
    let snapshot = sample_snapshot(run_id);
    store.put_snapshot(snapshot.clone()).unwrap();
    let read = store.get_snapshot(run_id).unwrap().unwrap();
    assert_eq!(read.run_id, run_id);
    assert_eq!(read.evidence.trace_ref, format!("trace:{run_id}"));
    assert_eq!(read.observations.len(), 1);
}

#[test]
fn in_memory_store_conformance_roundtrip() {
    let config = StorageConfig::in_memory_default();
    let store = create_run_store(&config);
    assert_store_roundtrip(store.as_ref(), "run_store_mem_01");
}

#[test]
fn file_backed_store_conformance_roundtrip() {
    let unique = format!(
        "mova_agent_api_storage_conformance_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    let config = StorageConfig::file_backed_local(dir.to_string_lossy().to_string());
    let store = create_run_store(&config);
    assert_store_roundtrip(store.as_ref(), "run_store_file_01");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn invalid_storage_config_maps_to_deterministic_failure() {
    let config = StorageConfig {
        adapter_kind: "unsupported".to_string(),
        base_path: None,
    };
    let store = create_run_store(&config);
    let err = store
        .put_snapshot(sample_snapshot("run_store_invalid_01"))
        .unwrap_err();
    assert_eq!(err.code, "storage_config_invalid");
}

#[test]
fn file_backed_missing_base_path_maps_to_deterministic_failure() {
    let config = StorageConfig {
        adapter_kind: "file_backed_local".to_string(),
        base_path: None,
    };
    let store = create_run_store(&config);
    let err = store
        .get_snapshot("run_missing")
        .unwrap_err();
    assert_eq!(err.code, "storage_config_invalid");
}
