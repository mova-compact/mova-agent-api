# MOVA Agent API Runtime Configuration & Secret Boundary Block V0

Date: 2026-05-23  
Repository: `D:/Projects_MOVA/mova-agent-api`

## Executive summary

Runtime configuration and secret boundary block V0 is complete with explicit adapter-owned runtime config, secret reference contract, redaction rules, and connector credential-reference injection boundaries.

Boundary verdict: `PASS_WITH_WARNINGS`

Warnings:

- secret handling remains reference-based and local/runtime-stubbed
- no live cloud secret manager/provider integration is included
- no live connector credentials are resolved or persisted

## Boundaries introduced

1. Runtime configuration boundary:
   - `RuntimeConfig` with explicit ownership for:
     - `auth`
     - `storage`
     - `connectors`
2. Runtime config loader boundary:
   - `RuntimeConfigLoader` trait
   - `StaticRuntimeConfigLoader` deterministic loader
3. Secret reference contract:
   - `SecretRef` with kinds:
     - `secret_ref`
     - `env_ref`
     - `runtime_secret`
4. Secret-safe redaction rules:
   - JSON payload redaction for secret-like keys
   - text redaction for secret-like error content
5. Connector credential injection boundary:
   - connector execution accepts `credential_refs` only
   - no raw secret values injected into connector executor contract
6. Deterministic config validation failures:
   - invalid runtime/auth/storage/connector config paths are explicit and testable
7. Secret-safe evidence/observation behavior:
   - connector request/auth payloads are redacted before observation/evidence persistence
   - error surfaces redact secret-like message material

## What is explicit and owned now

- runtime/config ownership is adapter/module owned, not transport owned
- HTTP remains adapter-only and uses runtime-configured boundaries
- auth/policy ownership remains unchanged
- connector execution ownership remains unchanged
- storage ownership remains unchanged

## What remains local/test-only

- static runtime config loader
- reference-only secret model (no external resolution)
- deterministic secret redaction and fixture-safe behavior

## Explicitly forbidden (unchanged)

- cloud secret manager integrations (Vault/AWS/GCP/Azure/Cloudflare)
- production runtime/deployment coupling
- live provider credentials
- secret persistence services
- transport-owned secret handling
- orchestration/dynamic routing/autonomous authority

## Promotion path for future live runtime/secret systems

1. add provider-specific runtime/secret adapters behind current traits
2. keep provider coupling isolated from HTTP/request/policy/connectors core
3. preserve reference-based secret contract
4. preserve redaction guarantees in observation/evidence/error surfaces
5. promote deployment/runtime provider semantics explicitly in a separate block

## Schemas/examples/tests added

- schemas:
  - `schemas/runtime_config.schema.json`
  - `schemas/secret_ref.schema.json`
- examples:
  - `examples/runtime_config_minimal.json`
  - `examples/secret_ref_minimal.json`
- tests:
  - `tests/runtime_secret_conformance.rs`
  - secret redaction checks in HTTP/evidence flows

## Validation commands

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`
