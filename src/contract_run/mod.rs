use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::contract_step::{ContractStep, ContractStepStatus, ContractStepType};
use crate::contracts::{flow_step_by_id, pick_next_target, AdmittedContract};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractStepOutcome {
    Default,
    Approve,
    Reject,
    Error,
}

impl ContractStepOutcome {
    pub fn as_flow_key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractTransition {
    NextStep { step_id: String },
    Terminal { status: ContractRunStatus, reason_code: String },
    Blocked { reason_code: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractRunDomainError {
    pub code: String,
    pub message: String,
}

impl ContractRunDomainError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

pub fn gate_for_step(run_id: &str, step: &ContractStep) -> HumanGate {
    HumanGate {
        gate_id: format!("gate_{run_id}_{}", step.step_id),
        run_id: run_id.to_string(),
        step_id: step.step_id.clone(),
        status: HumanGateStatus::WaitingReview,
        reason_code: "HUMAN_GATE_REQUIRED".to_string(),
        prompt: "Approve executing gated operation?".to_string(),
        requested_operation_id: step
            .operation_id
            .clone()
            .unwrap_or_else(|| "op_human_gate".to_string()),
        created_at: "2026-05-23T08:31:00Z".to_string(),
        resolved_at: None,
        decision: None,
        decided_by: None,
    }
}

pub fn contract_steps_from_flow(
    admitted_contract: &AdmittedContract,
) -> Result<Vec<ContractStep>, ContractRunDomainError> {
    let entry = admitted_contract.flow.entry.clone();
    let mut steps = Vec::new();
    for flow_step in &admitted_contract.flow.steps {
        let step_type = match flow_step.step_type.as_deref() {
            Some("connector_action") => ContractStepType::ConnectorAction,
            Some("human_gate") => ContractStepType::HumanGate,
            Some("terminal") => ContractStepType::Terminal,
            Some("deterministic_action") => ContractStepType::DeterministicAction,
            Some(other) => {
                return Err(ContractRunDomainError::new(
                    "contract_flow_invalid",
                    format!("unsupported step_type: {other}"),
                ))
            }
            None => {
                return Err(ContractRunDomainError::new(
                    "contract_flow_invalid",
                    format!("step {} missing step_type", flow_step.id),
                ))
            }
        };
        let operation_id = flow_step.operation_id.clone().or_else(|| match step_type {
            ContractStepType::ConnectorAction => Some("op_notify_webhook".to_string()),
            ContractStepType::HumanGate => Some("op_send_report".to_string()),
            _ => None,
        });
        steps.push(ContractStep {
            step_id: flow_step.id.clone(),
            step_type,
            status: if flow_step.id == entry {
                ContractStepStatus::Ready
            } else {
                ContractStepStatus::Pending
            },
            operation_id,
            requires_review: step_type == ContractStepType::HumanGate,
        });
    }
    Ok(steps)
}

pub fn resolve_flow_transition(
    admitted_contract: &AdmittedContract,
    current_step_id: &str,
    outcome: ContractStepOutcome,
) -> Result<ContractTransition, ContractRunDomainError> {
    let step = flow_step_by_id(&admitted_contract.flow, current_step_id).ok_or_else(|| {
        ContractRunDomainError::new(
            "contract_step_not_found",
            format!("missing contract step: {current_step_id}"),
        )
    })?;
    let target = pick_next_target(step, outcome.as_flow_key()).ok_or_else(|| {
        ContractRunDomainError::new(
            "transition_not_found",
            format!(
                "missing transition for step {} and outcome {}",
                current_step_id,
                outcome.as_flow_key()
            ),
        )
    })?;
    let target_object = target.as_object().ok_or_else(|| {
        ContractRunDomainError::new(
            "transition_invalid",
            format!("transition target for step {} must be an object", current_step_id),
        )
    })?;
    if let Some(step_id) = target_object.get("step").and_then(|value| value.as_str()) {
        return Ok(ContractTransition::NextStep {
            step_id: step_id.to_string(),
        });
    }
    if let Some(terminal) = target_object.get("terminal").and_then(|value| value.as_str()) {
        return match terminal {
            "completed" => Ok(ContractTransition::Terminal {
                status: ContractRunStatus::Completed,
                reason_code: "TRANSITION_APPLIED".to_string(),
            }),
            "blocked" => Ok(ContractTransition::Blocked {
                reason_code: "TRANSITION_APPLIED".to_string(),
            }),
            other => Ok(ContractTransition::Terminal {
                status: ContractRunStatus::Failed,
                reason_code: other.to_string(),
            }),
        };
    }
    Err(ContractRunDomainError::new(
        "transition_invalid",
        format!("step {} target must contain step or terminal", current_step_id),
    ))
}
