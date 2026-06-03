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

## Flow-driven transitions

Contract-run state no longer advances by fixture step ids.
After each step, runtime resolves `flow.steps[].next` using deterministic outcome keys.
Supported V0 outcomes: `default`, `approve`, `reject`.
Supported V0 targets: next step, terminal completed, terminal blocked.

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
