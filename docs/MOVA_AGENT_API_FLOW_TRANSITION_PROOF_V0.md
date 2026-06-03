# MOVA Agent API Flow Transition Proof V0

## What changed

- contract run start reads entry step from admitted contract flow
- `ContractRunState` steps are derived from flow steps
- step execution applies `flow.next` transition
- human gate reject/approve use flow transition semantics
- evidence includes transition trace
- second contract with non-fixture step ids proves no `step_001` dependency
- non-fixture gate evidence preserves `alpha_gate`
- executable steps without `operation_id` are rejected
- duplicate step ids are rejected at registration
- missing transition targets are rejected at registration

## Commands run

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`

## Boundary verdict

PASS_WITH_WARNINGS

## Remaining limitations

- V0 supports `default`/`approve`/`reject` outcomes only
- V0 supports next-step and terminal targets only
- no retry/fallback loop yet
- no scheduler
- no dynamic routing
- no autonomous planning
