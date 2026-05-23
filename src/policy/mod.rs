//! Policy admission skeleton for MOVA Agent API V0.
//!
//! Phase 2 scope:
//! - deterministic admission structures
//! - decision shapes
//! - no orchestration and no runtime policy authority logic

use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::request::AuthContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionDecision {
    Allow,
    Deny,
    RequireReview,
    Redact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySummary {
    pub decision: AdmissionDecision,
    pub policy_version: String,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyAdmission {
    pub admission_id: String,
    pub action_id: String,
    pub decision: AdmissionDecision,
    pub reason_code: String,
    pub policy_version: String,
    pub constraints: Value,
    pub allowed_scopes: Vec<String>,
    pub blocked_scopes: Vec<String>,
    pub auth_context: AuthContext,
}

impl PolicyAdmission {
    pub fn new(
        admission_id: String,
        action_id: String,
        decision: AdmissionDecision,
        reason_code: String,
        policy_version: String,
    ) -> Self {
        Self {
            admission_id,
            action_id,
            decision,
            reason_code,
            policy_version,
            constraints: Value::Object(Default::default()),
            allowed_scopes: Vec::new(),
            blocked_scopes: Vec::new(),
            auth_context: placeholder_auth_context(None),
        }
    }

    pub fn from_auth_context(
        admission_id: String,
        action_id: String,
        policy_version: String,
        auth_context: Option<AuthContext>,
    ) -> Self {
        let normalized = placeholder_auth_context(auth_context);
        let reason_code = if normalized.source.as_deref() == Some("header") {
            "ok_auth_placeholder_header"
        } else if normalized.source.as_deref() == Some("request") {
            "ok_auth_placeholder_request"
        } else {
            "ok_auth_placeholder_none"
        };

        Self {
            admission_id,
            action_id,
            decision: AdmissionDecision::Allow,
            reason_code: reason_code.to_string(),
            policy_version,
            constraints: serde_json::json!({
                "auth_context": normalized.clone()
            }),
            allowed_scopes: normalized.scopes.clone(),
            blocked_scopes: Vec::new(),
            auth_context: normalized,
        }
    }

    pub fn to_summary(&self) -> PolicySummary {
        PolicySummary {
            decision: self.decision,
            policy_version: self.policy_version.clone(),
            reason_code: self.reason_code.clone(),
        }
    }
}

fn placeholder_auth_context(auth_context: Option<AuthContext>) -> AuthContext {
    match auth_context {
        Some(context) => AuthContext {
            mode: if context.mode.trim().is_empty() {
                "placeholder".to_string()
            } else {
                context.mode
            },
            actor_id: context.actor_id,
            token_ref: context.token_ref,
            scopes: context.scopes,
            source: context.source,
            verified: false,
        },
        None => AuthContext {
            mode: "placeholder".to_string(),
            actor_id: None,
            token_ref: None,
            scopes: Vec::new(),
            source: None,
            verified: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_admission_new_sets_default_shape() {
        let admission = PolicyAdmission::new(
            "adm_01".to_string(),
            "act_01".to_string(),
            AdmissionDecision::Allow,
            "ok".to_string(),
            "policy.default.v0".to_string(),
        );

        assert_eq!(admission.admission_id, "adm_01");
        assert_eq!(admission.action_id, "act_01");
        assert_eq!(admission.decision, AdmissionDecision::Allow);
        assert_eq!(admission.reason_code, "ok");
        assert_eq!(admission.policy_version, "policy.default.v0");
        assert_eq!(admission.allowed_scopes, Vec::<String>::new());
        assert_eq!(admission.blocked_scopes, Vec::<String>::new());
        assert_eq!(admission.constraints, Value::Object(Default::default()));
        assert_eq!(admission.auth_context.mode, "placeholder");
        assert!(!admission.auth_context.verified);
    }

    #[test]
    fn policy_admission_summary_maps_fields() {
        let admission = PolicyAdmission {
            admission_id: "adm_02".to_string(),
            action_id: "act_02".to_string(),
            decision: AdmissionDecision::RequireReview,
            reason_code: "review_required".to_string(),
            policy_version: "policy.default.v0".to_string(),
            constraints: serde_json::json!({ "max_tokens": 1024 }),
            allowed_scopes: vec!["connector.docs.v1".to_string()],
            blocked_scopes: vec!["connector.ext.v1".to_string()],
            auth_context: AuthContext {
                mode: "placeholder".to_string(),
                actor_id: Some("agent_001".to_string()),
                token_ref: Some("token:ref:01".to_string()),
                scopes: vec!["actions.run".to_string()],
                source: Some("request".to_string()),
                verified: false,
            },
        };

        let summary = admission.to_summary();
        assert_eq!(summary.decision, AdmissionDecision::RequireReview);
        assert_eq!(summary.policy_version, "policy.default.v0");
        assert_eq!(summary.reason_code, "review_required");
    }

    #[test]
    fn admission_decision_serialization_shape_is_stable() {
        let serialized = serde_json::to_string(&AdmissionDecision::RequireReview).unwrap();
        assert_eq!(serialized, "\"require_review\"");
    }

    #[test]
    fn policy_admission_from_auth_context_normalizes_to_placeholder_mode() {
        let admission = PolicyAdmission::from_auth_context(
            "adm_03".to_string(),
            "act_03".to_string(),
            "policy.default.v0".to_string(),
            Some(AuthContext {
                mode: "".to_string(),
                actor_id: Some("agent_001".to_string()),
                token_ref: Some("token:ref:02".to_string()),
                scopes: vec!["actions.validate".to_string()],
                source: Some("header".to_string()),
                verified: true,
            }),
        );

        assert_eq!(admission.auth_context.mode, "placeholder");
        assert!(!admission.auth_context.verified);
        assert_eq!(admission.reason_code, "ok_auth_placeholder_header");
        assert_eq!(admission.allowed_scopes, vec!["actions.validate"]);
    }
}
