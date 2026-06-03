# MOVA Agent API Public Runtime Parity V0.1

## Purpose

Close release risk `R6 Runtime Divergence Risk`.

This proof records the parity boundary for the v0.1 public runtime:

- native public router
- Cloudflare Worker public adapter
- OpenAPI public contract

## Public route set

The public v0.1 surface is exactly:

- `GET /health`
- `GET /ready`
- `GET /capabilities`
- `POST /contracts/{contract_id}/runs`
- `GET /contract-runs/{run_id}`
- `GET /contract-runs/{run_id}/next`
- `POST /contract-runs/{run_id}/steps/{step_id}/execute`
- `GET /contract-runs/{run_id}/gates/current`
- `POST /contract-runs/{run_id}/gates/{gate_id}/resolve`
- `GET /contract-runs/{run_id}/evidence`

Anything else is internal or lab.

## Native parity proof

Native release router:

- `public_router()` in `src/http/mod.rs`

Proof coverage:

- `public_contract_run_requires_api_key`
- `public_contract_run_status_requires_api_key_before_not_found`
- `public_contract_run_rejects_invalid_api_key`
- `public_router_does_not_expose_contract_registration_route`
- `public_router_does_not_expose_lab_or_internal_routes`

These tests prove:

- missing API key returns `401`
- invalid API key returns `403`
- public router fails closed on lab/internal routes with `404`

## Worker parity proof

Worker release matcher:

- `src/worker_surface.rs`
- `src/worker_adapter.rs`

Proof coverage:

- worker route matcher tests for every contract-run public route
- live Worker smoke after deploy

Worker adapter fail-closed rule:

- unknown non-public path returns `404 route_not_found`

## Shared behavior proof

Both native public router and Worker public adapter enforce:

- `X-MOVA-API-KEY`
- server-owned `tenant_id`
- public `Idempotency-Key` replay semantics
- contract-run step override denial
- same public connector admission shape

Observed parity points:

- public start returns `202`
- public status returns `200`
- public next returns `200`
- public execute returns:
  - `202` on accepted step execution
  - `409 step_already_executed` on replay with a different key after completion
  - `503 connector_execution_failed` with connector detail `connector_secret_missing` when Telegram secrets are absent
- public evidence returns `200`

## OpenAPI parity

`docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml` is release-cleaned to the public route set only.

Public contract-run routes explicitly document:

- `401` missing API key
- `403` invalid API key
- `409` replay/state conflict where applicable
- public response shapes with `tenant_id`
- public evidence shape with `evidence.idempotency_key`

## Verdict

`R6` is `CLOSED` for the v0.1 public runtime boundary.

Known remaining blocker outside `R6`:

- `R7` is still open until real Telegram secrets are configured and live provider delivery succeeds.
