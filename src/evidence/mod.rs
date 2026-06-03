//! Evidence skeleton for MOVA Agent API V0.
//!
//! Phase 5 scope:
//! - evidence response structures
//! - deterministic response assembly

use crate::contract_run::{ContractRunEvidenceResponse, ContractRunStatus};
use crate::observation::ObservationRecord;
use crate::policy::PolicySummary;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Accepted,
    InProgress,
    Completed,
    Failed,
    Blocked,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceResponse {
    pub run_id: String,
    pub status: RunStatus,
    pub result: Value,
    pub evidence: Value,
    pub trace_ref: String,
    pub observation_refs: Vec<String>,
    pub policy_summary: PolicySummary,
}

pub fn build_evidence_response(
    run_id: String,
    status: RunStatus,
    trace_ref: String,
    observations: &[ObservationRecord],
    policy_summary: PolicySummary,
) -> EvidenceResponse {
    let observation_refs = observations
        .iter()
        .map(|record| record.evidence_ref.clone())
        .collect::<Vec<_>>();
    let connector_result = observations
        .iter()
        .find(|record| record.event_type == "observation.write")
        .and_then(|record| record.result.get("connector_response").cloned())
        .unwrap_or_else(|| serde_json::json!({}));
    let connector_status = observations
        .iter()
        .find(|record| record.event_type == "observation.write")
        .and_then(|record| record.result.get("connector_status").cloned())
        .unwrap_or_else(|| serde_json::json!("unknown"));

    EvidenceResponse {
        run_id,
        status,
        result: serde_json::json!({"outcome": "skeleton", "connector_status": connector_status}),
        evidence: serde_json::json!({"record_count": observations.len(), "connector_result": connector_result}),
        trace_ref,
        observation_refs,
        policy_summary,
    }
}

pub fn build_contract_run_evidence_response(
    run_id: String,
    contract_id: String,
    status: ContractRunStatus,
    trace_ref: String,
    observations: &[ObservationRecord],
    steps: Value,
    gates: Value,
    transitions: Value,
    policy_summary: PolicySummary,
) -> ContractRunEvidenceResponse {
    let observation_refs = observations
        .iter()
        .map(|record| record.evidence_ref.clone())
        .collect::<Vec<_>>();

    ContractRunEvidenceResponse {
        run_id,
        contract_id,
        status,
        result: serde_json::json!({"outcome": status.as_str()}),
        evidence: serde_json::json!({
            "steps": steps,
            "gates": gates,
            "transitions": transitions
        }),
        trace_ref,
        observation_refs,
        policy_summary: serde_json::to_value(policy_summary).unwrap_or_else(|_| serde_json::json!({})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::AdmissionDecision;

    #[test]
    fn evidence_builder_collects_observation_refs() {
        let observations = vec![
            ObservationRecord {
                run_id: "run_01".to_string(),
                step_id: "step_1".to_string(),
                event_type: "observation.write".to_string(),
                timestamp: "2026-05-23T09:20:00Z".to_string(),
                subject: serde_json::json!({}),
                result: serde_json::json!({}),
                metadata: serde_json::json!({}),
                evidence_ref: "ev_01".to_string(),
            },
            ObservationRecord {
                run_id: "run_01".to_string(),
                step_id: "step_2".to_string(),
                event_type: "observation.write".to_string(),
                timestamp: "2026-05-23T09:20:01Z".to_string(),
                subject: serde_json::json!({}),
                result: serde_json::json!({}),
                metadata: serde_json::json!({}),
                evidence_ref: "ev_02".to_string(),
            },
        ];

        let policy_summary = PolicySummary {
            decision: AdmissionDecision::Allow,
            policy_version: "policy.default.v0".to_string(),
            reason_code: "ok".to_string(),
        };

        let response = build_evidence_response(
            "run_01".to_string(),
            RunStatus::Completed,
            "trace:req_01".to_string(),
            &observations,
            policy_summary,
        );

        assert_eq!(response.run_id, "run_01");
        assert_eq!(response.status, RunStatus::Completed);
        assert_eq!(response.observation_refs, vec!["ev_01", "ev_02"]);
    }

    #[test]
    fn run_status_wire_shape_is_snake_case() {
        let value = serde_json::to_string(&RunStatus::InProgress).unwrap();
        assert_eq!(value, "\"in_progress\"");
    }
}
