# MOVA Agent API Merge + Deploy Evidence V0

## Merge
- source branch: `feat/contract-run-corridor`
- target branch: `main`
- merged commit / PR: `271719b8edc1c6f8aaaf7a1d78bed483d49767a0` (`271719b fix: allow local-only contract-run side effects`)
- merge method: `git merge --ff-only feat/contract-run-corridor`
- date/time: `2026-06-03 10:24:32 +02:00`

## Pre-merge validation
- `cargo test -j 1`: PASS
- `NODE_OPTIONS=--max-old-space-size=8192 npm run validate:examples`: PASS
- `npm run validate:openapi`: PASS
- `NODE_OPTIONS=--max-old-space-size=8192 npm run validate:all`: PASS

## Post-merge validation
- `cargo test -j 1`: PASS
- `NODE_OPTIONS=--max-old-space-size=8192 npm run validate:examples`: PASS
- `npm run validate:openapi`: PASS
- `NODE_OPTIONS=--max-old-space-size=8192 npm run validate:all`: PASS

## Cloudflare deploy
- worker name: `mova-agent-api-v0`
- environment: default `workers.dev`
- deploy command: `npx wrangler deploy`
- deployed URL: `https://mova-agent-api-v0.s-myasoedov81.workers.dev`
- deploy status: PASS
- deployed version id: `9171cd30-3328-4b89-aba1-ad666e6feead`

## Post-deploy smoke
- `/health`: PASS
- `/ready`: PASS
- `/capabilities`: PASS
- `POST /actions/validate` with `examples/agent_request_minimal.json`: PASS (`200`, `{"valid":true}`)
- `POST /actions/run` with `examples/agent_request_minimal.json`: WARN (`403`, `connector_execution_failed`, `connector_id is not allowed`)
- `contract-run smoke`: WARN (`POST /contracts/daily_owner_report_v0/runs` returned `404`)
- notes:
  live Worker was updated successfully, but public `capabilities` still exposes only the flat action-path projection.
  contract-run corridor routes are not available on the deployed Worker surface in this environment.
  repo script `scripts/smoke_contract_run_api.ps1` was not used as authoritative proof because it currently contains a PowerShell parser bug before request execution.

## Boundary verdict
`PASS_WITH_WARNINGS`

## Known limitations
- destructive `side_effect_intent` remains forbidden
- no scheduler
- no marketplace
- no dynamic routing
- no autonomous planning
- no arbitrary provider SDK integrations
- deployed Worker surface does not currently expose contract-run corridor routes
