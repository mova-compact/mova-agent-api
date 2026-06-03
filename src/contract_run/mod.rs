use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

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

fn parse_contract_step_type(step_type: Option<&str>, step_id: &str) -> Result<ContractStepType, ContractRunDomainError> {
    match step_type {
        Some("connector_action") => Ok(ContractStepType::ConnectorAction),
        Some("human_gate") => Ok(ContractStepType::HumanGate),
        Some("terminal") => Ok(ContractStepType::Terminal),
        Some("deterministic_action") => Ok(ContractStepType::DeterministicAction),
        Some(other) => Err(ContractRunDomainError::new(
            "contract_flow_invalid",
            format!("unsupported step_type for step {step_id}: {other}"),
        )),
        None => Err(ContractRunDomainError::new(
            "contract_flow_invalid",
            format!("step {step_id} missing step_type"),
        )),
    }
}

fn validate_step_ids_unique(admitted_contract: &AdmittedContract) -> Result<(), ContractRunDomainError> {
    let mut step_ids = HashSet::new();
    for step in &admitted_contract.flow.steps {
        if !step_ids.insert(step.id.clone()) {
            return Err(ContractRunDomainError::new(
                "contract_duplicate_step_id",
                format!("duplicate step id: {}", step.id),
            ));
        }
    }
    Ok(())
}

fn validate_entry_exists(admitted_contract: &AdmittedContract) -> Result<(), ContractRunDomainError> {
    if admitted_contract.flow.entry.trim().is_empty() {
        return Err(ContractRunDomainError::new(
            "contract_flow_invalid",
            "flow.entry must not be empty",
        ));
    }
    if flow_step_by_id(&admitted_contract.flow, &admitted_contract.flow.entry).is_none() {
        return Err(ContractRunDomainError::new(
            "contract_step_not_found",
            format!("flow.entry '{}' does not match any step id", admitted_contract.flow.entry),
        ));
    }
    Ok(())
}

fn validate_executable_operation_ids(admitted_contract: &AdmittedContract) -> Result<(), ContractRunDomainError> {
    for step in &admitted_contract.flow.steps {
        let step_type = parse_contract_step_type(step.step_type.as_deref(), &step.id)?;
        let requires_operation_id = matches!(
            step_type,
            ContractStepType::ConnectorAction | ContractStepType::DeterministicAction | ContractStepType::HumanGate
        );
        if requires_operation_id && step.operation_id.as_deref().unwrap_or_default().trim().is_empty() {
            return Err(ContractRunDomainError::new(
                "contract_operation_id_missing",
                format!("missing operation_id for executable step {}", step.id),
            ));
        }
    }
    Ok(())
}

fn validate_connector_metadata(admitted_contract: &AdmittedContract) -> Result<(), ContractRunDomainError> {
    for step in &admitted_contract.flow.steps {
        let step_type = parse_contract_step_type(step.step_type.as_deref(), &step.id)?;
        if step_type != ContractStepType::ConnectorAction {
            continue;
        }
        let connector = step.connector.as_ref().ok_or_else(|| {
            ContractRunDomainError::new(
                "contract_connector_metadata_missing",
                format!("connector_action step {} missing connector metadata", step.id),
            )
        })?;
        let connector_name = connector.get("name").and_then(|value| value.as_str()).unwrap_or_default();
        if connector_name.trim().is_empty() {
            return Err(ContractRunDomainError::new(
                "contract_connector_metadata_missing",
                format!("connector_action step {} missing connector.name", step.id),
            ));
        }
        let endpoint_ref = connector
            .get("endpoint_ref")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if endpoint_ref.trim().is_empty() {
            return Err(ContractRunDomainError::new(
                "contract_connector_metadata_missing",
                format!("connector_action step {} missing connector.endpoint_ref", step.id),
            ));
        }
        let method = connector.get("method").and_then(|value| value.as_str()).unwrap_or_default();
        if method.trim().is_empty() {
            return Err(ContractRunDomainError::new(
                "contract_connector_metadata_missing",
                format!("connector_action step {} missing connector.method", step.id),
            ));
        }
        let side_effect_intent = connector
            .get("side_effect_intent")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if !matches!(side_effect_intent, "none" | "external_network") {
            return Err(ContractRunDomainError::new(
                "contract_connector_metadata_missing",
                format!(
                    "connector_action step {} has unknown connector.side_effect_intent {}",
                    step.id, side_effect_intent
                ),
            ));
        }
    }
    Ok(())
}

