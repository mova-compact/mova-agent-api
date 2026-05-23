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
    pub audience: Option<String>,
    pub scopes: Vec<String>,
}

impl AuthVerificationResult {
    pub fn verified(issuer: String, subject: String, audience: Option<String>, scopes: Vec<String>) -> Self {
        Self {
            status: AuthVerificationStatus::Verified,
            reason_code: "auth_verified".to_string(),
            issuer: Some(issuer),
            subject: Some(subject),
            audience,
            scopes,
        }
    }

    pub fn unverified(reason_code: &str) -> Self {
        Self {
            status: AuthVerificationStatus::Unverified,
            reason_code: reason_code.to_string(),
            issuer: None,
            subject: None,
            audience: None,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthTrustConfig {
    pub verifier_kind: String,
    pub trusted_issuers: Vec<String>,
    pub trusted_audiences: Vec<String>,
    pub allowed_scopes: Vec<String>,
}

impl AuthTrustConfig {
    pub fn default_v0() -> Self {
        Self {
            verifier_kind: "deterministic_local".to_string(),
            trusted_issuers: vec!["mova-trusted".to_string()],
            trusted_audiences: vec!["mova-agent-api".to_string()],
            allowed_scopes: vec!["actions.run".to_string(), "actions.validate".to_string()],
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.verifier_kind.trim().is_empty() {
            return Err("verifier_kind_missing".to_string());
        }
        if self.verifier_kind != "deterministic_local" {
            return Err("verifier_kind_unsupported".to_string());
        }
        if self.trusted_issuers.is_empty() {
            return Err("trusted_issuers_empty".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DeterministicAuthVerifier {
    config: AuthTrustConfig,
}

impl DeterministicAuthVerifier {
    pub fn new(config: AuthTrustConfig) -> Self {
        Self { config }
    }

    pub fn default_v0() -> Self {
        Self::new(AuthTrustConfig::default_v0())
    }
}

impl AuthVerifier for DeterministicAuthVerifier {
    fn verify(&self, auth_context: &AuthContext) -> AuthVerificationResult {
        let token_ref = match &auth_context.token_ref {
            Some(value) if !value.trim().is_empty() => value,
            _ => return AuthVerificationResult::unverified("auth_token_missing"),
        };

        let parsed = parse_token_ref(token_ref);
        let (issuer, subject, audience) = match parsed {
            Some(value) => value,
            None => return AuthVerificationResult::unverified("auth_token_malformed"),
        };

        if !self.config.trusted_issuers.iter().any(|trusted| trusted == &issuer) {
            return AuthVerificationResult::unverified("auth_issuer_untrusted");
        }

        if !self.config.trusted_audiences.is_empty() {
            let Some(aud) = audience.clone() else {
                return AuthVerificationResult::unverified("auth_audience_missing");
            };
            if !self.config.trusted_audiences.iter().any(|trusted| trusted == &aud) {
                return AuthVerificationResult::unverified("auth_audience_untrusted");
            }
        }

        let filtered_scopes = if self.config.allowed_scopes.is_empty() {
            auth_context.scopes.clone()
        } else {
            auth_context
                .scopes
                .iter()
                .filter(|scope| self.config.allowed_scopes.iter().any(|allowed| allowed == *scope))
                .cloned()
                .collect::<Vec<_>>()
        };

        AuthVerificationResult::verified(issuer, subject, audience, filtered_scopes)
    }
}

#[derive(Debug, Clone)]
struct AlwaysUnverifiedAuthVerifier {
    reason_code: String,
}

impl AuthVerifier for AlwaysUnverifiedAuthVerifier {
    fn verify(&self, _auth_context: &AuthContext) -> AuthVerificationResult {
        AuthVerificationResult::unverified(&self.reason_code)
    }
}

pub fn create_auth_verifier(config: &AuthTrustConfig) -> Box<dyn AuthVerifier> {
    match config.validate() {
        Ok(()) => Box::new(DeterministicAuthVerifier::new(config.clone())),
        Err(err) => Box::new(AlwaysUnverifiedAuthVerifier {
            reason_code: format!("auth_verifier_config_invalid:{err}"),
        }),
    }
}

fn parse_token_ref(token_ref: &str) -> Option<(String, String, Option<String>)> {
    // token_ref contract for V0 deterministic verification:
    // token://<issuer>/<subject>?aud=<audience>
    let value = token_ref.strip_prefix("token://")?;
    let mut query_split = value.splitn(2, '?');
    let principal = query_split.next()?;
    let query = query_split.next();

    let mut parts = principal.splitn(2, '/');
    let issuer = parts.next()?.trim();
    let subject = parts.next()?.trim();
    if issuer.is_empty() || subject.is_empty() {
        return None;
    }
    let audience = query
        .and_then(|q| q.strip_prefix("aud="))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    Some((issuer.to_string(), subject.to_string(), audience))
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
            Some("token://mova-trusted/agent_001?aud=mova-agent-api"),
            vec!["actions.run"],
        ));
        assert!(result.is_verified());
        assert_eq!(result.issuer.as_deref(), Some("mova-trusted"));
        assert_eq!(result.subject.as_deref(), Some("agent_001"));
        assert_eq!(result.audience.as_deref(), Some("mova-agent-api"));
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

    #[test]
    fn verifier_rejects_missing_audience_when_trust_config_requires_it() {
        let verifier = DeterministicAuthVerifier::default_v0();
        let result = verifier.verify(&production_auth_context(
            Some("token://mova-trusted/agent_001"),
            vec!["actions.run"],
        ));
        assert!(!result.is_verified());
        assert_eq!(result.reason_code, "auth_audience_missing");
    }

    #[test]
    fn verifier_filters_scopes_using_allowed_scope_config() {
        let verifier = DeterministicAuthVerifier::default_v0();
        let result = verifier.verify(&production_auth_context(
            Some("token://mova-trusted/agent_001?aud=mova-agent-api"),
            vec!["actions.run", "scope.unknown"],
        ));
        assert!(result.is_verified());
        assert_eq!(result.scopes, vec!["actions.run"]);
    }

    #[test]
    fn verifier_factory_returns_explicit_unverified_adapter_on_invalid_config() {
        let config = AuthTrustConfig {
            verifier_kind: "unsupported".to_string(),
            trusted_issuers: vec!["mova-trusted".to_string()],
            trusted_audiences: vec!["mova-agent-api".to_string()],
            allowed_scopes: vec!["actions.run".to_string()],
        };
        let verifier = create_auth_verifier(&config);
        let result = verifier.verify(&production_auth_context(
            Some("token://mova-trusted/agent_001?aud=mova-agent-api"),
            vec!["actions.run"],
        ));
        assert!(!result.is_verified());
        assert!(result.reason_code.starts_with("auth_verifier_config_invalid:"));
    }
}
