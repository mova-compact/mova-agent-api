use serde::{Deserialize, Serialize};

use crate::request::Actor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanGateStatus {
    WaitingReview,
    Resolved,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanGateDecision {
    Approve,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanGate {
    pub gate_id: String,
    pub run_id: String,
    pub step_id: String,
    pub status: HumanGateStatus,
    pub reason_code: String,
    pub prompt: String,
    pub requested_operation_id: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub decision: Option<HumanGateDecision>,
    pub decided_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanGateResolutionRequest {
    pub decision: HumanGateDecision,
    pub actor: Actor,
    pub reason: String,
    pub timestamps: serde_json::Value,
}
