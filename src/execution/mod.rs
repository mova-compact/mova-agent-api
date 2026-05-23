//! Flat execution skeleton for MOVA Agent API V0.
//!
//! Phase 3 scope:
//! - deterministic single-action execution structure
//! - no real execution side effects

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Accepted,
    Ready,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub step_id: String,
    pub step_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatExecutionPlan {
    pub run_id: String,
    pub action_id: String,
    pub state: ExecutionState,
    pub steps: Vec<ExecutionStep>,
}

impl FlatExecutionPlan {
    pub fn from_action(run_id: String, action_id: String) -> Self {
        let steps = vec![
            ExecutionStep {
                step_id: "step_policy_admission".to_string(),
                step_kind: "policy_admission".to_string(),
            },
            ExecutionStep {
                step_id: "step_connector_call".to_string(),
                step_kind: "connector_call".to_string(),
            },
            ExecutionStep {
                step_id: "step_observation_write".to_string(),
                step_kind: "observation_write".to_string(),
            },
            ExecutionStep {
                step_id: "step_evidence_response".to_string(),
                step_kind: "evidence_response".to_string(),
            },
        ];

        Self {
            run_id,
            action_id,
            state: ExecutionState::Ready,
            steps,
        }
    }

    pub fn complete(self) -> Self {
        Self {
            state: ExecutionState::Completed,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_plan_from_action_is_single_path() {
        let plan = FlatExecutionPlan::from_action("run_01".to_string(), "act_01".to_string());

        assert_eq!(plan.run_id, "run_01");
        assert_eq!(plan.action_id, "act_01");
        assert_eq!(plan.state, ExecutionState::Ready);
        assert_eq!(plan.steps.len(), 4);
        assert_eq!(plan.steps[0].step_kind, "policy_admission");
        assert_eq!(plan.steps[1].step_kind, "connector_call");
        assert_eq!(plan.steps[2].step_kind, "observation_write");
        assert_eq!(plan.steps[3].step_kind, "evidence_response");
    }

    #[test]
    fn flat_plan_complete_transitions_state() {
        let plan = FlatExecutionPlan::from_action("run_02".to_string(), "act_02".to_string());
        let completed = plan.complete();
        assert_eq!(completed.state, ExecutionState::Completed);
    }
}

