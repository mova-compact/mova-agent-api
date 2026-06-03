# MOVA Agent API

`mova-agent-api` is the clean product repository for MOVA Agent API V0.

It is a single API product for controlled agent actions with one future source of truth inside this repository.

Canonical execution path:

- agent request
- action model
- policy admission
- flat execution
- connector call
- observation write
- evidence response

Controlled contract-run corridor:

- agent starts contract
- contract run state
- next allowed step
- operation admission
- guarded connector execution
- observation
- evidence

## What it is

- a product boundary for controlled agent actions
- a clean API surface for agent requests, admission, execution, observations, and evidence
- the future source of truth for MOVA Agent API V0

## What it is not

- not the old `mova-api` runtime workspace
- not the old `mova-mcp` flat runner
- not a proxy or marketplace layer
- not a visual builder
- not an autonomous agent platform
- not a rewrite of `mova-spec` or `mova-contract-spec`

## Current status

- V0 planning / skeleton
- flat action path and controlled contract-run corridor available as local V0 behavior
- contract-run connector_action steps execute through guarded `ConnectorExecutor` boundary
- no legacy implementation copied here

## Source of truth rule

- `mova-agent-api` is the only active source of truth for the new product
- `mova-spec` remains the upstream language canon
- `mova-contract-spec` remains the upstream contract package canon
- old runtime and MCP repositories are reference-only

## Initial repository layout

- `docs/` - product definition and specifications
- `src/` - future implementation boundary
- `schemas/` - product-local schemas and contract shapes
- `tests/` - validation and smoke coverage
- `examples/` - minimal usage examples

## Reference material

- `_mova_meta/docs/MOVA_AGENT_API_EXTRACTION_AUDIT_V0.md`

## Documentation quick links

- `docs/MOVA_AGENT_API_V0_HANDBOOK.md` - operator/user troubleshooting handbook
- `docs/MOVA_AGENT_API_DOCS_INDEX_V0.md` - full docs map (user-facing vs internal)
- `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml` - public API contract
- `docs/MOVA_AGENT_API_CONTRACT_RUN_CORRIDOR_V0.md` - contract-run corridor boundary and limitation summary
- `docs/MOVA_AGENT_API_NO_BYPASS_INVARIANTS_V0.md` - invariant list for non-bypass execution
- `schemas/` + `examples/` - schema and payload references
- `scripts/smoke_public_api.ps1` - public deployment smoke script
- `scripts/smoke_contract_run_api.ps1` - contract-run corridor smoke script

## Controlled contract-run corridor

`/actions/run` remains the low-level single-action execution path.

The contract-run API is the product-level corridor where the contract-run state owns step order and the agent can execute only the current admitted operation. This layer does not add cognition, dynamic routing, or autonomous orchestration.

Product corridor routes use `/contracts/{contract_id}/runs` and `/contract-runs/{run_id}/...`.
Legacy `/contracts/{contract_id}/run` remains compatibility-only and must not be extended for new product behavior.
