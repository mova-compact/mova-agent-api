use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::policy::AdmissionDecision;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationAdmission {
    pub admission_id: String,
    pub decision: AdmissionDecision,
    pub reason_code: String,
    pub policy_version: String,
    pub contract_id: String,
    pub run_id: String,
    pub step_id: String,
    pub operation_id: String,
    pub allowed_connector_id: Option<String>,
    pub allowed_endpoint_ref: Option<String>,
    pub allowed_method: Option<String>,
    pub constraints: Value,
    pub expires_at: Option<String>,
}
