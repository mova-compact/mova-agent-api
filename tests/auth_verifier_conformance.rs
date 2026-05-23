use mova_agent_api::auth::{
    create_auth_verifier, AuthTrustConfig, AuthVerificationStatus, OfflineStubFixture,
};
use mova_agent_api::request::AuthContext;

fn production_context(token_ref: Option<&str>, scopes: Vec<&str>) -> AuthContext {
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
fn deterministic_verifier_conformance_verified() {
    let config = AuthTrustConfig::default_v0();
    let verifier = create_auth_verifier(&config);
    let result = verifier.verify(&production_context(
        Some("token://mova-trusted/agent_001?aud=mova-agent-api"),
        vec!["actions.run", "scope.unknown"],
    ));
    assert_eq!(result.status, AuthVerificationStatus::Verified);
    assert_eq!(result.reason_code, "auth_verified");
    assert_eq!(result.issuer.as_deref(), Some("mova-trusted"));
    assert_eq!(result.audience.as_deref(), Some("mova-agent-api"));
    assert_eq!(result.scopes, vec!["actions.run"]);
}

#[test]
fn deterministic_verifier_conformance_invalid_issuer() {
    let config = AuthTrustConfig::default_v0();
    let verifier = create_auth_verifier(&config);
    let result = verifier.verify(&production_context(
        Some("token://untrusted/agent_001?aud=mova-agent-api"),
        vec!["actions.run"],
    ));
    assert_eq!(result.status, AuthVerificationStatus::Unverified);
    assert_eq!(result.reason_code, "auth_issuer_untrusted");
}

#[test]
fn deterministic_verifier_conformance_invalid_audience() {
    let config = AuthTrustConfig::default_v0();
    let verifier = create_auth_verifier(&config);
    let result = verifier.verify(&production_context(
        Some("token://mova-trusted/agent_001?aud=wrong"),
        vec!["actions.run"],
    ));
    assert_eq!(result.status, AuthVerificationStatus::Unverified);
    assert_eq!(result.reason_code, "auth_audience_untrusted");
}

#[test]
fn deterministic_verifier_conformance_invalid_config_path() {
    let config = AuthTrustConfig {
        verifier_kind: "unsupported".to_string(),
        trusted_issuers: vec!["mova-trusted".to_string()],
        trusted_audiences: vec!["mova-agent-api".to_string()],
        allowed_scopes: vec!["actions.run".to_string()],
        offline_stub_fixtures: Vec::new(),
    };
    let verifier = create_auth_verifier(&config);
    let result = verifier.verify(&production_context(
        Some("token://mova-trusted/agent_001?aud=mova-agent-api"),
        vec!["actions.run"],
    ));
    assert_eq!(result.status, AuthVerificationStatus::Unverified);
    assert!(result.reason_code.starts_with("auth_verifier_config_invalid:"));
}

#[test]
fn offline_stub_verifier_conformance_verified_fixture() {
    let config = AuthTrustConfig {
        verifier_kind: "offline_provider_stub".to_string(),
        trusted_issuers: vec![],
        trusted_audiences: vec![],
        allowed_scopes: vec![],
        offline_stub_fixtures: vec![OfflineStubFixture {
            token_ref: "token://stub/provider?aud=mova-agent-api".to_string(),
            status: AuthVerificationStatus::Verified,
            reason_code: "auth_stub_verified".to_string(),
            issuer: Some("stub".to_string()),
            subject: Some("provider".to_string()),
            audience: Some("mova-agent-api".to_string()),
            scopes: vec!["actions.run".to_string()],
        }],
    };
    let verifier = create_auth_verifier(&config);
    let result = verifier.verify(&production_context(
        Some("token://stub/provider?aud=mova-agent-api"),
        vec![],
    ));
    assert_eq!(result.status, AuthVerificationStatus::Verified);
    assert_eq!(result.reason_code, "auth_stub_verified");
}

#[test]
fn offline_stub_verifier_conformance_missing_fixture() {
    let config = AuthTrustConfig {
        verifier_kind: "offline_provider_stub".to_string(),
        trusted_issuers: vec![],
        trusted_audiences: vec![],
        allowed_scopes: vec![],
        offline_stub_fixtures: vec![OfflineStubFixture {
            token_ref: "token://stub/provider?aud=mova-agent-api".to_string(),
            status: AuthVerificationStatus::Verified,
            reason_code: "auth_stub_verified".to_string(),
            issuer: Some("stub".to_string()),
            subject: Some("provider".to_string()),
            audience: Some("mova-agent-api".to_string()),
            scopes: vec!["actions.run".to_string()],
        }],
    };
    let verifier = create_auth_verifier(&config);
    let result = verifier.verify(&production_context(
        Some("token://stub/missing?aud=mova-agent-api"),
        vec![],
    ));
    assert_eq!(result.status, AuthVerificationStatus::Unverified);
    assert_eq!(result.reason_code, "auth_stub_fixture_not_found");
}
