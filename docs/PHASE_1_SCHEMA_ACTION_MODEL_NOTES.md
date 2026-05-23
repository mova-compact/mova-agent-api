# Phase 1 Schema and Action Model Notes

Date: 2026-05-23
Phase: 1 - schema and action model skeleton

## Scope applied

- Product-local schemas were added for request, action, policy admission, connector call, observation record, and evidence response.
- Module skeleton directories were aligned with the V0 spec:
  - `src/request/`
  - `src/policy/`
  - `src/execution/`
  - `src/connectors/`
  - `src/observation/`
  - `src/evidence/`
- Minimal examples were added:
  - `examples/agent_request_minimal.json`
  - `examples/evidence_response_minimal.json`

## Explicitly out of scope

- No execution behavior
- No connector implementation
- No policy logic implementation
- No orchestration or dynamic routing
- No platform-expansion features

## Validation command

Tooling added in this phase:

- `npm install`
- `npm run validate:examples`

This validates:

- `examples/agent_request_minimal.json` against `schemas/request_envelope.schema.json`
- `examples/evidence_response_minimal.json` against `schemas/evidence_response.schema.json`

## Design notes

- The action model enforces `input_ref` or `input_payload` via `anyOf`.
- Admission decisions are constrained to:
  - `allow`
  - `deny`
  - `require_review`
  - `redact`
- The request envelope embeds action shape by schema reference.
- Connector, observation, and evidence models are shape-only and runtime-neutral.
