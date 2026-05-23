//! Secret reference and redaction boundary for MOVA Agent API V0.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretRefKind {
    SecretRef,
    EnvRef,
    RuntimeSecret,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRef {
    pub kind: SecretRefKind,
    pub reference: String,
}

impl SecretRef {
    pub fn validate(&self) -> Result<(), SecretBoundaryError> {
        if self.reference.trim().is_empty() {
            return Err(SecretBoundaryError::new(
                "secret_ref_invalid",
                "reference must not be empty",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretBoundaryError {
    pub code: String,
    pub message: String,
}

impl SecretBoundaryError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
        }
    }
}

pub fn redact_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if is_secret_key(k) {
                    out.insert(k.clone(), Value::String("[REDACTED]".to_string()));
                } else {
                    out.insert(k.clone(), redact_json(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(redact_json).collect()),
        _ => value.clone(),
    }
}

pub fn redact_text(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    if lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("credential")
        || lower.contains("api_key")
    {
        "[REDACTED]".to_string()
    } else {
        input.to_string()
    }
}

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("credential")
        || lower.contains("api_key")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_json_redacts_secret_like_keys() {
        let value = serde_json::json!({
            "token_ref": "token://x/y",
            "nested": {"api_key": "raw", "safe":"ok"}
        });
        let redacted = redact_json(&value);
        assert_eq!(redacted["token_ref"], "[REDACTED]");
        assert_eq!(redacted["nested"]["api_key"], "[REDACTED]");
        assert_eq!(redacted["nested"]["safe"], "ok");
    }

    #[test]
    fn redact_text_redacts_secret_like_content() {
        assert_eq!(redact_text("token=abc"), "[REDACTED]");
        assert_eq!(redact_text("safe-message"), "safe-message");
    }
}
