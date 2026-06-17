# CONTRACT ADMISSION BRIDGE V0

## Purpose

Minimal bridge between MOVA contract package canon and existing `mova-agent-api` execution surface, without introducing orchestration/runtime expansion.

## Added endpoints

- `POST /contracts/register`
- `GET /contracts`
- `GET /contracts/{contract_id}`
- `POST /contracts/{contract_id}/run`
- `POST /contracts/runs/{run_id}/decision`

## Admitted contract shape

Minimal admitted contract record includes:

- `contract_id`
- `execution_type`
- `source_url` (identity only in current V0 bridge)
- `flow` (from `inline_flow_json`)
- optional:
  - `manifest`
  - `policy`
  - `connector_requirements`
  - `evidence_expectations`
  - `open_questions`

## Admission validation (minimal)

Registration validates:

- flow entry exists
- `flow.steps` non-empty
- step ids are unique
- `next` targets are valid (`step` or `terminal`)
- referenced step ids exist
- connector references in flow are consistent with declared `connector_requirements` when provided

## Runtime bridge semantics

- Run by contract id resolves admitted flow.
- Runtime follows `next` transitions deterministically.
- Outcome selection is explicit from request payload:
  - `input_payload.outcomes.{step_id} = outcome`
  - fallback: `default`
- No hidden routing/planning/orchestration added.

## Human gate bridge

When a step is `execution_mode=HUMAN_GATE` or `step_type=human_gate_escalation`:

- run status becomes `waiting_human`
- in-memory continuation record is created
- continuation endpoint accepts:
  - `approve` -> `completed`
  - `reject` -> `stopped_by_human_gate`

## Evidence continuity

Bridge preserves:

- `run_id`
- `trace_ref`
- flow step trace (in observation result)
- terminal status in run/evidence surfaces

Evidence is exposed through existing:

- `GET /runs/{run_id}`
- `GET /runs/{run_id}/evidence`

## Proof package

Canonical proof contract used:

- `fixture_contract_admission_bridge_v0`

Proven operations:

- register contract
- list contracts
- inspect contract
- run by `contract_id` (success path)
- run by `contract_id` (human gate path)
- human decision continuation (`reject`)
- evidence visibility through existing run/evidence API

## Intentionally unsupported in this bridge

- source_url remote fetch/compile execution (source_url is accepted as identity metadata only)
- durable continuation state for human gate (current continuation is in-memory)
- dynamic workflow planning
- orchestration engine
- recursive contract calls

## Runtime limitations

- Continuation state is process-local in-memory.
- Restart drops waiting human gate runs.
- Source-url-only registration cannot execute until inline flow is provided.

## Proof commands

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`
- `cargo build --target wasm32-unknown-unknown --features worker`

