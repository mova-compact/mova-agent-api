# Phase 9 Run and Evidence Consistency Notes

Date: 2026-05-23
Phase: incremental consistency hardening after HTTP skeleton

## Scope applied

- Strengthened request semantic validation in `src/request/mod.rs`:
  - non-empty critical identifiers
  - exactly one of `action.input_ref` or `action.input_payload`
  - object check for `inputs`
- Unified HTTP error envelope in `src/http/mod.rs`:
  - `error.code`
  - `error.message`
  - `error.details`
- Improved run/evidence consistency in run status responses:
  - `trace_ref`
  - `observation_count`
- Aligned `schemas/action.schema.json` and OpenAPI with exclusive input selector rule.

## Contract impact

- OpenAPI now documents `400`/`404` error responses with a shared error schema.
- Run and run-status responses now include minimal traceability summary fields.

## Boundary checks

- HTTP remains adapter-only.
- Connector behavior remains deterministic and side-effect-free.
- No persistent storage was introduced.
- No orchestration or dynamic routing was introduced.
