//! Auth verification contract layer for MOVA Agent API V0.
//!
//! This module defines production-auth verification semantics without
//! introducing external provider coupling or durable auth state.

use crate::request::AuthContext;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthVerificationStatus {
    Verified,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthVerificationResult {
    pub status: AuthVerificationStatus,
    pub reason_code: String,
    pub issuer: Option<String>,
    pub subject: Option<String>,
    pub scopes: Vec<String>,
}

impl AuthVerificationResult {
    pub fn verified(issuer: String, subject: String, scopes: Vec<String>) -> Self {
        Self {
            status: AuthVerificationStatus::Verified,
            reason_code: "auth_verified".to_string(),
            issuer: Some(issuer),
            subject: Some(subject),
            scopes,
        }
    }

    pub fn unverified(reason_code: &str) -> Self {
        Self {
            status: AuthVerificationStatus::Unverified,
            reason_code: reason_code.to_string(),
            issuer: None,
            subject: None,
            scopes: Vec::new(),
        }
    }

    pub fn is_verified(&self) -> bool {
        self.status == AuthVerificationStatus::Verified
    }
}

pub trait AuthVerifier: Send + Sync {
    fn verify(&self, auth_context: &AuthContext) -> AuthVerificationResult;
}

#[derive(Debug, Clone)]
pub struct DeterministicAuthVerifier {
    trusted_issuers: Vec<String>,
}

impl DeterministicAuthVerifier {
    pub fn new(trusted_issuers: Vec<String>) -> Self {
        Self { trusted_issuers }
    }

    pub fn default_v0() -> Self {
        Self::new(vec!["mova-trusted".to_string()])
    }
}

impl AuthVerifier for DeterministicAuthVerifier {
    fn verify(&self, auth_context: &AuthContext) -> AuthVerificationResult {
        let token_ref = match &auth_context.token_ref {
            Some(value) if !value.trim().is_empty() => value,
            _ => return AuthVerificationResult::unverified("auth_token_missing"),
        };

        let parsed = parse_token_ref(token_ref);
        let (issuer, subject) = match parsed {
            Some(value) => value,
            None => return AuthVerificationResult::unverified("auth_token_malformed"),
        };

        if !self.trusted_issuers.iter().any(|trusted| trusted == &issuer) {
            return AuthVerificationResult::unverified("auth_issuer_untrusted");
        }

        AuthVerificationResult::verified(issuer, subject, auth_context.scopes.clone())
    }
}

fn parse_token_ref(token_ref: &str) -> Option<(String, String)> {
    // token_ref contract for V0 deterministic verification:
    // token://<issuer>/<subject>
    let value = token_ref.strip_prefix("token://")?;
    let mut parts = value.splitn(2, '/');
    let issuer = parts.next()?.trim();
    let subject = parts.next()?.trim();
    if issuer.is_empty() || subject.is_empty() {
        return None;
    }
    Some((issuer.to_string(), subject.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn production_auth_context(token_ref: Option<&str>, scopes: Vec<&str>) -> AuthContext {
        AuthContext {
            mode: "production".to_string(),
            actor_id: Some("agent_001".to_string()),
            token_ref: token_ref.map(|v| v.to_string()),
            scopes: scopes.into_iter().map(|v| v.to_string()).collect(),
            source: Some("header".to_string()),
            verified: false,
        }
    }

    #[test]
    fn verifier_marks_trusted_token_as_verified() {
        let verifier = DeterministicAuthVerifier::default_v0();
        let result = verifier.verify(&production_auth_context(
            Some("token://mova-trusted/agent_001"),
            vec!["actions.run"],
        ));
        assert!(result.is_verified());
        assert_eq!(result.issuer.as_deref(), Some("mova-trusted"));
        assert_eq!(result.subject.as_deref(), Some("agent_001"));
    }

    #[test]
    fn verifier_rejects_untrusted_issuer() {
        let verifier = DeterministicAuthVerifier::default_v0();
        let result = verifier.verify(&production_auth_context(
            Some("token://untrusted/agent_001"),
            vec!["actions.run"],
        ));
        assert!(!result.is_verified());
        assert_eq!(result.reason_code, "auth_issuer_untrusted");
    }
}
