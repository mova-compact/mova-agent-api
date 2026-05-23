# Phase 8 HTTP Transport Skeleton Notes

Date: 2026-05-23
Phase: 8 - HTTP transport skeleton

## Scope applied

- Added HTTP adapter module:
  - `src/http/mod.rs`
- Added route skeletons:
  - `GET /capabilities`
  - `POST /actions/validate`
  - `POST /actions/run`
  - `GET /runs/{run_id}`
  - `GET /runs/{run_id}/evidence`
- Added deterministic in-memory run state for route tests only.
- Added transport integration tests:
  - `tests/http_transport.rs`

## Adapter boundary

- HTTP layer is adapter-only.
- Business structures remain in existing modules:
  - `request`
  - `policy`
  - `execution`
  - `connectors`
  - `observation`
  - `evidence`

## Explicit non-expansion checks

- No real connector side effects were added.
- No persistent storage was added.
- No orchestration was added.
- No dynamic routing was added.

## Contract alignment

- OpenAPI remains the public contract:
  - `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml`
