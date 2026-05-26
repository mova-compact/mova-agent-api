//! Local contract loading and minimal admission bridge for MOVA Agent API V0.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractFlowStep {
    pub id: String,
    #[serde(default)]
    pub step_type: Option<String>,
    pub execution_mode: String,
    pub next: Value,
    #[serde(default)]
    pub connector: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractFlow {
    pub version: String,
    pub description: String,
    pub entry: String,
    pub steps: Vec<ContractFlowStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractConnectorRequirements {
    pub contract_id: String,
    pub connectors: Vec<ContractConnectorSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractConnectorSpec {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdmittedContract {
    pub contract_id: String,
    pub execution_type: String,
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub commit_sha: Option<String>,
    pub contract_path: Option<String>,
    pub registered_at: Option<String>,
    pub admitted: Option<bool>,
    pub manifest: Option<Value>,
    pub flow: ContractFlow,
    pub policy: Option<Value>,
    pub connector_requirements: Option<ContractConnectorRequirements>,
    pub evidence_expectations: Option<Value>,
    pub open_questions: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractRegistryError {
    pub code: String,
    pub message: String,
}

impl ContractRegistryError {
    pub fn new(code: &str, message: String) -> Self {
        Self {
            code: code.to_string(),
            message,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalContractPackage {
    pub contract_id: String,
    pub flow: ContractFlow,
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
        validate_flow_shape(&flow, &HashSet::new())?;
        Ok(LocalContractPackage {
            contract_id: contract_id.to_string(),
            flow,
        })
    }
}

pub fn validate_admitted_contract(contract: &AdmittedContract) -> Result<(), ContractRegistryError> {
    let mut declared_connectors = HashSet::new();
    if let Some(reqs) = &contract.connector_requirements {
        for connector in &reqs.connectors {
            declared_connectors.insert(connector.name.clone());
        }
    }
    validate_flow_shape(&contract.flow, &declared_connectors)
}

fn validate_flow_shape(
    flow: &ContractFlow,
    declared_connectors: &HashSet<String>,
) -> Result<(), ContractRegistryError> {
    if flow.entry.trim().is_empty() {
        return Err(ContractRegistryError::new(
            "contract_flow_invalid",
            "flow.entry must be a non-empty string".to_string(),
        ));
    }
    if flow.steps.is_empty() {
        return Err(ContractRegistryError::new(
            "contract_flow_invalid",
            "flow.steps must not be empty".to_string(),
        ));
    }

    let mut ids = HashSet::new();
    for step in &flow.steps {
        if step.id.trim().is_empty() {
            return Err(ContractRegistryError::new(
                "contract_flow_invalid",
                "step.id must not be empty".to_string(),
            ));
        }
        if !ids.insert(step.id.clone()) {
            return Err(ContractRegistryError::new(
                "contract_flow_invalid",
                format!("duplicate step id: {}", step.id),
            ));
        }
        if step.execution_mode.trim().is_empty() {
            return Err(ContractRegistryError::new(
                "contract_flow_invalid",
                format!("step {} missing execution_mode", step.id),
            ));
        }
        if let Some(conn) = &step.connector {
            if let Some(name) = conn.get("name").and_then(|v| v.as_str()) {
                if !declared_connectors.is_empty() && !declared_connectors.contains(name) {
                    return Err(ContractRegistryError::new(
                        "contract_connector_mismatch",
                        format!("step {} references undeclared connector {}", step.id, name),
                    ));
                }
            }
        }
    }

    if !ids.contains(&flow.entry) {
        return Err(ContractRegistryError::new(
            "contract_flow_invalid",
            format!("flow.entry '{}' does not match any step id", flow.entry),
        ));
    }

    for step in &flow.steps {
        let next_obj = step
            .next
            .as_object()
            .ok_or_else(|| {
                ContractRegistryError::new(
                    "contract_flow_invalid",
                    format!("step {} next must be an object", step.id),
                )
            })?;
        if next_obj.is_empty() {
            return Err(ContractRegistryError::new(
                "contract_flow_invalid",
                format!("step {} next must not be empty", step.id),
            ));
        }
        for target in next_obj.values() {
            let target_obj = target.as_object().ok_or_else(|| {
                ContractRegistryError::new(
                    "contract_flow_invalid",
                    format!("step {} next target must be an object", step.id),
                )
            })?;
            if let Some(step_ref) = target_obj.get("step").and_then(|v| v.as_str()) {
                if !ids.contains(step_ref) {
                    return Err(ContractRegistryError::new(
                        "contract_flow_invalid",
                        format!("step {} points to unknown step {}", step.id, step_ref),
                    ));
                }
            } else if target_obj.get("terminal").and_then(|v| v.as_str()).is_none() {
                return Err(ContractRegistryError::new(
                    "contract_flow_invalid",
                    format!("step {} target must contain step or terminal", step.id),
                ));
            }
        }
    }

    Ok(())
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

pub fn parse_inline_flow_json(value: &Value) -> Result<ContractFlow, ContractRegistryError> {
    serde_json::from_value(value.clone()).map_err(|err| {
        ContractRegistryError::new(
            "contract_flow_parse_failed",
            format!("failed to parse inline_flow_json: {err}"),
        )
    })
}

pub fn parse_contract_connector_requirements(value: Option<Value>) -> Result<Option<ContractConnectorRequirements>, ContractRegistryError> {
    match value {
        None => Ok(None),
        Some(v) => serde_json::from_value(v).map(Some).map_err(|err| {
            ContractRegistryError::new(
                "contract_connector_requirements_invalid",
                format!("failed to parse connector_requirements: {err}"),
            )
        }),
    }
}

pub fn flow_step_by_id<'a>(flow: &'a ContractFlow, step_id: &str) -> Option<&'a ContractFlowStep> {
    flow.steps.iter().find(|s| s.id == step_id)
}

pub fn pick_next_target(step: &ContractFlowStep, outcome: &str) -> Option<Value> {
    let next = step.next.as_object()?;
    if let Some(v) = next.get(outcome) {
        return Some(v.clone());
    }
    next.get("default").cloned()
}

pub fn extract_outcomes_map(input_payload: &Value) -> HashMap<String, String> {
    input_payload
        .get("outcomes")
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default()
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
                    step_type: None,
                    execution_mode: "DETERMINISTIC".to_string(),
                    next: json!({"default": {"terminal": "completed"}}),
                    connector: None,
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

    #[test]
    fn flow_validation_rejects_missing_step_ref() {
        let flow = ContractFlow {
            version: "1.0".to_string(),
            description: "x".to_string(),
            entry: "start".to_string(),
            steps: vec![ContractFlowStep {
                id: "start".to_string(),
                step_type: None,
                execution_mode: "DETERMINISTIC".to_string(),
                next: json!({"default": {"step": "missing"}}),
                connector: None,
            }],
        };
        let err = validate_flow_shape(&flow, &HashSet::new()).unwrap_err();
        assert_eq!(err.code, "contract_flow_invalid");
    }
}
