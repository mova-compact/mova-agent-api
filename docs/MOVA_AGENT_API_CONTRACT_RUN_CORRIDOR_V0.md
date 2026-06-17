# MOVA Agent API Contract Run Corridor V0

## What changed

- Added product-level contract-run API over existing action-run API.
- Added next-step ownership by runtime state.
- Added `OperationAdmission`.
- Added `HumanGate` API.
- Added contract-run evidence.
- Public contract-run runtime now requires `X-MOVA-API-KEY`.
- Public contract-run runtime now assigns server-owned `tenant_id`.
- Public contract-run start and step execution now support `Idempotency-Key`.

## What remains unchanged

- `/actions/run` remains flat single-action path.
- No autonomous orchestration.
- No dynamic routing.
- No arbitrary connector discovery.
- No raw URL execution.
- No client-selected provider connectors or secret-bound targets.

## Real guarded connector execution

Contract-run step execution no longer fabricates connector results for `connector_action` steps.
For `connector_action` steps, execution builds `ConnectorExecutionRequest` from `OperationAdmission` and contract step metadata only.
Client payload cannot override `connector_id`, `connector_ref`, `endpoint_ref`, `method`, `provider`, `operation`, `target_url`, or `side_effect_intent`.
`/actions/run` and contract-run corridor use separate endpoint policy scopes:
- `webhook_site_test` -> `actions.run`
- `webhook_site_contract_run_test` -> `contracts.run`
Contract-run fixture `fixture_contract_run_alpha_v0` uses `webhook_site_contract_run_test`.
Contract-run `connector_action` V0 allows `side_effect_intent`: `none`, `local_only`, `external_network`.
`destructive` remains denied.

## Provider connector proxy registry

Contract-run corridor now also supports controlled provider connectors through registry resolution:
- contract step uses `connector.name = provider.connector.v1`
- contract step references only `connector_ref`
- runtime resolves `connector_ref` via `MOVA_PROVIDER_CONNECTOR_REGISTRY_JSON`
- provider dispatcher calls the provider adapter inside connector layer
- public evidence stays summary-only

Telegram `send_message` is the first provider adapter behind this registry.
It is not a contract-run runtime special case and it is not the connector architecture itself.

Built-in demo contract:
- `fixture_provider_connector_proxy_v0`

Built-in registry demo target:
- `telegram.fixture_primary_channel` -> provider `telegram`, operation `send_message`, scope `contracts.run`

## Public runtime boundary

Public V0 runtime surface is limited to:
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

Release hardening rules:
- Missing `X-MOVA-API-KEY` returns `401`.
- Invalid `X-MOVA-API-KEY` returns `403`.
- Public client cannot provide `auth_context`.
- Public client cannot set `context.tenant_id`.
- Runtime injects server-owned `tenant_id` into run state and evidence.

Replay rules:
- Same `Idempotency-Key` on start replays the same run result.
- Same `Idempotency-Key` on completed step execution replays the same step result.
- Different `Idempotency-Key` after completed step returns `409`.

## Flow-driven transitions

Contract-run state no longer advances by fixture step ids.
After each step, runtime resolves `flow.steps[].next` using deterministic outcome keys.
Supported V0 outcomes: `default`, `approve`, `reject`, `error`.
Supported V0 targets: next step, terminal completed, terminal blocked, terminal failed.

## Production-grade V0 hardening

- flow validation runs before contract admission and before run start
- executable steps require explicit `operation_id`
- no implicit fixture operation fallback remains
- gate evidence preserves actual `step_id` and `requested_operation_id`
- transition failures are recorded before error response
- contract-run evidence includes server-owned `tenant_id`
- contract-run evidence includes last effective `idempotency_key`
- public evidence exposes `endpoint_ref` but not resolved provider URL
- transition failures use `409`, connector runtime failures use `502`

## Boundary verdict

PASS_WITH_WARNINGS

## Known limitations

- V0 uses static/fixture contracts.
- Generic flow-to-step conversion is limited to current V0 connector shape.
- No remote contract package loading.
- No scheduler.
- No marketplace.
- No arbitrary provider connectors from client payload.
- Provider adapters remain controlled runtime integrations only.
