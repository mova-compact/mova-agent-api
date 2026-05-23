# MOVA Agent API Deployment & Runtime Provider Integration Gate V0

Date: 2026-05-23  
Repository: `D:/Projects_MOVA/mova-agent-api`

## Executive summary

Deployment/runtime provider integration gate V0 is complete with explicit provider/runtime boundaries, environment-based runtime loading, secret resolver contracts, and secret-safe runtime resolution behavior.

Boundary verdict: `PASS_WITH_WARNINGS`

Warnings:

- runtime provider integration remains local/test-only (`local_env`, static/default runtime config)
- no live cloud/runtime/deployment coupling is enabled
- no live secret providers or live connector credentials are integrated

## Boundaries now in place

1. Runtime provider adapter boundary:
   - `RuntimeProvider` trait
   - `RuntimeProviderCapabilities` model
   - `LocalEnvRuntimeProvider` (local env-based provider)
   - `FailingRuntimeProvider` (deterministic failure path)
2. Environment/runtime config loading boundary:
   - `RuntimeConfigLoader` + `StaticRuntimeConfigLoader`
   - provider-based config load (`load_runtime_config`)
3. Secret resolver boundary:
   - `SecretResolver` trait
   - `LocalEnvSecretResolver` for local env/runtime references
4. Runtime/provider validation semantics:
   - deterministic config validation across auth/storage/connectors
   - deterministic invalid-provider/config failures
5. Secret-safe runtime resolution flow:
   - connector credential injection uses secret references only
   - secret resolution failures mapped deterministically
   - evidence/observation/error payloads remain redacted

## What provider/runtime behavior is allowed in V0

- local env/runtime config overrides via `LocalEnvRuntimeProvider`
- local env/runtime secret reference resolution via `LocalEnvSecretResolver`
- provider capability inspection through `/capabilities` runtime provider section

## What remains local/test-only

- provider/runtime integration is non-deployment-coupled
- no live cloud/runtime orchestration bindings
- no live external secret manager integrations

## Explicitly forbidden (unchanged)

- Cloudflare/Docker/Kubernetes deployment ownership in core
- cloud secret manager coupling (Vault/AWS/GCP/Azure/Cloudflare)
- production deployment automation ownership
- live provider credentials
- provider-owned authorization/persistence
- orchestration/dynamic routing/autonomous authority

## Promotion path for future real deployment/runtime integration

1. add provider-specific runtime adapters implementing `RuntimeProvider`
2. add provider-specific secret resolvers implementing `SecretResolver`
3. keep provider coupling isolated from HTTP/policy/connector/storage ownership
4. preserve secret reference + redaction guarantees
5. gate deployment/runtime semantics in dedicated explicit promotion blocks

## Schemas/examples/tests added

- schemas:
  - `schemas/runtime_provider_capabilities.schema.json`
  - `schemas/runtime_config.schema.json`
  - `schemas/secret_ref.schema.json`
- examples:
  - `examples/runtime_provider_capabilities_minimal.json`
  - `examples/runtime_config_minimal.json`
  - `examples/secret_ref_minimal.json`
- tests:
  - `tests/runtime_secret_conformance.rs`
  - expanded HTTP/connector/vertical tests for runtime/secret boundary behavior

## Validation commands

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`
