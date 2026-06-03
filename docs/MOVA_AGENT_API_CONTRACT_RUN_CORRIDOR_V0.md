# MOVA Agent API Contract Run Corridor V0

## What changed

- Added product-level contract-run API over existing action-run API.
- Added next-step ownership by runtime state.
- Added `OperationAdmission`.
- Added `HumanGate` API.
- Added contract-run evidence.

## What remains unchanged

- `/actions/run` remains flat single-action path.
- No autonomous orchestration.
- No dynamic routing.
- No arbitrary connector discovery.
- No raw URL execution.
- No production connector SDK integrations.

## Real guarded connector execution

Contract-run step execution no longer fabricates connector results for `connector_action` steps.
For `connector_action` steps, execution builds `ConnectorExecutionRequest` from `OperationAdmission` and contract step metadata only.
Client payload cannot override `connector_id`, `endpoint_ref`, `method`, `target_url`, or `side_effect_intent`.
Contract-run `connector_action` V0 allows `side_effect_intent`: `none`, `local_only`, `external_network`.
`destructive` remains denied.

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
- No arbitrary provider connectors.
- Production provider SDK integrations remain forbidden.
