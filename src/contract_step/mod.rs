use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractStepType {
    ConnectorAction,
    DeterministicAction,
    HumanGate,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractStepStatus {
    Pending,
    Ready,
    Running,
    Completed,
    WaitingReview,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractStep {
    pub step_id: String,
    pub step_type: ContractStepType,
    pub status: ContractStepStatus,
    pub operation_id: Option<String>,
    pub requires_review: bool,
}