fn validate_transition_targets(admitted_contract: &AdmittedContract) -> Result<(), ContractRunDomainError> {
    let valid_step_ids = admitted_contract
        .flow
        .steps
        .iter()
        .map(|step| step.id.as_str())
        .collect::<HashSet<_>>();

    for step in &admitted_contract.flow.steps {
        let step_type = parse_contract_step_type(step.step_type.as_deref(), &step.id)?;
        let next = step.next.as_object().ok_or_else(|| {
            ContractRunDomainError::new(
                "contract_transition_invalid",
                format!("step {} next must be an object", step.id),
            )
        })?;
        if next.is_empty() {
            return Err(ContractRunDomainError::new(
                "contract_transition_invalid",
                format!("step {} next must not be empty", step.id),
            ));
        }
        if step_type == ContractStepType::HumanGate && !(next.contains_key("approve") || next.contains_key("reject")) {
            return Err(ContractRunDomainError::new(
                "contract_transition_invalid",
                format!("human_gate step {} must define approve or reject transition", step.id),
            ));
        }
        for (outcome, target) in next {
            if !matches!(outcome.as_str(), "default" | "approve" | "reject" | "error") {
                return Err(ContractRunDomainError::new(
                    "contract_transition_invalid",
                    format!("step {} has unsupported transition outcome {}", step.id, outcome),
                ));
            }
            let target_object = target.as_object().ok_or_else(|| {
                ContractRunDomainError::new(
                    "contract_transition_invalid",
                    format!("step {} transition {} must be an object", step.id, outcome),
                )
            })?;
            let step_target = target_object.get("step").and_then(|value| value.as_str());
            let terminal_target = target_object.get("terminal").and_then(|value| value.as_str());
            if step_target.is_some() && terminal_target.is_some() {
                return Err(ContractRunDomainError::new(
                    "contract_transition_invalid",
                    format!("step {} transition {} cannot contain both step and terminal", step.id, outcome),
                ));
            }
            if let Some(step_target) = step_target {
                if !valid_step_ids.contains(step_target) {
                    return Err(ContractRunDomainError::new(
                        "contract_transition_target_missing",
                        format!("step {} points to unknown step {}", step.id, step_target),
                    ));
                }
                continue;
            }
            if let Some(terminal_target) = terminal_target {
                if !matches!(terminal_target, "completed" | "blocked" | "failed") {
                    return Err(ContractRunDomainError::new(
                        "contract_transition_invalid",
                        format!(
                            "step {} transition {} uses unsupported terminal {}",
                            step.id, outcome, terminal_target
                        ),
                    ));
                }
                continue;
            }
            return Err(ContractRunDomainError::new(
                "contract_transition_invalid",
                format!("step {} target must contain either step or terminal", step.id),
            ));
        }
    }

    Ok(())
}

pub fn validate_contract_run_flow(admitted_contract: &AdmittedContract) -> Result<(), ContractRunDomainError> {
    validate_entry_exists(admitted_contract)?;
    validate_step_ids_unique(admitted_contract)?;
    validate_executable_operation_ids(admitted_contract)?;
    validate_connector_metadata(admitted_contract)?;
    validate_transition_targets(admitted_contract)?;
    Ok(())
}

pub fn gate_for_step(run_id: &str, step: &ContractStep) -> HumanGate {
    HumanGate {
        gate_id: format!("gate_{run_id}_{}", step.step_id),
        run_id: run_id.to_string(),
        step_id: step.step_id.clone(),
        status: HumanGateStatus::WaitingReview,
        reason_code: "HUMAN_GATE_REQUIRED".to_string(),
        prompt: "Approve executing gated operation?".to_string(),
        requested_operation_id: step.operation_id.clone().unwrap_or_default(),
        created_at: "2026-05-23T08:31:00Z".to_string(),
        resolved_at: None,
        decision: None,
        decided_by: None,
    }
}

