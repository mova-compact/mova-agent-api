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

## Boundary verdict

PASS_WITH_WARNINGS

## Known limitations

- V0 uses static/fixture contracts.
- No remote contract package loading.
- No scheduler.
- No marketplace.
- No arbitrary provider connectors.
