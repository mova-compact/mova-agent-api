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
    pub action: Action,
    pub inputs: Value,
    pub context: Value,
    pub correlation: Value,
    pub timestamps: Value,
}

pub fn parse_request_envelope(value: Value) -> Result<RequestEnvelope, serde_json::Error> {
    serde_json::from_value(value)
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
    }
}