pub fn contract_steps_from_flow(
    admitted_contract: &AdmittedContract,
) -> Result<Vec<ContractStep>, ContractRunDomainError> {
    validate_contract_run_flow(admitted_contract)?;
    let entry = admitted_contract.flow.entry.clone();
    let mut steps = Vec::new();
    for flow_step in &admitted_contract.flow.steps {
        let step_type = parse_contract_step_type(flow_step.step_type.as_deref(), &flow_step.id)?;
        steps.push(ContractStep {
            step_id: flow_step.id.clone(),
            step_type,
            status: if flow_step.id == entry {
                ContractStepStatus::Ready
            } else {
                ContractStepStatus::Pending
            },
            operation_id: flow_step.operation_id.clone(),
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
        "contract_transition_invalid",
        format!("step {} target must contain step or terminal", current_step_id),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::ContractFlow;
    use serde_json::json;

    fn admitted_contract_with_flow(flow: ContractFlow) -> AdmittedContract {
        AdmittedContract {
            contract_id: "contract_v0".to_string(),
            execution_type: "agent".to_string(),
            source_type: Some("test".to_string()),
            source_url: None,
            commit_sha: None,
            contract_path: None,
            registered_at: None,
            admitted: Some(true),
            manifest: None,
            flow,
            policy: None,
            connector_requirements: None,
            evidence_expectations: None,
            open_questions: None,
        }
    }

    #[test]
    fn validate_contract_run_flow_rejects_missing_operation_id() {
        let contract = admitted_contract_with_flow(ContractFlow {
            version: "1.0".to_string(),
            description: "x".to_string(),
            entry: "start".to_string(),
            steps: vec![crate::contracts::ContractFlowStep {
                id: "start".to_string(),
                step_type: Some("connector_action".to_string()),
                operation_id: None,
                execution_mode: "DETERMINISTIC".to_string(),
                next: json!({"default": {"terminal": "completed"}}),
                connector: Some(json!({
                    "name": "connector.http.generic.v1",
                    "endpoint_ref": "webhook_site_test",
                    "method": "POST",
                    "side_effect_intent": "external_network"
                })),
            }],
        });
        let err = validate_contract_run_flow(&contract).unwrap_err();
        assert_eq!(err.code, "contract_operation_id_missing");
    }

    #[test]
    fn validate_contract_run_flow_rejects_duplicate_step_ids() {
        let contract = admitted_contract_with_flow(ContractFlow {
            version: "1.0".to_string(),
            description: "x".to_string(),
            entry: "start".to_string(),
            steps: vec![
                crate::contracts::ContractFlowStep {
                    id: "start".to_string(),
                    step_type: Some("human_gate".to_string()),
                    operation_id: Some("op_gate".to_string()),
                    execution_mode: "HUMAN_GATE".to_string(),
                    next: json!({"approve": {"terminal": "completed"}}),
                    connector: None,
                },
                crate::contracts::ContractFlowStep {
                    id: "start".to_string(),
                    step_type: Some("terminal".to_string()),
                    operation_id: None,
                    execution_mode: "DETERMINISTIC".to_string(),
                    next: json!({"default": {"terminal": "completed"}}),
                    connector: None,
                },
            ],
        });
        let err = validate_contract_run_flow(&contract).unwrap_err();
        assert_eq!(err.code, "contract_duplicate_step_id");
    }

    #[test]
    fn resolve_flow_transition_rejects_missing_outcome() {
        let contract = admitted_contract_with_flow(ContractFlow {
            version: "1.0".to_string(),
            description: "x".to_string(),
            entry: "start".to_string(),
            steps: vec![crate::contracts::ContractFlowStep {
                id: "start".to_string(),
                step_type: Some("connector_action".to_string()),
                operation_id: Some("op_start".to_string()),
                execution_mode: "DETERMINISTIC".to_string(),
                next: json!({"approve": {"terminal": "completed"}}),
                connector: Some(json!({
                    "name": "connector.http.generic.v1",
                    "endpoint_ref": "webhook_site_test",
                    "method": "POST",
                    "side_effect_intent": "external_network"
                })),
            }],
        });
        let err = resolve_flow_transition(&contract, "start", ContractStepOutcome::Default).unwrap_err();
        assert_eq!(err.code, "transition_not_found");
    }
}
