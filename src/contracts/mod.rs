//! Local contract fixture registration/loading boundary for MOVA Agent API V0.
//!
//! This module keeps contract package loading explicit and local-only for
//! deterministic tests and proofs.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractFlowStep {
    pub id: String,
    pub execution_mode: String,
    pub next: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractFlow {
    pub version: String,
    pub description: String,
    pub entry: String,
    pub steps: Vec<ContractFlowStep>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalContractPackage {
    pub contract_id: String,
    pub flow: ContractFlow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractRegistryError {
    pub code: String,
    pub message: String,
}

impl ContractRegistryError {
    fn new(code: &str, message: String) -> Self {
        Self {
            code: code.to_string(),
            message,
        }
    }
}

#[derive(Debug, Default)]
pub struct LocalContractRegistry;

impl LocalContractRegistry {
    pub fn load_fixture_dir(
        &self,
        contract_id: &str,
        fixture_dir: &Path,
    ) -> Result<LocalContractPackage, ContractRegistryError> {
        let flow_path = fixture_dir.join("flow.json");
        let content = fs::read_to_string(&flow_path).map_err(|err| {
            ContractRegistryError::new(
                "contract_fixture_read_failed",
                format!("failed to read {}: {err}", flow_path.display()),
            )
        })?;
        let flow: ContractFlow = serde_json::from_str(&content).map_err(|err| {
            ContractRegistryError::new(
                "contract_fixture_parse_failed",
                format!("failed to parse {}: {err}", flow_path.display()),
            )
        })?;
        if flow.steps.is_empty() {
            return Err(ContractRegistryError::new(
                "contract_fixture_invalid",
                "flow.steps must not be empty".to_string(),
            ));
        }
        Ok(LocalContractPackage {
            contract_id: contract_id.to_string(),
            flow,
        })
    }
}

pub fn map_contract_to_agent_request(
    contract: &LocalContractPackage,
    request_id: &str,
    actor_id: &str,
) -> Value {
    let step_count = contract.flow.steps.len();
    json!({
        "request_id": request_id,
        "actor": {"actor_type": "ai_agent", "actor_id": actor_id},
        "source": {"channel": "api", "client_id": "contract_local_fixture"},
        "action": {
            "action_id": format!("act_{}", request_id),
            "action_type": "run_contract_package",
            "target_kind": "contract_package",
            "input_payload": {
                "contract_id": contract.contract_id,
                "entry": contract.flow.entry,
                "step_count": step_count
            },
            "policy_context": {"policy_profile_ref": "policy.default.v0"},
            "connector_context": {
                "connector_set": ["connector.docs.v1"],
                "connector_id": "connector.docs.v1",
                "side_effect_intent": "none"
            },
            "trace_ref": format!("trace:{}", request_id)
        },
        "inputs": {
            "contract_id": contract.contract_id,
            "entry": contract.flow.entry
        },
        "context": {
            "contract_flow_version": contract.flow.version
        },
        "correlation": {"trace_id": format!("corr_{}", request_id)},
        "timestamps": {"requested_at": "2026-05-23T10:30:00Z"}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_contract_to_agent_request_uses_contract_identity() {
        let contract = LocalContractPackage {
            contract_id: "operator_terminal_smoke_v0".to_string(),
            flow: ContractFlow {
                version: "1.0".to_string(),
                description: "deterministic flow".to_string(),
                entry: "start".to_string(),
                steps: vec![ContractFlowStep {
                    id: "start".to_string(),
                    execution_mode: "DETERMINISTIC".to_string(),
                    next: json!({"default": {"terminal": "completed"}}),
                }],
            },
        };
        let request = map_contract_to_agent_request(&contract, "req_01", "agent_01");
        assert_eq!(
            request["action"]["input_payload"]["contract_id"],
            "operator_terminal_smoke_v0"
        );
        assert_eq!(request["action"]["target_kind"], "contract_package");
    }
}
