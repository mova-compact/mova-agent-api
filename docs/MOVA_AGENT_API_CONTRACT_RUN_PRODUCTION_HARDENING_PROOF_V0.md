# MOVA Agent API Contract Run Production Hardening Proof V0

## What changed

- removed fixture step id leakage from gate evidence
- removed implicit `operation_id` fallback for executable steps
- added contract-run flow validation before registration and run start
- added duplicate step, missing entry, and missing transition target validation
- added transition failure evidence
- removed `resolved_url` from public evidence
- clarified `409` transition failure vs `502` connector failure boundary

## Commands run

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`

## Boundary verdict

PASS_WITH_WARNINGS

## Remaining limitations

- V0 supports `default`/`approve`/`reject`/`error` transition keys only
- V0 supports step and terminal transition targets only
- no retry/fallback loops
- no scheduler
- no dynamic routing
- no autonomous planning
- no provider marketplace
