# MOVA Agent API Auth Promotion Block V0

Date: 2026-05-23  
Repository: `D:/Projects_MOVA/mova-agent-api`  
Block scope: offline, provider-agnostic auth promotion surface

## Executive summary

The AUTH PROMOTION BLOCK V0 is complete inside current boundaries.

Included:

1. production auth contract layer
2. explicit trust config model
3. pluggable verifier adapter boundary
4. offline provider stub verifier boundary
5. auth verifier conformance tests
6. policy enforcement tests
7. HTTP pass-through tests
8. OpenAPI/docs/examples alignment

Boundary verdict: `PASS_WITH_WARNINGS`

Warnings:

- provider integration remains intentionally offline/stub-only
- no external IdP/JWT/OAuth network verification is implemented
- no durable identity/session state is implemented

## What the auth block contains

- `src/auth/mod.rs`:
  - provider-agnostic `AuthVerifier` contract
  - `AuthTrustConfig` with:
    - `verifier_kind`
    - `trusted_issuers`
    - `trusted_audiences`
    - `allowed_scopes`
    - `offline_stub_fixtures`
  - `deterministic_local` verifier
  - `offline_provider_stub` verifier
  - config validation and deterministic invalid-config fallback
- `src/policy/mod.rs`:
  - policy-owned final authorization decisions
  - production mode outcomes:
    - `auth_unverified`
    - `scope_denied`
    - `authorized`
  - auth verification trace in policy constraints
- `src/http/mod.rs`:
  - auth metadata extraction/pass-through only
  - unified `authorization_failed` error contract
  - no transport-owned authorization

## Contractually production-ready pieces

- policy ownership of authorization
- explicit trust configuration contract
- deterministic token reference contract:
  - `token://<issuer>/<subject>?aud=<audience>`
- deterministic issuer/audience/scope decisioning
- unified authorization error shape (`403`, `authorization_failed`)
- conformance tests for verifier contract behavior

## Still offline/stubbed

- no external provider SDK integration
- no network calls
- no real JWT/OAuth verification
- no external key discovery
- no user/session persistence
- offline provider fixture responses only

## Explicitly forbidden (unchanged)

- real external IdP integration without promotion
- network-bound auth verification
- durable identity/session platform
- RBAC platform expansion
- transport-owned authorization
- orchestration/dynamic routing
- autonomous agent authority

## Future provider integration promotion path

1. add a new provider adapter implementing `AuthVerifier`
2. keep provider coupling isolated in adapter-only module
3. keep policy as final allow/deny owner
4. preserve `AuthTrustConfig` as explicit source of trust rules
5. add provider conformance tests matching existing verifier contract suite
6. gate any network/dependency additions via explicit promotion decision

## Validation commands

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`

## Current block evidence

- Trust config schema: `schemas/auth_trust_config.schema.json`
- Trust config example: `examples/auth_trust_config_minimal.json`
- Verifier conformance tests: `tests/auth_verifier_conformance.rs`
- HTTP auth behavior tests: `tests/http_transport.rs`
- Policy auth behavior tests: `src/policy/mod.rs` tests
