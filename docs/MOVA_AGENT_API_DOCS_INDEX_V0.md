# MOVA Agent API V0 Docs Index

## User / Operator quick docs

- `docs/MOVA_AGENT_API_V0_HANDBOOK.md`
  - first-line troubleshooting for common operational/API issues.
- `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml`
  - public route and error contract.
- `docs/MOVA_AGENT_API_CONTRACT_RUN_CORRIDOR_V0.md`
  - contract-run corridor summary and V0 limitation set.
- `docs/MOVA_AGENT_API_CONTRACT_RUN_REAL_EXECUTION_PROOF_V0.md`
  - proof that connector_action steps now execute through guarded connector boundary.
- `docs/MOVA_AGENT_API_FLOW_TRANSITION_PROOF_V0.md`
  - proof that contract-run transitions are now flow-driven instead of fixture-id-driven.
- `docs/MOVA_AGENT_API_CONTRACT_RUN_PRODUCTION_HARDENING_PROOF_V0.md`
  - proof that contract-run corridor hardening closes remaining V0 boundary leaks.
- `docs/MOVA_AGENT_API_NO_BYPASS_INVARIANTS_V0.md`
  - invariant list for controlled execution without bypass paths.
- `schemas/*.schema.json`
  - payload/contract validation source.
- `examples/*.json`
  - minimal valid request/response examples.
- `scripts/smoke_public_api.ps1`
  - repeatable live smoke against deployed Worker URL.

## Internal proof / freeze docs

- `docs/MOVA_AGENT_API_V0_FREEZE_SNAPSHOT.md`
- `docs/MOVA_AGENT_API_V0_CORE_READINESS_FREEZE.md`
- `docs/MOVA_AGENT_API_V0_OPERATIONAL_HARDENING_BLOCK.md`
- `docs/MOVA_AGENT_API_CLOUDFLARE_DEPLOYMENT_PROOF_V0.md`
- `docs/MOVA_AGENT_API_CLOUDFLARE_PERSISTENCE_PROVIDER_V0.md`
- `docs/MOVA_AGENT_API_WEBHOOK_SITE_CONNECTOR_PROVIDER_V0.md`
- `docs/MOVA_AGENT_API_UNIVERSAL_HTTP_CONNECTOR_V0.md`
- `docs/MOVA_AGENT_API_ENDPOINT_REGISTRY_GOVERNANCE_V0.md`

These are implementation/proof records, not lightweight user troubleshooting guides.

## Product definition / architecture docs

- `docs/MOVA_AGENT_API_TZ_V0.md`
- `docs/MOVA_AGENT_API_SPEC_V0.md`
- `docs/MOVA_AGENT_API_ROADMAP_V0.md`
- `docs/MOVA_AGENT_API_RUNTIME_AUTHORITY_BOUNDARY_V0.md`
- `docs/MOVA_AGENT_API_CONTRACT_RUN_CORRIDOR_V0.md`
- `docs/MOVA_AGENT_API_CONTRACT_RUN_REAL_EXECUTION_PROOF_V0.md`
- `docs/MOVA_AGENT_API_FLOW_TRANSITION_PROOF_V0.md`
- `docs/MOVA_AGENT_API_CONTRACT_RUN_PRODUCTION_HARDENING_PROOF_V0.md`
- `docs/MOVA_AGENT_API_NO_BYPASS_INVARIANTS_V0.md`
- `docs/MOVA_AGENT_API_AUTH_BOUNDARY_V0.md`
- `docs/MOVA_AGENT_API_PRODUCTION_AUTH_CONTRACT_V0.md`
- `docs/MOVA_AGENT_API_AUTH_VERIFIER_ADAPTER_CONTRACT_V0.md`
- `docs/MOVA_AGENT_API_PERSISTENCE_PROMOTION_BLOCK_V0.md`
- `docs/MOVA_AGENT_API_CONNECTOR_EXECUTION_PROMOTION_BLOCK_V0.md`
- `docs/MOVA_AGENT_API_RUNTIME_SECRET_BOUNDARY_BLOCK_V0.md`
- `docs/MOVA_AGENT_API_DEPLOYMENT_RUNTIME_GATE_V0.md`

## Validation entry points

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`
- `cargo build --target wasm32-unknown-unknown --features worker`
- `npm run smoke:public`
- `powershell -ExecutionPolicy Bypass -File scripts/smoke_contract_run_api.ps1`

## Mapping for known issue classes

- `KV binding issue` -> handbook section "KV binding issue"
- `run_not_found issue` -> handbook section "run_not_found"
- `Webhook.site not receiving request` -> handbook section "Webhook.site not receiving request"
- `validation errors` -> handbook section "validation errors"
- `auth/scope denied` -> handbook section "auth/scope denied"
- `connector guard denied` -> handbook section "connector guard denied"
- `storage_unavailable` -> handbook section "storage_unavailable"
- `request_too_large` -> handbook section "request_too_large"
- `rate limited` -> handbook section "rate_limited"
