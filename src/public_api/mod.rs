use crate::request::AuthContext;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicApiConfig {
    pub api_key: String,
    pub admin_api_key: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub client_id: String,
    pub token_issuer: String,
    pub token_audience: String,
    pub allowed_scopes: Vec<String>,
    pub admin_allowed_scopes: Vec<String>,
}

impl PublicApiConfig {
    pub fn deterministic_local_default() -> Self {
        Self {
            api_key: "mova-dev-key".to_string(),
            admin_api_key: "mova-admin-dev-key".to_string(),
            tenant_id: "tenant_server_owned".to_string(),
            actor_id: "agent_public_001".to_string(),
            client_id: "client_public_001".to_string(),
            token_issuer: "mova-trusted".to_string(),
            token_audience: "mova-agent-api".to_string(),
            allowed_scopes: vec!["contracts.run".to_string()],
            admin_allowed_scopes: vec!["contracts.register".to_string()],
        }
    }

    pub fn from_env_or_default() -> Self {
        let mut cfg = Self::deterministic_local_default();
        if let Ok(value) = std::env::var("MOVA_API_KEY") {
            cfg.api_key = value;
        }
        if let Ok(value) = std::env::var("MOVA_ADMIN_API_KEY") {
            cfg.admin_api_key = value;
        }
        if let Ok(value) = std::env::var("MOVA_SERVER_TENANT_ID") {
            cfg.tenant_id = value;
        }
        if let Ok(value) = std::env::var("MOVA_PUBLIC_ACTOR_ID") {
            cfg.actor_id = value;
        }
        if let Ok(value) = std::env::var("MOVA_PUBLIC_CLIENT_ID") {
            cfg.client_id = value;
        }
        if let Ok(value) = std::env::var("MOVA_PUBLIC_TOKEN_ISSUER") {
            cfg.token_issuer = value;
        }
        if let Ok(value) = std::env::var("MOVA_PUBLIC_TOKEN_AUDIENCE") {
            cfg.token_audience = value;
        }
        if let Ok(value) = std::env::var("MOVA_PUBLIC_ALLOWED_SCOPES") {
            let scopes = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            if !scopes.is_empty() {
                cfg.allowed_scopes = scopes;
            }
        }
        if let Ok(value) = std::env::var("MOVA_PUBLIC_ADMIN_ALLOWED_SCOPES") {
            let scopes = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            if !scopes.is_empty() {
                cfg.admin_allowed_scopes = scopes;
            }
        }
        cfg
    }

    pub fn auth_context(&self) -> AuthContext {
        AuthContext {
            mode: "production".to_string(),
            actor_id: Some(self.actor_id.clone()),
            token_ref: Some(format!(
                "token://{}/{}?aud={}",
                self.token_issuer, self.actor_id, self.token_audience
            )),
            scopes: self.allowed_scopes.clone(),
            source: Some("header".to_string()),
            verified: false,
        }
    }

    pub fn admin_auth_context(&self) -> AuthContext {
        AuthContext {
            mode: "production_admin".to_string(),
            actor_id: Some(self.actor_id.clone()),
            token_ref: Some(format!(
                "token://{}/{}?aud={}",
                self.token_issuer, self.actor_id, self.token_audience
            )),
            scopes: self.admin_allowed_scopes.clone(),
            source: Some("header".to_string()),
            verified: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicApiAuthError {
    MissingApiKey,
    InvalidApiKey,
    MissingAdminApiKey,
    InvalidAdminApiKey,
}

pub fn authenticate_api_key(header_value: Option<&str>, config: &PublicApiConfig) -> Result<(), PublicApiAuthError> {
    let Some(value) = header_value.map(|v| v.trim()).filter(|v| !v.is_empty()) else {
        return Err(PublicApiAuthError::MissingApiKey);
    };
    if value != config.api_key {
        return Err(PublicApiAuthError::InvalidApiKey);
    }
    Ok(())
}

pub fn authenticate_admin_api_key(
    header_value: Option<&str>,
    config: &PublicApiConfig,
) -> Result<(), PublicApiAuthError> {
    let Some(value) = header_value.map(|v| v.trim()).filter(|v| !v.is_empty()) else {
        return Err(PublicApiAuthError::MissingAdminApiKey);
    };
    if value != config.admin_api_key {
        return Err(PublicApiAuthError::InvalidAdminApiKey);
    }
    Ok(())
}

pub fn sanitize_idempotency_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        .take(64)
        .collect::<String>()
}
