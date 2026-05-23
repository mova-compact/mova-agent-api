//! Request boundary skeleton for MOVA Agent API V0.
//!
//! Phase 6 extension:
//! - minimal request envelope types for vertical smoke

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub actor_type: String,
    pub actor_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub channel: String,
    pub client_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthContext {
    pub mode: String,
    pub actor_id: Option<String>,
    pub token_ref: Option<String>,
    pub scopes: Vec<String>,
    pub source: Option<String>,
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub action_id: String,
    pub action_type: String,
    pub target_kind: String,
    pub input_ref: Option<String>,
    pub input_payload: Option<Value>,
    pub policy_context: Value,
    pub connector_context: Value,
    pub trace_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestEnvelope {
    pub request_id: String,
    pub actor: Actor,
    pub source: Source,
    pub auth_context: Option<AuthContext>,
    pub action: Action,
    pub inputs: Value,
    pub context: Value,
    pub correlation: Value,
    pub timestamps: Value,
}

pub fn parse_request_envelope(value: Value) -> Result<RequestEnvelope, serde_json::Error> {
    serde_json::from_value(value)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestValidationError {
    pub field: String,
    pub message: String,
}

pub fn validate_request_envelope(envelope: &RequestEnvelope) -> Vec<RequestValidationError> {
    let mut errors = Vec::new();

    if envelope.request_id.trim().is_empty() {
        errors.push(RequestValidationError {
            field: "request_id".to_string(),
            message: "must not be empty".to_string(),
        });
    }
    if envelope.actor.actor_type.trim().is_empty() {
        errors.push(RequestValidationError {
            field: "actor.actor_type".to_string(),
            message: "must not be empty".to_string(),
        });
    }
    if envelope.actor.actor_id.trim().is_empty() {
        errors.push(RequestValidationError {
            field: "actor.actor_id".to_string(),
            message: "must not be empty".to_string(),
        });
    }
    if envelope.source.channel.trim().is_empty() {
        errors.push(RequestValidationError {
            field: "source.channel".to_string(),
            message: "must not be empty".to_string(),
        });
    }
    if envelope.source.client_id.trim().is_empty() {
        errors.push(RequestValidationError {
            field: "source.client_id".to_string(),
            message: "must not be empty".to_string(),
        });
    }
    if let Some(auth_context) = &envelope.auth_context {
        if auth_context.mode.trim().is_empty() {
            errors.push(RequestValidationError {
                field: "auth_context.mode".to_string(),
                message: "must not be empty".to_string(),
            });
        }
        if auth_context.verified {
            errors.push(RequestValidationError {
                field: "auth_context.verified".to_string(),
                message: "must be false in V0 placeholder auth mode".to_string(),
            });
        }
    }
    if envelope.action.action_id.trim().is_empty() {
        errors.push(RequestValidationError {
            field: "action.action_id".to_string(),
            message: "must not be empty".to_string(),
        });
    }
    if envelope.action.trace_ref.trim().is_empty() {
        errors.push(RequestValidationError {
            field: "action.trace_ref".to_string(),
            message: "must not be empty".to_string(),
        });
    }

    let has_input_ref = envelope
        .action
        .input_ref
        .as_ref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let has_input_payload = envelope.action.input_payload.is_some();

    if has_input_ref == has_input_payload {
        errors.push(RequestValidationError {
            field: "action".to_string(),
            message: "exactly one of input_ref or input_payload is required".to_string(),
        });
    }

    if !envelope.inputs.is_object() {
        errors.push(RequestValidationError {
            field: "inputs".to_string(),
            message: "must be an object".to_string(),
        });
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_request_envelope_parses_minimal_shape() {
        let value = serde_json::json!({
            "request_id": "req_01",
            "actor": {"actor_type": "ai_agent", "actor_id": "agent_001"},
            "source": {"channel": "api", "client_id": "client_001"},
            "auth_context": {
                "mode": "placeholder",
                "actor_id": "agent_001",
                "token_ref": "token:ref:01",
                "scopes": ["actions.run"],
                "source": "header",
                "verified": false
            },
            "action": {
                "action_id": "act_01",
                "action_type": "validate_document",
                "target_kind": "document",
                "input_payload": {"document_id": "doc_123"},
                "policy_context": {"policy_profile_ref": "policy.default.v0"},
                "connector_context": {"connector_set": ["connector.docs.v1"]},
                "trace_ref": "trace:req_01"
            },
            "inputs": {"document_id": "doc_123"},
            "context": {"tenant_id": "tenant_001"},
            "correlation": {"trace_id": "trace_abc123"},
            "timestamps": {"requested_at": "2026-05-23T08:30:00Z"}
        });

        let envelope = parse_request_envelope(value).unwrap();
        assert_eq!(envelope.request_id, "req_01");
        assert_eq!(envelope.action.action_id, "act_01");
        assert_eq!(envelope.action.trace_ref, "trace:req_01");
        assert_eq!(envelope.auth_context.as_ref().unwrap().mode, "placeholder");
    }

    #[test]
    fn validate_request_envelope_accepts_minimal_shape() {
        let value = serde_json::json!({
            "request_id": "req_01",
            "actor": {"actor_type": "ai_agent", "actor_id": "agent_001"},
            "source": {"channel": "api", "client_id": "client_001"},
            "auth_context": {
                "mode": "placeholder",
                "actor_id": "agent_001",
                "token_ref": "token:ref:01",
                "scopes": ["actions.run"],
                "source": "header",
                "verified": false
            },
            "action": {
                "action_id": "act_01",
                "action_type": "validate_document",
                "target_kind": "document",
                "input_payload": {"document_id": "doc_123"},
                "policy_context": {"policy_profile_ref": "policy.default.v0"},
                "connector_context": {"connector_set": ["connector.docs.v1"]},
                "trace_ref": "trace:req_01"
            },
            "inputs": {"document_id": "doc_123"},
            "context": {"tenant_id": "tenant_001"},
            "correlation": {"trace_id": "trace_abc123"},
            "timestamps": {"requested_at": "2026-05-23T08:30:00Z"}
        });
        let envelope = parse_request_envelope(value).unwrap();
        let errors = validate_request_envelope(&envelope);
        assert!(errors.is_empty());
    }

    #[test]
    fn validate_request_envelope_rejects_missing_input_choice() {
        let value = serde_json::json!({
            "request_id": "req_01",
            "actor": {"actor_type": "ai_agent", "actor_id": "agent_001"},
            "source": {"channel": "api", "client_id": "client_001"},
            "action": {
                "action_id": "act_01",
                "action_type": "validate_document",
                "target_kind": "document",
                "policy_context": {"policy_profile_ref": "policy.default.v0"},
                "connector_context": {"connector_set": ["connector.docs.v1"]},
                "trace_ref": "trace:req_01"
            },
            "inputs": {"document_id": "doc_123"},
            "context": {"tenant_id": "tenant_001"},
            "correlation": {"trace_id": "trace_abc123"},
            "timestamps": {"requested_at": "2026-05-23T08:30:00Z"}
        });
        let envelope = parse_request_envelope(value).unwrap();
        let errors = validate_request_envelope(&envelope);
        assert!(errors.iter().any(|e| e.field == "action"));
    }

    #[test]
    fn validate_request_envelope_rejects_verified_true_for_v0() {
        let value = serde_json::json!({
            "request_id": "req_01",
            "actor": {"actor_type": "ai_agent", "actor_id": "agent_001"},
            "source": {"channel": "api", "client_id": "client_001"},
            "auth_context": {
                "mode": "placeholder",
                "actor_id": "agent_001",
                "token_ref": "token:ref:01",
                "scopes": ["actions.run"],
                "source": "header",
                "verified": true
            },
            "action": {
                "action_id": "act_01",
                "action_type": "validate_document",
                "target_kind": "document",
                "input_payload": {"document_id": "doc_123"},
                "policy_context": {"policy_profile_ref": "policy.default.v0"},
                "connector_context": {"connector_set": ["connector.docs.v1"]},
                "trace_ref": "trace:req_01"
            },
            "inputs": {"document_id": "doc_123"},
            "context": {"tenant_id": "tenant_001"},
            "correlation": {"trace_id": "trace_abc123"},
            "timestamps": {"requested_at": "2026-05-23T08:30:00Z"}
        });
        let envelope = parse_request_envelope(value).unwrap();
        let errors = validate_request_envelope(&envelope);
        assert!(errors.iter().any(|e| e.field == "auth_context.verified"));
    }
}
