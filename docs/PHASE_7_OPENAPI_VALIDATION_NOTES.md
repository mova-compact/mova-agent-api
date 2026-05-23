# Phase 7 OpenAPI and Validation Notes

Date: 2026-05-23
Phase: 7 - OpenAPI and validation

## Scope applied

- Added OpenAPI 3.1 draft surface:
  - `GET /capabilities`
  - `POST /actions/validate`
  - `POST /actions/run`
  - `GET /runs/{run_id}`
  - `GET /runs/{run_id}/evidence`
- Added OpenAPI validation script:
  - `scripts/validate-openapi.mjs`
  - `npm run validate:openapi`
- Expanded schema validation coverage to include:
  - `connector_call` example
  - `observation_record` example
- Added aggregate validation command:
  - `npm run validate:all`

## Explicitly out of scope

- No HTTP server implementation
- No runtime orchestration
- No dynamic routing
- No real external connector effects

## Validation surface

- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`
