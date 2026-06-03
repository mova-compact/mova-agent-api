use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::contract_step::{ContractStep, ContractStepStatus, ContractStepType};
use crate::gate::{HumanGate, HumanGateStatus};
use crate::operation_admission::OperationAdmission;
use crate::request::{Actor, AuthContext, Source};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractRunRequest {
    pub request_id: String,
    pub actor: Actor,
    pub source: Source,
    pub auth_context: AuthContext,
    pub inputs: Value,
    pub context: Value,
    pub correlation: Value,
    pub timestamps: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractRunStatus {
    Accepted,
    InProgress,
    WaitingReview,
    Completed,
    Failed,
    Blocked,
}

impl ContractRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::InProgress => "in_progress",
            Self::WaitingReview => "waiting_review",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractRunState {
    pub run_id: String,
    pub contract_id: String,
    pub status: ContractRunStatus,
    pub auth_context: AuthContext,
    pub current_step_id: String,
    pub next_allowed_operation_id: Option<String>,
    pub completed_step_ids: Vec<String>,
    pub current_gate_id: Option<String>,
    pub trace_ref: String,
    pub observation_count: usize,
    pub created_at: String,
    pub updated_at: String,
    pub steps: Vec<ContractStep>,
    pub gate: Option<HumanGate>,
    pub last_admission: Option<OperationAdmission>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractRunStatusResponse {
    pub run_id: String,
    pub contract_id: String,
    pub status: ContractRunStatus,
    pub current_step_id: String,
    pub next_allowed_operation_id: Option<String>,
    pub trace_ref: String,
    pub observation_count: usize,
    pub gate: Option<HumanGate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractRunEvidenceResponse {
    pub run_id: String,
    pub contract_id: String,
    pub status: ContractRunStatus,
    pub result: Value,
    pub evidence: Value,
    pub trace_ref: String,
    pub observation_refs: Vec<String>,
    pub policy_summary: Value,
}

pub fn fixture_steps() -> Vec<ContractStep> {
    vec![
        ContractStep {
            step_id: "step_001".to_string(),
            step_type: ContractStepType::ConnectorAction,
            status: ContractStepStatus::Ready,
            operation_id: Some("op_notify_webhook".to_string()),
            requires_review: false,
        },
        ContractStep {
            step_id: "step_002".to_string(),
            step_type: ContractStepType::HumanGate,
            status: ContractStepStatus::Pending,
            operation_id: Some("op_send_report".to_string()),
            requires_review: true,
        },
        ContractStep {
            step_id: "step_003".to_string(),
            step_type: ContractStepType::Terminal,
            status: ContractStepStatus::Pending,
            operation_id: None,
            requires_review: false,
        },
    ]
}

pub fn default_gate(run_id: &str) -> HumanGate {
    HumanGate {
        gate_id: format!("gate_{run_id}"),
        run_id: run_id.to_string(),
        step_id: "step_002".to_string(),
        status: HumanGateStatus::WaitingReview,
        reason_code: "HUMAN_GATE_REQUIRED".to_string(),
        prompt: "Approve sending the owner report?".to_string(),
        requested_operation_id: "op_send_report".to_string(),
        created_at: "2026-05-23T08:31:00Z".to_string(),
        resolved_at: None,
        decision: None,
        decided_by: None,
    }
}
