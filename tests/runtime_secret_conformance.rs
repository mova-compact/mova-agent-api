use mova_agent_api::runtime::{RuntimeConfig, RuntimeConfigLoader, StaticRuntimeConfigLoader};
use mova_agent_api::secrets::{redact_json, SecretRef, SecretRefKind};

#[test]
fn runtime_config_loader_returns_valid_config() {
    let cfg = RuntimeConfig::deterministic_local_default();
    let loader = StaticRuntimeConfigLoader::new(cfg.clone());
    let loaded = loader.load().unwrap();
    assert_eq!(loaded.storage.adapter_kind, "in_memory");
}

#[test]
fn runtime_config_loader_fails_invalid_config() {
    let mut cfg = RuntimeConfig::deterministic_local_default();
    cfg.connectors.adapter_kind = "unsupported".to_string();
    let loader = StaticRuntimeConfigLoader::new(cfg);
    let err = loader.load().unwrap_err();
    assert_eq!(err.code, "connector_config_invalid");
}

#[test]
fn secret_ref_contract_validation() {
    let valid = SecretRef {
        kind: SecretRefKind::EnvRef,
        reference: "env://MOVA_CONNECTOR_TOKEN".to_string(),
    };
    assert!(valid.validate().is_ok());
    let invalid = SecretRef {
        kind: SecretRefKind::EnvRef,
        reference: "".to_string(),
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn secret_redaction_removes_sensitive_fields() {
    let value = serde_json::json!({
        "safe":"x",
        "runtime_secret":"raw",
        "nested":{"credential":"raw2","ok":true}
    });
    let redacted = redact_json(&value);
    assert_eq!(redacted["runtime_secret"], "[REDACTED]");
    assert_eq!(redacted["nested"]["credential"], "[REDACTED]");
    assert_eq!(redacted["safe"], "x");
}
